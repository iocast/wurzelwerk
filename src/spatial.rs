//! Spatial index (uniform grid) for viewport culling + hit testing,
//! and convex hulls for the clan backgrounds.

use std::collections::HashMap;

use iced::{Point, Rectangle};

use crate::model::{Clan, Id, NODE_H, NODE_W, Person};

/// Grid cell size in world units.
pub const CELL: f32 = 512.0;

type Key = (i32, i32);

fn key(p: Point) -> Key {
    ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
}

/// Nodes are bucketed by their top-left corner.
#[derive(Debug, Default)]
pub struct Grid {
    cells: HashMap<Key, Vec<u32>>,
}

impl Grid {
    pub fn build(people: &[Person]) -> Self {
        let mut grid = Self::default();
        for (i, p) in people.iter().enumerate() {
            grid.insert(i as u32, Point::new(p.x, p.y));
        }
        grid
    }

    pub fn insert(&mut self, i: u32, at: Point) {
        self.cells.entry(key(at)).or_default().push(i);
    }

    pub fn move_item(&mut self, i: u32, from: Point, to: Point) {
        let (old, new) = (key(from), key(to));
        if old == new {
            return;
        }
        let mut now_empty = false;
        if let Some(bucket) = self.cells.get_mut(&old) {
            if let Some(pos) = bucket.iter().position(|&x| x == i) {
                bucket.swap_remove(pos);
            }
            now_empty = bucket.is_empty();
        }
        if now_empty {
            self.cells.remove(&old);
        }
        self.cells.entry(new).or_default().push(i);
    }

    /// Calls `f` for every node whose top-left lies in a cell touching `r`
    /// (`r` is expanded by the node size so partially visible nodes are included).
    pub fn query(&self, r: Rectangle, mut f: impl FnMut(u32)) {
        let (x0, y0) = key(Point::new(r.x - NODE_W, r.y - NODE_H));
        let (x1, y1) = key(Point::new(r.x + r.width, r.y + r.height));
        let span = (x1 - x0 + 1) as i64 * (y1 - y0 + 1) as i64;

        if span > self.cells.len() as i64 {
            // zoomed far out: walk the occupied cells instead of the empty range
            for (&(cx, cy), bucket) in &self.cells {
                if cx >= x0 && cx <= x1 && cy >= y0 && cy <= y1 {
                    bucket.iter().for_each(|&i| f(i));
                }
            }
        } else {
            for cy in y0..=y1 {
                for cx in x0..=x1 {
                    if let Some(bucket) = self.cells.get(&(cx, cy)) {
                        bucket.iter().for_each(|&i| f(i));
                    }
                }
            }
        }
    }

    /// Occupied cells intersecting `r` with their nodes (overview rendering).
    pub fn cells_in(&self, r: Rectangle, mut f: impl FnMut(Rectangle, &[u32])) {
        for (&(cx, cy), bucket) in &self.cells {
            let cell = Rectangle {
                x: cx as f32 * CELL,
                y: cy as f32 * CELL,
                width: CELL,
                height: CELL,
            };
            if cell.intersects(&r) {
                f(cell, bucket);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Hull {
    pub points: Vec<Point>,
    pub bbox: Rectangle,
    /// People inside the hull (shown on the clan label).
    pub members: usize,
}

const HULL_PAD: f32 = 22.0;

fn corners(p: &Person, out: &mut Vec<Point>) {
    out.push(Point::new(p.x - HULL_PAD, p.y - HULL_PAD));
    out.push(Point::new(p.x + NODE_W + HULL_PAD, p.y - HULL_PAD));
    out.push(Point::new(p.x + NODE_W + HULL_PAD, p.y + NODE_H + HULL_PAD));
    out.push(Point::new(p.x - HULL_PAD, p.y + NODE_H + HULL_PAD));
}

/// One pass over the people, one hull per clan.
pub fn all_hulls<'a>(
    people: impl IntoIterator<Item = &'a Person>,
    clans: &[Clan],
) -> HashMap<Id, Hull> {
    let mut points: HashMap<&str, Vec<Point>> = HashMap::with_capacity(clans.len());
    for p in people {
        for c in &p.clans {
            corners(p, points.entry(c.as_str()).or_default());
        }
    }
    clans
        .iter()
        .filter_map(|c| {
            let pts = points.remove(c.id.as_str())?;
            Some((c.id.clone(), hull_of(pts)))
        })
        .collect()
}

pub fn clan_hull<'a>(people: impl IntoIterator<Item = &'a Person>, clan: &str) -> Option<Hull> {
    let mut pts = Vec::new();
    for p in people
        .into_iter()
        .filter(|p| p.clans.iter().any(|c| c == clan))
    {
        corners(p, &mut pts);
    }
    (!pts.is_empty()).then(|| hull_of(pts))
}

fn hull_of(pts: Vec<Point>) -> Hull {
    let members = pts.len() / 4;
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in &pts {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    Hull {
        points: convex_hull(pts),
        bbox: Rectangle {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        },
        members,
    }
}

/// Andrew's monotone chain, O(n log n).
pub fn convex_hull(mut pts: Vec<Point>) -> Vec<Point> {
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup_by(|a, b| a.x == b.x && a.y == b.y);
    if pts.len() < 3 {
        return pts;
    }
    let cross =
        |o: Point, a: Point, b: Point| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);

    let mut lower: Vec<Point> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<Point> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}
