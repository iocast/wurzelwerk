//! Orthogonal line routing for the board.
//!
//! * **Router** - A* over a sparse orthogonal visibility graph: the Hanan grid spanned by
//!   the card outlines inside a search window (the approach of libavoid). Search states
//!   carry the travel axis, so the cost is exactly `length + bends × penalty`, and all
//!   candidate ports of both cards are searched at once (multi-source / multi-target)
//!   instead of running one search per port pair.
//! * **Obstacle index** - inflated card rectangles in a bucket grid; a search only
//!   rasterises the cards inside its window.
//! * **Route cache** - [`Routes`] keeps every line until something it depends on changes.
//!   Each route remembers the windows it searched, so moving a card only re-routes the
//!   lines whose windows touch the card's old or new position.
//! * **Nudging** - parallel segments are centred in their free channel and overlapping
//!   segments of different lines are spread apart; lines stay orthogonal and clear of cards.
//! * **Labels** - automatic line labels take the first candidate position that overlaps
//!   neither a card nor another label (greedy placement).

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use iced::{Point, Rectangle};

use crate::model::{Data, Id, Link, NODE_H, NODE_W, Person};

pub const ROUTE_CLEARANCE: f32 = 10.0;
const ROUTE_BEND_COST: f32 = 24.0;
/// Ports stay this far away from the card corners.
const PORT_INSET: f32 = 4.0;
const EPS: f32 = 0.01;
/// Upper bound for the A* state count of a single search (two states per grid node).
const MAX_SEARCH_STATES: usize = 1 << 22;
const BUCKET: f32 = 256.0;
/// With more moved cards than this every line is re-routed (cheaper than window tests).
const MAX_DIRTY_CARDS: usize = 32;

/// Minimum distance between a nudged line and a card.
const NUDGE_MARGIN: f32 = 5.0;
/// Distance between parallel lines that share a channel.
const NUDGE_SPACING: f32 = 6.0;
/// Minimum length of a segment that is shortened by nudging its neighbour.
const NUDGE_MIN_LEG: f32 = 6.0;
/// How far a U-shaped segment may move away from its legs (room to spread a bundle).
const NUDGE_U_SLACK: f32 = NUDGE_SPACING * 2.0;
/// How far to look for the walls of a channel.
const NUDGE_REACH: f32 = 300.0;
/// Channels up to this width get their lines centred (gaps between rows / columns).
const NUDGE_CENTRE_CHANNEL: f32 = 240.0;

/// Line label font size in world units (labels are drawn at `11 × min(zoom, 1.4)` px).
pub const LABEL_TEXT: f32 = 11.0;
const LABEL_HALF_HEIGHT: f32 = 8.0;
const LABEL_BUCKET: f32 = 128.0;

// ---------------------------------------------------------------------------
// geometry helpers
// ---------------------------------------------------------------------------

fn right(r: &Rectangle) -> f32 {
    r.x + r.width
}

fn bottom(r: &Rectangle) -> f32 {
    r.y + r.height
}

/// Closed intersection test (touching rectangles intersect).
fn touches(a: &Rectangle, b: &Rectangle) -> bool {
    a.x <= right(b) && b.x <= right(a) && a.y <= bottom(b) && b.y <= bottom(a)
}

/// Open intersection test (touching rectangles do not overlap).
fn overlaps(a: &Rectangle, b: &Rectangle) -> bool {
    a.x < right(b) - EPS && b.x < right(a) - EPS && a.y < bottom(b) - EPS && b.y < bottom(a) - EPS
}

fn grow(r: Rectangle, dx: f32, dy: f32) -> Rectangle {
    Rectangle {
        x: r.x - dx,
        y: r.y - dy,
        width: r.width + 2.0 * dx,
        height: r.height + 2.0 * dy,
    }
}

fn union(a: Rectangle, b: Rectangle) -> Rectangle {
    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
    let (x1, y1) = (right(&a).max(right(&b)), bottom(&a).max(bottom(&b)));
    Rectangle {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    }
}

fn bounds_of(points: impl IntoIterator<Item = Point>) -> Option<Rectangle> {
    points.into_iter().fold(None, |bounds, p| {
        let r = Rectangle {
            x: p.x,
            y: p.y,
            width: 0.0,
            height: 0.0,
        };
        Some(bounds.map_or(r, |b| union(b, r)))
    })
}

fn card(person: &Person) -> Rectangle {
    Rectangle {
        x: person.x,
        y: person.y,
        width: NODE_W,
        height: NODE_H,
    }
}

fn card_at(at: Point) -> Rectangle {
    Rectangle {
        x: at.x,
        y: at.y,
        width: NODE_W,
        height: NODE_H,
    }
}

fn centre(person: &Person) -> Point {
    Point::new(person.x + NODE_W / 2.0, person.y + NODE_H / 2.0)
}

/// Card grown by the routing clearance: lines may run on the outline, never inside.
pub fn inflated(person: &Person) -> Rectangle {
    grow(card(person), ROUTE_CLEARANCE, ROUTE_CLEARANCE)
}

pub fn point_inside(point: Point, rect: Rectangle) -> bool {
    point.x > rect.x + EPS
        && point.x < right(&rect) - EPS
        && point.y > rect.y + EPS
        && point.y < bottom(&rect) - EPS
}

pub fn segment_clear(start: Point, end: Point, obstacles: &[Rectangle]) -> bool {
    obstacles.iter().all(|rect| {
        if (start.y - end.y).abs() < EPS {
            !(start.y > rect.y + EPS
                && start.y < bottom(rect) - EPS
                && start.x.min(end.x) < right(rect) - EPS
                && start.x.max(end.x) > rect.x + EPS)
        } else if (start.x - end.x).abs() < EPS {
            !(start.x > rect.x + EPS
                && start.x < right(rect) - EPS
                && start.y.min(end.y) < bottom(rect) - EPS
                && start.y.max(end.y) > rect.y + EPS)
        } else {
            false
        }
    })
}

/// Drops duplicate points and the middle point of collinear triples.
pub fn simplify_path(points: Vec<Point>) -> Vec<Point> {
    let mut simplified = Vec::with_capacity(points.len());
    for point in points {
        if simplified.last().is_some_and(|previous: &Point| {
            (previous.x - point.x).abs() < EPS && (previous.y - point.y).abs() < EPS
        }) {
            continue;
        }
        while simplified.len() >= 2 {
            let a = simplified[simplified.len() - 2];
            let b = simplified[simplified.len() - 1];
            if ((a.x - b.x).abs() < EPS && (b.x - point.x).abs() < EPS)
                || ((a.y - b.y).abs() < EPS && (b.y - point.y).abs() < EPS)
            {
                simplified.pop();
            } else {
                break;
            }
        }
        simplified.push(point);
    }
    simplified
}

/// Indices of the points that shape the line: both ends plus every real corner.
/// Duplicates and points on a straight run (or a 180° fold) are dropped; of two
/// coinciding points the one with `prefer(index)` survives.
pub fn corner_indices(points: &[Point], prefer: impl Fn(usize) -> bool) -> Vec<usize> {
    let same = |a: Point, b: Point| (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS;
    let collinear = |a: Point, b: Point, c: Point| {
        ((a.x - b.x).abs() < EPS && (b.x - c.x).abs() < EPS)
            || ((a.y - b.y).abs() < EPS && (b.y - c.y).abs() < EPS)
    };
    let n = points.len();
    let mut kept: Vec<usize> = Vec::with_capacity(n);
    for (index, &point) in points.iter().enumerate() {
        if let Some(&last) = kept.last()
            && same(points[last], point)
        {
            if index + 1 == n {
                if last == 0 {
                    kept.push(index);
                } else {
                    *kept.last_mut().unwrap() = index;
                }
            } else if last != 0 && prefer(index) && !prefer(last) {
                *kept.last_mut().unwrap() = index;
            }
            continue;
        }
        while kept.len() >= 2
            && collinear(
                points[kept[kept.len() - 2]],
                points[kept[kept.len() - 1]],
                point,
            )
        {
            kept.pop();
        }
        kept.push(index);
    }
    kept
}

fn path_cost(points: &[Point]) -> f32 {
    points
        .windows(2)
        .map(|pair| pair[0].distance(pair[1]))
        .sum::<f32>()
        + ROUTE_BEND_COST * points.len().saturating_sub(2) as f32
}

fn append_route_point(points: &mut Vec<Point>, point: Point) {
    if points.last().is_none_or(|last| last.distance(point) > EPS) {
        points.push(point);
    }
}

// ---------------------------------------------------------------------------
// obstacle index
// ---------------------------------------------------------------------------

type Bucket = (i32, i32);

fn bucket_span(r: &Rectangle, size: f32) -> (Bucket, Bucket) {
    (
        ((r.x / size).floor() as i32, (r.y / size).floor() as i32),
        (
            (right(r) / size).floor() as i32,
            (bottom(r) / size).floor() as i32,
        ),
    )
}

/// Inflated card rectangles (index = person index) in a uniform bucket grid.
#[derive(Debug, Default)]
pub struct Obstacles {
    rects: Vec<Rectangle>,
    buckets: HashMap<Bucket, Vec<u32>>,
}

impl Obstacles {
    pub fn build(people: &[Person]) -> Self {
        let mut obstacles = Self {
            rects: people.iter().map(inflated).collect(),
            buckets: HashMap::new(),
        };
        for i in 0..obstacles.rects.len() {
            obstacles.link(i);
        }
        obstacles
    }

    fn link(&mut self, i: usize) {
        let ((x0, y0), (x1, y1)) = bucket_span(&self.rects[i], BUCKET);
        for by in y0..=y1 {
            for bx in x0..=x1 {
                self.buckets.entry((bx, by)).or_default().push(i as u32);
            }
        }
    }

    fn unlink(&mut self, i: usize) {
        let ((x0, y0), (x1, y1)) = bucket_span(&self.rects[i], BUCKET);
        for by in y0..=y1 {
            for bx in x0..=x1 {
                if let Some(bucket) = self.buckets.get_mut(&(bx, by)) {
                    bucket.retain(|&j| j != i as u32);
                    if bucket.is_empty() {
                        self.buckets.remove(&(bx, by));
                    }
                }
            }
        }
    }

    fn update(&mut self, i: usize, person: &Person) {
        self.unlink(i);
        self.rects[i] = inflated(person);
        self.link(i);
    }

    fn rect(&self, i: u32) -> Rectangle {
        self.rects[i as usize]
    }

    /// Sorted indices of all rectangles touching `area`.
    fn query(&self, area: Rectangle, out: &mut Vec<u32>) {
        out.clear();
        let ((x0, y0), (x1, y1)) = bucket_span(&area, BUCKET);
        let span = (x1 - x0 + 1) as i64 * (y1 - y0 + 1) as i64;
        if span > self.buckets.len() as i64 {
            for (&(bx, by), bucket) in &self.buckets {
                if bx >= x0 && bx <= x1 && by >= y0 && by <= y1 {
                    out.extend_from_slice(bucket);
                }
            }
        } else {
            for by in y0..=y1 {
                for bx in x0..=x1 {
                    if let Some(bucket) = self.buckets.get(&(bx, by)) {
                        out.extend_from_slice(bucket);
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out.retain(|&i| touches(&self.rects[i as usize], &area));
    }
}

// ---------------------------------------------------------------------------
// ports
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Horizontal = 0,
    Vertical = 1,
}

#[derive(Clone, Copy)]
struct Port {
    anchor: Point,
    escape: Point,
}

/// Left, right, top, bottom - each slid along its card edge toward `toward`.
fn ports(person: &Person, toward: Point) -> [Port; 4] {
    let left = person.x;
    let right = person.x + NODE_W;
    let top = person.y;
    let bottom = person.y + NODE_H;
    let y = toward.y.clamp(top + PORT_INSET, bottom - PORT_INSET);
    let x = toward.x.clamp(left + PORT_INSET, right - PORT_INSET);
    [
        Port {
            anchor: Point::new(left, y),
            escape: Point::new(left - ROUTE_CLEARANCE, y),
        },
        Port {
            anchor: Point::new(right, y),
            escape: Point::new(right + ROUTE_CLEARANCE, y),
        },
        Port {
            anchor: Point::new(x, top),
            escape: Point::new(x, top - ROUTE_CLEARANCE),
        },
        Port {
            anchor: Point::new(x, bottom),
            escape: Point::new(x, bottom + ROUTE_CLEARANCE),
        },
    ]
}

fn port_axis(index: usize) -> Axis {
    if index < 2 {
        Axis::Horizontal
    } else {
        Axis::Vertical
    }
}

/// The stored port, or all four when none is stored.
fn port_choices(ports: &[Port; 4], stored: Option<u8>) -> Vec<(usize, Port)> {
    match stored.map(usize::from).filter(|index| *index < ports.len()) {
        Some(index) => vec![(index, ports[index])],
        None => ports.iter().copied().enumerate().collect(),
    }
}

fn port_terminals(choices: &[(usize, Port)]) -> Vec<Terminal> {
    choices
        .iter()
        .map(|&(index, port)| Terminal {
            point: port.escape,
            axis: Some(port_axis(index)),
            back: None,
        })
        .collect()
}

/// The stored port, otherwise the port with the cheapest path to `target`.
fn select_port(
    ports: &[Port; 4],
    stored: Option<u8>,
    target: Point,
    obstacles: &Obstacles,
    window: &mut Window,
) -> (usize, Port) {
    // a stored port only holds while the line still leaves the card through that side;
    // once the first bend is dragged beside or behind the edge, pick the side again
    let stored = stored.filter(|&index| {
        let escape = ports.get(usize::from(index)).map(|port| port.escape);
        escape.is_some_and(|escape| match index {
            0 => target.x <= escape.x + EPS,
            1 => target.x >= escape.x - EPS,
            2 => target.y <= escape.y + EPS,
            _ => target.y >= escape.y - EPS,
        })
    });
    let choices = port_choices(ports, stored);
    if choices.len() == 1 {
        return choices[0];
    }
    let found = find_path(
        &port_terminals(&choices),
        &[Terminal::free(target)],
        obstacles,
        window,
    );
    choices[found.source]
}

fn project_out_of_obstacles(mut point: Point, obstacles: &Obstacles) -> Point {
    let mut ids = Vec::new();
    for _ in 0..8 {
        obstacles.query(
            Rectangle {
                x: point.x,
                y: point.y,
                width: 0.0,
                height: 0.0,
            },
            &mut ids,
        );
        let Some(rect) = ids
            .iter()
            .map(|&i| obstacles.rect(i))
            .find(|rect| point_inside(point, *rect))
        else {
            break;
        };
        point = [
            Point::new(rect.x, point.y),
            Point::new(right(&rect), point.y),
            Point::new(point.x, rect.y),
            Point::new(point.x, bottom(&rect)),
        ]
        .into_iter()
        .min_by(|left, right| left.distance(point).total_cmp(&right.distance(point)))
        .unwrap_or(point);
    }
    point
}

// ---------------------------------------------------------------------------
// search
// ---------------------------------------------------------------------------

/// Start or end of a search. `axis` is the travel direction leaving a source /
/// entering a target (a port's escape direction); `None` = any direction.
/// `back` (sources only): the point the line arrived from - the search never turns
/// straight back toward it, so lines cannot fold onto themselves at a user bend.
#[derive(Debug, Clone, Copy)]
struct Terminal {
    point: Point,
    axis: Option<Axis>,
    back: Option<Point>,
}

impl Terminal {
    fn free(point: Point) -> Self {
        Self {
            point,
            axis: None,
            back: None,
        }
    }
}

struct Found {
    points: Vec<Point>,
    source: usize,
    target: usize,
}

/// Union of all areas a route looked at; the route only depends on cards in there.
#[derive(Default)]
struct Window(Option<Rectangle>);

impl Window {
    fn add(&mut self, area: Rectangle) {
        self.0 = Some(self.0.map_or(area, |window| union(window, area)));
    }
}

/// Cheapest orthogonal path from any source to any target.
fn find_path(
    sources: &[Terminal],
    targets: &[Terminal],
    obstacles: &Obstacles,
    window: &mut Window,
) -> Found {
    let Some(bounds) = bounds_of(sources.iter().chain(targets).map(|t| t.point)) else {
        return Found {
            points: Vec::new(),
            source: 0,
            target: 0,
        };
    };
    let reach = bounds.width.max(bounds.height);
    for (dx, dy) in [
        (NODE_W * 2.0, NODE_H * 2.0),
        (reach + NODE_W * 4.0, reach + NODE_H * 4.0),
    ] {
        let area = grow(bounds, dx, dy);
        window.add(area);
        if let Some(found) = grid_search(area, sources, targets, obstacles) {
            return found;
        }
    }
    fallback_path(sources, targets, obstacles, window)
}

fn sort_dedup(values: &mut Vec<f32>) {
    values.sort_by(f32::total_cmp);
    values.dedup_by(|value, kept| (*value - *kept).abs() < EPS);
}

fn count_le(values: &[f32], limit: f32) -> usize {
    values.partition_point(|v| *v <= limit)
}

fn count_lt(values: &[f32], limit: f32) -> usize {
    values.partition_point(|v| *v < limit)
}

fn grid_index(values: &[f32], v: f32) -> usize {
    values
        .partition_point(|x| *x < v - EPS)
        .min(values.len() - 1)
}

#[derive(Clone, Copy)]
struct Open {
    f: f32,
    g: f32,
    state: usize,
}

impl PartialEq for Open {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Open {}

impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Open {
    /// Max-heap order: lowest `f` first, then deepest `g`, then lowest state.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .f
            .total_cmp(&self.f)
            .then_with(|| self.g.total_cmp(&other.g))
            .then_with(|| other.state.cmp(&self.state))
    }
}

/// A* on the Hanan grid of the cards inside `area`. `None` if no path stays inside.
fn grid_search(
    area: Rectangle,
    sources: &[Terminal],
    targets: &[Terminal],
    obstacles: &Obstacles,
) -> Option<Found> {
    let terminals = || sources.iter().chain(targets);
    let mut ids = Vec::new();
    obstacles.query(area, &mut ids);
    // a card that swallows a terminal (cards closer than the clearance) is ignored
    let rects: Vec<Rectangle> = ids
        .iter()
        .map(|&i| obstacles.rect(i))
        .filter(|rect| !terminals().any(|t| point_inside(t.point, *rect)))
        .collect();

    let mut xs = Vec::with_capacity(rects.len() * 2 + 10);
    let mut ys = Vec::with_capacity(rects.len() * 2 + 10);
    xs.extend([area.x, right(&area)]);
    ys.extend([area.y, bottom(&area)]);
    for t in terminals() {
        xs.push(t.point.x);
        ys.push(t.point.y);
    }
    for r in &rects {
        xs.extend(
            [r.x, right(r)]
                .into_iter()
                .filter(|x| *x > area.x && *x < right(&area)),
        );
        ys.extend(
            [r.y, bottom(r)]
                .into_iter()
                .filter(|y| *y > area.y && *y < bottom(&area)),
        );
    }
    sort_dedup(&mut xs);
    sort_dedup(&mut ys);
    let (nx, ny) = (xs.len(), ys.len());
    if nx < 2 || ny < 2 || nx * ny * 2 > MAX_SEARCH_STATES {
        return None;
    }

    // grid edges strictly inside a card are blocked; card outlines are grid lines,
    // so an edge is either completely inside or completely outside every card
    let mut h_blocked = vec![false; (nx - 1) * ny];
    let mut v_blocked = vec![false; nx * (ny - 1)];
    for r in &rects {
        let (x_in, x_out) = (count_le(&xs, r.x + EPS), count_lt(&xs, right(r) - EPS));
        let (y_in, y_out) = (count_le(&ys, r.y + EPS), count_lt(&ys, bottom(r) - EPS));
        for j in y_in..y_out {
            for i in x_in.max(1) - 1..x_out.min(nx - 1) {
                h_blocked[j * (nx - 1) + i] = true;
            }
        }
        for j in y_in.max(1) - 1..y_out.min(ny - 1) {
            for i in x_in..x_out {
                v_blocked[j * nx + i] = true;
            }
        }
    }

    let node_of = |p: Point| grid_index(&ys, p.y) * nx + grid_index(&xs, p.x);
    let target_nodes: Vec<usize> = targets.iter().map(|t| node_of(t.point)).collect();
    // admissible: Manhattan distance plus one bend when not aligned with a target
    let heuristic = |node: usize| {
        let (x, y) = (xs[node % nx], ys[node / nx]);
        targets
            .iter()
            .map(|t| {
                let (dx, dy) = ((t.point.x - x).abs(), (t.point.y - y).abs());
                dx + dy
                    + if dx > EPS && dy > EPS {
                        ROUTE_BEND_COST
                    } else {
                        0.0
                    }
            })
            .fold(f32::INFINITY, f32::min)
    };

    let states = nx * ny * 2;
    let mut best = vec![f32::INFINITY; states];
    let mut parent = vec![u32::MAX; states];
    let mut open = BinaryHeap::new();
    for source in sources {
        let node = node_of(source.point);
        let axes: &[usize] = match source.axis {
            Some(axis) => &[axis as usize][..],
            None => &[0, 1],
        };
        for &axis in axes {
            let state = node * 2 + axis;
            if best[state] > 0.0 {
                best[state] = 0.0;
                open.push(Open {
                    f: heuristic(node),
                    g: 0.0,
                    state,
                });
            }
        }
    }

    // sources with a `back` point may not leave straight toward it (no 180° fold)
    let folds: Vec<(usize, usize)> = sources
        .iter()
        .filter_map(|source| {
            let back = source.back?;
            let p = source.point;
            let direction = if (back.y - p.y).abs() < EPS && back.x < p.x - EPS {
                0
            } else if (back.y - p.y).abs() < EPS && back.x > p.x + EPS {
                1
            } else if (back.x - p.x).abs() < EPS && back.y < p.y - EPS {
                2
            } else if (back.x - p.x).abs() < EPS && back.y > p.y + EPS {
                3
            } else {
                return None;
            };
            Some((node_of(p), direction))
        })
        .collect();

    let mut goal: Option<(f32, usize, usize)> = None;
    while let Some(Open { f, g, state }) = open.pop() {
        if goal.is_some_and(|(cost, _, _)| cost <= f) {
            break;
        }
        if g > best[state] {
            continue;
        }
        let node = state / 2;
        let axis = state % 2;
        for (target, &target_node) in target_nodes.iter().enumerate() {
            if target_node != node {
                continue;
            }
            let arrival = match targets[target].axis {
                Some(required) if required as usize != axis => ROUTE_BEND_COST,
                _ => 0.0,
            };
            if goal.is_none_or(|(cost, _, _)| g + arrival < cost) {
                goal = Some((g + arrival, state, target));
            }
        }

        let (i, j) = (node % nx, node / nx);
        let neighbours = [
            (i > 0 && !h_blocked[j * (nx - 1) + i - 1]).then(|| (node - 1, 0)),
            (i + 1 < nx && !h_blocked[j * (nx - 1) + i]).then(|| (node + 1, 0)),
            (j > 0 && !v_blocked[(j - 1) * nx + i]).then(|| (node - nx, 1)),
            (j + 1 < ny && !v_blocked[j * nx + i]).then(|| (node + nx, 1)),
        ];
        let start = parent[state] == u32::MAX;
        for (direction, step) in neighbours.into_iter().enumerate() {
            let Some((next, next_axis)) = step else {
                continue;
            };
            if start && folds.contains(&(node, direction)) {
                continue;
            }
            let length = (xs[next % nx] - xs[i]).abs() + (ys[next / nx] - ys[j]).abs();
            let bend = if next_axis != axis {
                ROUTE_BEND_COST
            } else {
                0.0
            };
            let cost = g + length + bend;
            let next_state = next * 2 + next_axis;
            if cost + 1e-4 < best[next_state] {
                best[next_state] = cost;
                parent[next_state] = state as u32;
                open.push(Open {
                    f: cost + heuristic(next),
                    g: cost,
                    state: next_state,
                });
            }
        }
    }

    let (_, mut state, target) = goal?;
    let mut points = Vec::new();
    loop {
        let node = state / 2;
        points.push(Point::new(xs[node % nx], ys[node / nx]));
        match parent[state] {
            u32::MAX => break,
            previous => state = previous as usize,
        }
    }
    points.reverse();
    let start_node = state / 2;
    let source = sources
        .iter()
        .position(|s| {
            node_of(s.point) == start_node && s.axis.is_none_or(|axis| axis as usize == state % 2)
        })
        .unwrap_or(0);
    // grid coordinates are deduplicated with a tolerance - snap the ends exactly
    points[0] = sources[source].point;
    let last = points.len() - 1;
    points[last] = targets[target].point;
    Some(Found {
        points: simplify_path(points),
        source,
        target,
    })
}

/// No path inside the search windows: try four detours around everything nearby,
/// then give up with a plain (still orthogonal) L.
fn fallback_path(
    sources: &[Terminal],
    targets: &[Terminal],
    obstacles: &Obstacles,
    window: &Window,
) -> Found {
    let manhattan = |a: Point, b: Point| (a.x - b.x).abs() + (a.y - b.y).abs();
    let (source, target) = (0..sources.len())
        .flat_map(|s| (0..targets.len()).map(move |t| (s, t)))
        .min_by(|&(s1, t1), &(s2, t2)| {
            manhattan(sources[s1].point, targets[t1].point)
                .total_cmp(&manhattan(sources[s2].point, targets[t2].point))
        })
        .unwrap_or((0, 0));
    let (start, end) = (sources[source].point, targets[target].point);
    let mut ids = Vec::new();
    if let Some(area) = window.0 {
        obstacles.query(area, &mut ids);
    }
    let relevant: Vec<Rectangle> = ids
        .iter()
        .map(|&i| obstacles.rect(i))
        .filter(|rect| !point_inside(start, *rect) && !point_inside(end, *rect))
        .collect();
    let left = relevant
        .iter()
        .map(|rect| rect.x)
        .fold(start.x.min(end.x), f32::min)
        - ROUTE_CLEARANCE;
    let right_side = relevant
        .iter()
        .map(right)
        .fold(start.x.max(end.x), f32::max)
        + ROUTE_CLEARANCE;
    let top = relevant
        .iter()
        .map(|rect| rect.y)
        .fold(start.y.min(end.y), f32::min)
        - ROUTE_CLEARANCE;
    let bottom_side = relevant
        .iter()
        .map(bottom)
        .fold(start.y.max(end.y), f32::max)
        + ROUTE_CLEARANCE;
    let candidates = [
        vec![start, Point::new(start.x, top), Point::new(end.x, top), end],
        vec![
            start,
            Point::new(start.x, bottom_side),
            Point::new(end.x, bottom_side),
            end,
        ],
        vec![start, Point::new(left, start.y), Point::new(left, end.y), end],
        vec![
            start,
            Point::new(right_side, start.y),
            Point::new(right_side, end.y),
            end,
        ],
    ];
    let points = candidates
        .into_iter()
        .filter(|path| {
            path.windows(2)
                .all(|pair| segment_clear(pair[0], pair[1], &relevant))
        })
        .min_by(|left, right| path_cost(left).total_cmp(&path_cost(right)))
        .unwrap_or_else(|| vec![start, Point::new(end.x, start.y), end]);
    Found {
        points: simplify_path(points),
        source,
        target,
    }
}

// ---------------------------------------------------------------------------
// single routes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Route {
    pub points: Vec<Point>,
    /// `(index in points, index in link.route)` of every user bend.
    pub custom_bends: Vec<(usize, usize)>,
    pub label_at: Point,
    pub label_centered: bool,
    pub start_port: usize,
    pub end_port: usize,
    /// Bounding box of `points` (labels not included).
    pub bbox: Rectangle,
}

impl Route {
    fn new(points: Vec<Point>, raw: &Raw) -> Self {
        let (label_at, label_centered) = default_label(&points);
        let bbox = bounds_of(points.iter().copied()).unwrap_or(Rectangle {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        });
        Self {
            points,
            custom_bends: raw.custom_bends.clone(),
            label_at,
            label_centered,
            start_port: raw.start_port,
            end_port: raw.end_port,
            bbox,
        }
    }
}

/// Router output before nudging.
#[derive(Debug, Clone)]
struct Raw {
    points: Vec<Point>,
    custom_bends: Vec<(usize, usize)>,
    start_port: usize,
    end_port: usize,
}

fn route_points(
    a: &Person,
    b: &Person,
    link: &Link,
    obstacles: &Obstacles,
    window: &mut Window,
) -> Raw {
    let waypoints: Vec<Point> = link
        .route
        .iter()
        .map(|[x, y]| project_out_of_obstacles(Point::new(*x, *y), obstacles))
        .collect();
    let source_ports = ports(a, waypoints.first().copied().unwrap_or(centre(b)));
    let destination_ports = ports(b, waypoints.last().copied().unwrap_or(centre(a)));

    let (Some(&first), Some(&last)) = (waypoints.first(), waypoints.last()) else {
        // automatic route: one search over every allowed port pair
        let sources = port_choices(&source_ports, link.start_port);
        let targets = port_choices(&destination_ports, link.end_port);
        let found = find_path(
            &port_terminals(&sources),
            &port_terminals(&targets),
            obstacles,
            window,
        );
        let (start_port, start) = sources[found.source];
        let (end_port, end) = targets[found.target];
        let mut points = Vec::with_capacity(found.points.len() + 2);
        points.push(start.anchor);
        points.extend(found.points);
        points.push(end.anchor);
        return Raw {
            points: simplify_path(points),
            custom_bends: Vec::new(),
            start_port,
            end_port,
        };
    };

    // user bends: the route passes exactly through every stored waypoint
    let (start_port, start) =
        select_port(&source_ports, link.start_port, first, obstacles, window);
    let (end_port, end) =
        select_port(&destination_ports, link.end_port, last, obstacles, window);
    let mut points = vec![start.anchor, start.escape];
    let mut custom_bends = Vec::with_capacity(waypoints.len());
    let mut from = Terminal {
        point: start.escape,
        axis: Some(port_axis(start_port)),
        back: None,
    };
    for (stored_index, waypoint) in waypoints.iter().copied().enumerate() {
        let leg = find_path(&[from], &[Terminal::free(waypoint)], obstacles, window);
        points.extend(leg.points.into_iter().skip(1));
        custom_bends.push((points.len() - 1, stored_index));
        from = Terminal {
            back: points.len().checked_sub(2).map(|i| points[i]),
            ..Terminal::free(waypoint)
        };
    }
    let leg = find_path(
        &[from],
        &[Terminal {
            point: end.escape,
            axis: Some(port_axis(end_port)),
            back: None,
        }],
        obstacles,
        window,
    );
    points.extend(leg.points.into_iter().skip(1));
    append_route_point(&mut points, end.anchor);

    // keep corners only: escape points and stored bends on a straight run disappear
    let is_bend = |index: usize| custom_bends.iter().any(|(i, _)| *i == index);
    let kept = corner_indices(&points, is_bend);
    let mut renumbered = vec![usize::MAX; points.len()];
    for (new_index, &old_index) in kept.iter().enumerate() {
        renumbered[old_index] = new_index;
    }
    let custom_bends = custom_bends
        .iter()
        .filter_map(|&(index, stored)| {
            let index = renumbered[index];
            (index != usize::MAX).then_some((index, stored))
        })
        .collect();
    Raw {
        points: kept.iter().map(|&index| points[index]).collect(),
        custom_bends,
        start_port,
        end_port,
    }
}

/// Route of a single link (no nudging, default label).
#[cfg(test)]
pub fn route(a: &Person, b: &Person, link: &Link, people: &[Person]) -> Route {
    let obstacles = Obstacles::build(people);
    let raw = route_points(a, b, link, &obstacles, &mut Window::default());
    Route::new(raw.points.clone(), &raw)
}

/// Line drawn while connecting: from `source` to the cursor or to the hovered person.
pub fn connection_preview(
    source: &Person,
    target: Point,
    target_person: Option<&Person>,
    obstacles: &Obstacles,
    selected_port: Option<usize>,
) -> Vec<Point> {
    let starts = port_choices(
        &ports(source, target),
        selected_port.and_then(|port| u8::try_from(port).ok()),
    );
    let mut window = Window::default();
    let mut points = Vec::new();
    match target_person {
        Some(target_person) => {
            let ends = port_choices(&ports(target_person, centre(source)), None);
            let found = find_path(
                &port_terminals(&starts),
                &port_terminals(&ends),
                obstacles,
                &mut window,
            );
            points.push(starts[found.source].1.anchor);
            points.extend(found.points);
            points.push(ends[found.target].1.anchor);
        }
        None => {
            let found = find_path(
                &port_terminals(&starts),
                &[Terminal::free(target)],
                obstacles,
                &mut window,
            );
            points.push(starts[found.source].1.anchor);
            points.extend(found.points);
        }
    }
    simplify_path(points)
}

// ---------------------------------------------------------------------------
// labels
// ---------------------------------------------------------------------------

/// Background width of a line label drawn at `size` px.
pub fn label_width(label: &str, size: f32) -> f32 {
    label.chars().count() as f32 * size * 0.56 + 8.0
}

/// Middle of the longest segment: above horizontal, right of vertical segments.
fn default_label(points: &[Point]) -> (Point, bool) {
    let longest = points
        .windows(2)
        .max_by(|a, b| a[0].distance(a[1]).total_cmp(&b[0].distance(b[1])));
    longest.map_or((Point::ORIGIN, true), |pair| {
        let (start, end) = (pair[0], pair[1]);
        let horizontal = (end.x - start.x).abs() >= (end.y - start.y).abs();
        let at = if horizontal {
            Point::new((start.x + end.x) / 2.0, start.y - 10.0)
        } else {
            Point::new((start.x + end.x) / 2.0 + 8.0, (start.y + end.y) / 2.0)
        };
        (at, horizontal)
    })
}

/// Where a link's label is drawn: the user position if any, else the placed default.
pub fn label_layout(link: &Link, route: &Route) -> (Point, bool) {
    match link.label_position {
        Some(position) if position.legacy_absolute => {
            (Point::new(position.offset[0], position.offset[1]), true)
        }
        Some(position) if route.points.len() >= 2 => {
            let index = position.segment.min(route.points.len() - 2);
            let start = route.points[index];
            let end = route.points[index + 1];
            (
                Point::new(
                    start.x + (end.x - start.x) * position.along + position.offset[0],
                    start.y + (end.y - start.y) * position.along + position.offset[1],
                ),
                true,
            )
        }
        _ => (route.label_at, route.label_centered),
    }
}

/// World rectangle covered by a label background (matches `board::draw_edges`).
fn label_rect(at: Point, centered: bool, width: f32) -> Rectangle {
    Rectangle {
        x: if centered { at.x - width / 2.0 } else { at.x - 4.0 },
        y: at.y - LABEL_HALF_HEIGHT,
        width,
        height: LABEL_HALF_HEIGHT * 2.0,
    }
}

/// Candidate label positions, best first: the longest segments, middle before sides.
fn label_candidates(points: &[Point], width: f32) -> Vec<(Point, bool)> {
    let mut segments: Vec<(usize, f32)> = points
        .windows(2)
        .enumerate()
        .map(|(index, pair)| (index, pair[0].distance(pair[1])))
        .filter(|(_, length)| *length > 1.0)
        .collect();
    segments.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut candidates = Vec::new();
    for &(index, _) in segments.iter().take(4) {
        let (start, end) = (points[index], points[index + 1]);
        let horizontal = (end.x - start.x).abs() >= (end.y - start.y).abs();
        for t in [0.5, 0.3, 0.7] {
            let at = Point::new(
                start.x + (end.x - start.x) * t,
                start.y + (end.y - start.y) * t,
            );
            if horizontal {
                candidates.push((Point::new(at.x, at.y - 10.0), true));
                candidates.push((Point::new(at.x, at.y + 10.0), true));
            } else {
                candidates.push((Point::new(at.x + 8.0, at.y), false));
                candidates.push((Point::new(at.x - width, at.y), false));
            }
        }
    }
    candidates
}

#[derive(Default)]
struct LabelIndex {
    buckets: HashMap<Bucket, Vec<Rectangle>>,
}

impl LabelIndex {
    fn insert(&mut self, rect: Rectangle) {
        let ((x0, y0), (x1, y1)) = bucket_span(&rect, LABEL_BUCKET);
        for by in y0..=y1 {
            for bx in x0..=x1 {
                self.buckets.entry((bx, by)).or_default().push(rect);
            }
        }
    }

    fn hits(&self, rect: &Rectangle) -> bool {
        let ((x0, y0), (x1, y1)) = bucket_span(rect, LABEL_BUCKET);
        (y0..=y1).any(|by| {
            (x0..=x1).any(|bx| {
                self.buckets
                    .get(&(bx, by))
                    .is_some_and(|rects| rects.iter().any(|other| overlaps(rect, other)))
            })
        })
    }
}

fn label_is_free(
    rect: &Rectangle,
    obstacles: &Obstacles,
    placed: &LabelIndex,
    scratch: &mut Vec<u32>,
) -> bool {
    obstacles.query(*rect, scratch);
    let on_card = scratch.iter().any(|&i| {
        let card = grow(obstacles.rect(i), -ROUTE_CLEARANCE, -ROUTE_CLEARANCE);
        overlaps(rect, &card)
    });
    !on_card && !placed.hits(rect)
}

// ---------------------------------------------------------------------------
// nudging
// ---------------------------------------------------------------------------

/// Which segments a nudging pass moves: horizontal ones move in y, vertical ones in x.
#[derive(Clone, Copy)]
enum Orient {
    Horizontal,
    Vertical,
}

impl Orient {
    /// Coordinate the segment is moved along (y for horizontal segments).
    fn line(self, p: Point) -> f32 {
        match self {
            Self::Horizontal => p.y,
            Self::Vertical => p.x,
        }
    }

    fn along(self, p: Point) -> f32 {
        match self {
            Self::Horizontal => p.x,
            Self::Vertical => p.y,
        }
    }

    fn set_line(self, p: &mut Point, value: f32) {
        match self {
            Self::Horizontal => p.y = value,
            Self::Vertical => p.x = value,
        }
    }

    fn line_span(self, r: &Rectangle) -> (f32, f32) {
        match self {
            Self::Horizontal => (r.y, bottom(r)),
            Self::Vertical => (r.x, right(r)),
        }
    }

    fn along_span(self, r: &Rectangle) -> (f32, f32) {
        match self {
            Self::Horizontal => (r.x, right(r)),
            Self::Vertical => (r.y, bottom(r)),
        }
    }

    fn rect(self, along: (f32, f32), line: (f32, f32)) -> Rectangle {
        let (x, y) = match self {
            Self::Horizontal => (along, line),
            Self::Vertical => (line, along),
        };
        Rectangle {
            x: x.0,
            y: y.0,
            width: x.1 - x.0,
            height: y.1 - y.0,
        }
    }
}

struct Line {
    points: Vec<Point>,
    /// User bends (and the endpoints of user-edited routes) never move.
    fixed: Vec<bool>,
    /// From / to cards: automatic endpoints may slide along their card edge.
    cards: [Rectangle; 2],
}

struct Segment {
    line: usize,
    index: usize,
    lo: f32,
    hi: f32,
    /// Free space around the segment (cards kept `NUDGE_MARGIN` away).
    channel: (f32, f32),
    desired: f32,
    /// -2..=2: where the neighbouring segments go (negative = toward smaller coordinates).
    turn: i8,
    /// Side the neighbour at the `hi` end goes to (orders Z-shaped lines).
    hi_side: i8,
}

fn line_key(value: f32) -> i64 {
    (value * 2.0).round() as i64
}

/// Free interval around `at` for a segment covering `lo..hi`.
fn channel(
    orient: Orient,
    at: f32,
    (lo, hi): (f32, f32),
    obstacles: &Obstacles,
    scratch: &mut Vec<u32>,
) -> (f32, f32) {
    obstacles.query(
        orient.rect((lo, hi), (at - NUDGE_REACH, at + NUDGE_REACH)),
        scratch,
    );
    let inset = ROUTE_CLEARANCE - NUDGE_MARGIN;
    let (mut min, mut max) = (f32::NEG_INFINITY, f32::INFINITY);
    for &i in scratch.iter() {
        let rect = obstacles.rect(i);
        let (a0, a1) = orient.along_span(&rect);
        if a0 + inset >= hi - EPS || a1 - inset <= lo + EPS {
            continue;
        }
        let (l0, l1) = orient.line_span(&rect);
        let (l0, l1) = (l0 + inset, l1 - inset);
        if l1 <= at + EPS {
            min = min.max(l1);
        } else if l0 >= at - EPS {
            max = max.min(l0);
        }
        // cards spanning the segment are its own endpoint cards
    }
    (min, max)
}

/// How far segment `index` may move without reversing its neighbours or leaving a card edge.
fn movable(line: &Line, index: usize, orient: Orient) -> Option<(f32, f32)> {
    let points = &line.points;
    let n = points.len();
    let at = orient.line(points[index]);
    let (mut lo, mut hi) = (f32::NEG_INFINITY, f32::INFINITY);
    let (mut smaller, mut larger, mut on_card) = (false, false, false);
    for (end, far) in [
        (index, index.checked_sub(1)),
        (index + 1, (index + 2 < n).then_some(index + 2)),
    ] {
        if end == 0 || end + 1 == n {
            let (c0, c1) = orient.line_span(&line.cards[usize::from(end != 0)]);
            lo = lo.max(c0 + PORT_INSET);
            hi = hi.min(c1 - PORT_INSET);
            on_card = true;
        } else if let Some(far) = far {
            let v = orient.line(points[far]);
            if v < at - EPS {
                lo = lo.max(v + NUDGE_MIN_LEG);
                smaller = true;
            } else if v > at + EPS {
                hi = hi.min(v - NUDGE_MIN_LEG);
                larger = true;
            } else {
                return None;
            }
        }
    }
    // U-shape (both neighbours on one side): moving away would only stretch the
    // detour, so allow just enough room to spread a bundle
    if !on_card && smaller != larger {
        if smaller {
            hi = hi.min(at + NUDGE_U_SLACK);
        } else {
            lo = lo.max(at - NUDGE_U_SLACK);
        }
    }
    (lo <= hi).then_some((lo, hi))
}

fn slides_on(orient: Orient, anchor: Point, card: &Rectangle) -> bool {
    let (a0, a1) = orient.along_span(card);
    let along = orient.along(anchor);
    (along - a0).abs() < EPS || (along - a1).abs() < EPS
}

fn nudge(lines: &mut [Line], obstacles: &Obstacles, orient: Orient, scratch: &mut Vec<u32>) {
    let mut segments = Vec::new();
    for (line_index, line) in lines.iter().enumerate() {
        let n = line.points.len();
        for index in 0..n.saturating_sub(1) {
            let (p, q) = (line.points[index], line.points[index + 1]);
            let at = orient.line(p);
            if (orient.line(q) - at).abs() > EPS
                || (orient.along(q) - orient.along(p)).abs() < EPS
                || line.fixed[index]
                || line.fixed[index + 1]
                || (index == 0 && !slides_on(orient, p, &line.cards[0]))
                || (index + 2 == n && !slides_on(orient, q, &line.cards[1]))
            {
                continue;
            }
            let (lo, hi) = {
                let (a, b) = (orient.along(p), orient.along(q));
                (a.min(b), a.max(b))
            };
            let channel = channel(orient, at, (lo, hi), obstacles, scratch);
            if at < channel.0 - EPS || at > channel.1 + EPS {
                continue;
            }
            let side = |far: Option<usize>| -> i8 {
                far.map_or(0, |far| {
                    let v = orient.line(line.points[far]);
                    if v < at - EPS {
                        -1
                    } else if v > at + EPS {
                        1
                    } else {
                        0
                    }
                })
            };
            let (before, after) = (
                side(index.checked_sub(1)),
                side((index + 2 < n).then_some(index + 2)),
            );
            let hi_side = if orient.along(q) >= orient.along(p) {
                after
            } else {
                before
            };
            // U-shapes stay where the router put them (tight detours); others centre in
            // narrow channels such as the gap between two rows
            let u_shape = before != 0 && before == after;
            let desired = if !u_shape && channel.1 - channel.0 <= NUDGE_CENTRE_CHANNEL {
                (channel.0 + channel.1) / 2.0
            } else {
                at
            };
            segments.push(Segment {
                line: line_index,
                index,
                lo,
                hi,
                channel,
                desired,
                turn: before + after,
                hi_side,
            });
        }
    }

    // bundles = segments that want the same line and overlap along it
    segments.sort_by(|a, b| {
        line_key(a.desired)
            .cmp(&line_key(b.desired))
            .then(a.lo.total_cmp(&b.lo))
    });
    let mut start = 0;
    while start < segments.len() {
        let key = line_key(segments[start].desired);
        let mut reach = segments[start].hi;
        let mut end = start + 1;
        while end < segments.len()
            && line_key(segments[end].desired) == key
            && segments[end].lo <= reach + 1.0
        {
            reach = reach.max(segments[end].hi);
            end += 1;
        }
        place_bundle(&mut segments[start..end], lines, orient);
        start = end;
    }
}

fn place_bundle(bundle: &mut [Segment], lines: &mut [Line], orient: Orient) {
    // Order that avoids crossings: lines turning toward smaller coordinates go to the
    // small side; Z-shapes by the side their far end turns to; nested U-shapes with
    // the widest one outside.
    bundle.sort_by(|a, b| {
        let span = |s: &Segment| s.hi - s.lo;
        a.turn
            .cmp(&b.turn)
            .then(a.hi_side.cmp(&b.hi_side))
            .then_with(|| match a.turn.cmp(&0) {
                Ordering::Less => span(a).total_cmp(&span(b)),
                Ordering::Greater => span(b).total_cmp(&span(a)),
                Ordering::Equal => Ordering::Equal,
            })
            .then(a.line.cmp(&b.line))
            .then(a.index.cmp(&b.index))
    });
    let count = bundle.len();
    let lo = bundle
        .iter()
        .map(|s| s.channel.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let hi = bundle
        .iter()
        .map(|s| s.channel.1)
        .fold(f32::INFINITY, f32::min);
    let mut spacing = if count > 1 { NUDGE_SPACING } else { 0.0 };
    if count > 1 && (hi - lo).is_finite() {
        spacing = spacing.min(((hi - lo) / (count + 1) as f32).max(0.0));
    }
    let span = spacing * (count - 1) as f32;
    let mut first = bundle[0].desired - span / 2.0;
    if lo + span <= hi {
        first = first.clamp(lo, hi - span);
    }
    for (k, segment) in bundle.iter().enumerate() {
        let line = &mut lines[segment.line];
        let Some((min, max)) = movable(line, segment.index, orient) else {
            continue;
        };
        let (min, max) = (min.max(segment.channel.0), max.min(segment.channel.1));
        if min > max {
            continue;
        }
        let value = (first + k as f32 * spacing).clamp(min, max);
        orient.set_line(&mut line.points[segment.index], value);
        orient.set_line(&mut line.points[segment.index + 1], value);
    }
}

// ---------------------------------------------------------------------------
// route cache
// ---------------------------------------------------------------------------

struct Entry {
    // inputs
    from: Id,
    to: Id,
    label: String,
    waypoints: Vec<[f32; 2]>,
    start_port: Option<u8>,
    end_port: Option<u8>,
    // outputs
    raw: Raw,
    window: Rectangle,
    cards: [Rectangle; 2],
    route: Route,
    seen: u64,
}

impl Entry {
    fn new(a: &Person, b: &Person, link: &Link, obstacles: &Obstacles, seen: u64) -> Self {
        let mut window = Window::default();
        let raw = route_points(a, b, link, obstacles, &mut window);
        let cards = [card(a), card(b)];
        let window = window
            .0
            .map_or(union(cards[0], cards[1]), |w| union(w, union(cards[0], cards[1])));
        Self {
            from: link.from.clone(),
            to: link.to.clone(),
            label: link.label.clone(),
            waypoints: link.route.clone(),
            start_port: link.start_port,
            end_port: link.end_port,
            route: Route::new(raw.points.clone(), &raw),
            raw,
            window,
            cards,
            seen,
        }
    }

    fn matches(&self, link: &Link) -> bool {
        self.from == link.from
            && self.to == link.to
            && self.label == link.label
            && self.waypoints == link.route
            && self.start_port == link.start_port
            && self.end_port == link.end_port
    }
}

/// All routed lines of the board, updated incrementally by [`Routes::sync`].
#[derive(Default)]
pub struct Routes {
    obstacles: Obstacles,
    /// Positions the obstacle index was built from.
    people: Vec<(Id, Point)>,
    entries: HashMap<Id, Entry>,
    stamp: u64,
}

impl Routes {
    pub fn get(&self, link_id: &str) -> Option<&Route> {
        self.entries.get(link_id).map(|entry| &entry.route)
    }

    pub fn obstacles(&self) -> &Obstacles {
        &self.obstacles
    }

    /// Brings the routes up to date with `data`; only lines whose inputs or search
    /// windows changed are re-routed. Cheap when nothing changed.
    pub fn sync(&mut self, data: &Data) {
        let dirty = self.sync_people(&data.people);
        self.stamp += 1;
        let stamp = self.stamp;
        let mut changed = false;
        for link in &data.links {
            let (Some(a), Some(b)) = (data.person(&link.from), data.person(&link.to)) else {
                continue;
            };
            let stale = self.entries.get(&link.id).is_none_or(|entry| {
                !entry.matches(link)
                    || dirty.as_ref().is_none_or(|rects| {
                        rects.iter().any(|rect| touches(&entry.window, rect))
                    })
            });
            if stale {
                let entry = Entry::new(a, b, link, &self.obstacles, stamp);
                self.entries.insert(link.id.clone(), entry);
                changed = true;
            } else if let Some(entry) = self.entries.get_mut(&link.id) {
                entry.seen = stamp;
            }
        }
        let before = self.entries.len();
        self.entries.retain(|_, entry| entry.seen == stamp);
        if changed || self.entries.len() != before {
            self.finish(data);
        }
    }

    /// Updates the obstacle index; returns the moved card areas (`None` = everything).
    fn sync_people(&mut self, people: &[Person]) -> Option<Vec<Rectangle>> {
        let grown = |at: Point| grow(card_at(at), ROUTE_CLEARANCE, ROUTE_CLEARANCE);
        let mut dirty = Vec::new();
        let same_people = self.people.len() == people.len()
            && self.people.iter().zip(people).all(|((id, _), p)| *id == p.id);
        if same_people {
            for (i, ((_, at), person)) in self.people.iter_mut().zip(people).enumerate() {
                let now = Point::new(person.x, person.y);
                if *at != now {
                    dirty.push(grown(*at));
                    dirty.push(grown(now));
                    *at = now;
                    self.obstacles.update(i, person);
                }
            }
        } else {
            {
                let old: HashMap<&str, Point> = self
                    .people
                    .iter()
                    .map(|(id, at)| (id.as_str(), *at))
                    .collect();
                let mut kept = HashMap::with_capacity(people.len());
                for person in people {
                    let now = Point::new(person.x, person.y);
                    match old.get(person.id.as_str()) {
                        Some(&at) if at == now => {}
                        Some(&at) => {
                            dirty.push(grown(at));
                            dirty.push(grown(now));
                        }
                        None => dirty.push(grown(now)),
                    }
                    kept.insert(person.id.as_str(), ());
                }
                for (id, at) in &old {
                    if !kept.contains_key(id) {
                        dirty.push(grown(*at));
                    }
                }
            }
            self.people = people
                .iter()
                .map(|p| (p.id.clone(), Point::new(p.x, p.y)))
                .collect();
            self.obstacles = Obstacles::build(people);
        }
        (dirty.len() <= MAX_DIRTY_CARDS * 2).then_some(dirty)
    }

    /// Nudges all raw routes and places the automatic labels.
    fn finish(&mut self, data: &Data) {
        let links: Vec<&Link> = data
            .links
            .iter()
            .filter(|link| self.entries.contains_key(&link.id))
            .collect();
        let mut lines: Vec<Line> = links
            .iter()
            .map(|link| {
                let entry = &self.entries[&link.id];
                // user-edited lines stay exactly where the user put them; moving them
                // would feed back into the next segment drag
                let fixed = vec![!link.route.is_empty(); entry.raw.points.len()];
                Line {
                    points: entry.raw.points.clone(),
                    fixed,
                    cards: entry.cards,
                }
            })
            .collect();
        let mut scratch = Vec::new();
        nudge(&mut lines, &self.obstacles, Orient::Horizontal, &mut scratch);
        nudge(&mut lines, &self.obstacles, Orient::Vertical, &mut scratch);

        for (link, line) in links.iter().zip(lines) {
            if let Some(entry) = self.entries.get_mut(&link.id) {
                entry.route = Route::new(line.points, &entry.raw);
            }
        }

        // labels: user-placed ones are fixed obstacles, the others pick a free spot
        let mut placed = LabelIndex::default();
        for link in &links {
            if link.label.is_empty() || link.label_position.is_none() {
                continue;
            }
            let route = &self.entries[&link.id].route;
            let (at, centered) = label_layout(link, route);
            placed.insert(label_rect(at, centered, label_width(&link.label, LABEL_TEXT)));
        }
        for link in &links {
            if link.label.is_empty() || link.label_position.is_some() {
                continue;
            }
            let Some(entry) = self.entries.get_mut(&link.id) else {
                continue;
            };
            let width = label_width(&link.label, LABEL_TEXT);
            let chosen = label_candidates(&entry.route.points, width)
                .into_iter()
                .find(|&(at, centered)| {
                    label_is_free(
                        &label_rect(at, centered, width),
                        &self.obstacles,
                        &placed,
                        &mut scratch,
                    )
                });
            if let Some((at, centered)) = chosen {
                entry.route.label_at = at;
                entry.route.label_centered = centered;
            }
            let route = &entry.route;
            placed.insert(label_rect(route.label_at, route.label_centered, width));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(id: &str, x: f32, y: f32) -> Person {
        Person {
            id: id.into(),
            name: id.into(),
            born: String::new(),
            died: String::new(),
            x,
            y,
            clans: Vec::new(),
        }
    }

    fn routed_link() -> Link {
        Link {
            id: "link".into(),
            from: "from".into(),
            to: "to".into(),
            label: "related to".into(),
            route: vec![[180.0, 20.0], [180.0, 180.0]],
            start_port: None,
            end_port: None,
            label_position: None,
        }
    }

    fn link(id: &str, from: &str, to: &str) -> Link {
        Link {
            id: id.into(),
            from: from.into(),
            to: to.into(),
            route: Vec::new(),
            ..routed_link()
        }
    }

    fn obstacle_rects(people: &[Person]) -> Vec<Rectangle> {
        people.iter().map(inflated).collect()
    }

    fn is_orthogonal(points: &[Point]) -> bool {
        points.windows(2).all(|pair| {
            (pair[0].x - pair[1].x).abs() < EPS || (pair[0].y - pair[1].y).abs() < EPS
        })
    }

    fn assert_on_border(point: Point, person: &Person) {
        let on_vertical_border =
            (point.x - person.x).abs() < EPS || (point.x - person.x - NODE_W).abs() < EPS;
        let on_horizontal_border =
            (point.y - person.y).abs() < EPS || (point.y - person.y - NODE_H).abs() < EPS;
        assert!(
            (on_vertical_border && (person.y..=person.y + NODE_H).contains(&point.y))
                || (on_horizontal_border && (person.x..=person.x + NODE_W).contains(&point.x)),
            "{point:?} is not on the border of {person:?}"
        );
    }

    fn assert_route_avoids_cards(points: &[Point], obstacles: &[Rectangle]) {
        assert!(is_orthogonal(points));
        assert!(
            points
                .windows(2)
                .skip(1)
                .take(points.len().saturating_sub(3))
                .all(|pair| segment_clear(pair[0], pair[1], obstacles)),
            "{points:?} crosses a card"
        );
    }

    #[test]
    fn custom_routes_attach_to_rectangle_borders_with_orthogonal_segments() {
        let from = person("from", 0.0, 0.0);
        let to = person("to", 300.0, 120.0);
        let link = routed_link();
        let route = route(&from, &to, &link, &[from.clone(), to.clone()]);

        assert_on_border(route.points[0], &from);
        assert_on_border(*route.points.last().unwrap(), &to);
        assert!(is_orthogonal(&route.points));
        assert_eq!(route.custom_bends.len(), link.route.len());
        for (point_index, stored_index) in route.custom_bends {
            assert_eq!(
                route.points[point_index],
                Point::new(link.route[stored_index][0], link.route[stored_index][1])
            );
        }
    }

    #[test]
    fn router_keeps_zero_one_and_multiple_user_bends_orthogonal() {
        let from = person("from", 0.0, 0.0);
        let to = person("to", 320.0, 180.0);
        for bends in [
            Vec::new(),
            vec![[180.0, 70.0]],
            vec![[180.0, 70.0], [220.0, 100.0], [260.0, 150.0]],
        ] {
            let link = Link {
                route: bends.clone(),
                ..routed_link()
            };
            let route = route(&from, &to, &link, &[from.clone(), to.clone()]);

            assert_on_border(route.points[0], &from);
            assert_on_border(*route.points.last().unwrap(), &to);
            assert!(is_orthogonal(&route.points));
            assert_eq!(route.custom_bends.len(), bends.len());
        }
    }

    #[test]
    fn custom_route_endpoints_follow_people_and_select_the_nearest_new_edges() {
        let from = person("from", 0.0, 0.0);
        let to = person("to", 300.0, 120.0);
        let link = routed_link();

        let first_route = route(&from, &to, &link, &[from.clone(), to.clone()]);
        let moved_to = person("to", 240.0, 100.0);
        let moved_route = route(&from, &moved_to, &link, &[from.clone(), moved_to.clone()]);

        assert_ne!(first_route.points.last(), moved_route.points.last());
        assert_on_border(*moved_route.points.last().unwrap(), &moved_to);
        assert!((moved_route.points.last().unwrap().x - moved_to.x).abs() < EPS);
        assert!(is_orthogonal(&moved_route.points));
    }

    #[test]
    fn horizontally_approaching_custom_route_uses_the_destination_side() {
        let from = person("from", 0.0, 0.0);
        let to = person("to", 300.0, 120.0);
        let mut link = routed_link();
        link.route = vec![[260.0, 150.0], [280.0, 150.0]];

        let route = route(&from, &to, &link, &[from.clone(), to.clone()]);
        let end = *route.points.last().unwrap();

        assert!((end.x - to.x).abs() < EPS);
        assert!(end.y >= to.y && end.y <= to.y + NODE_H);
        assert!(is_orthogonal(&route.points));
    }

    #[test]
    fn automatic_route_detours_around_intervening_people() {
        let from = person("from", 0.0, 0.0);
        let to = person("to", 360.0, 0.0);
        let blocker = person("blocker", 180.0, -4.0);
        let link = Link {
            route: Vec::new(),
            ..routed_link()
        };
        let people = [from.clone(), to.clone(), blocker];
        let route = route(&from, &to, &link, &people);

        assert!(route.points.len() > 2);
        assert_route_avoids_cards(&route.points, &obstacle_rects(&people));
        assert_on_border(*route.points.first().unwrap(), &from);
        assert_on_border(*route.points.last().unwrap(), &to);
    }

    #[test]
    fn moved_endpoint_arrows_remain_on_the_side_facing_the_route() {
        let from = person("from", 0.0, 0.0);
        let to = person("to", 260.0, 220.0);
        let link = Link {
            route: vec![[220.0, 242.0], [230.0, 242.0]],
            ..routed_link()
        };
        let route = route(&from, &to, &link, &[from.clone(), to.clone()]);
        let end = *route.points.last().unwrap();

        assert!((end.x - to.x).abs() < EPS);
        assert!(end.y >= to.y && end.y <= to.y + NODE_H);
        assert!(route.points[route.points.len() - 2].x < end.x);
    }

    #[test]
    fn edited_waypoints_keep_the_line_on_the_bottom_and_right_edges() {
        let source = person("source", 200.0, 40.0);
        let target = person("target", -60.0, 300.0);
        let link = Link {
            route: vec![[260.0, 190.0], [80.0, 323.0]],
            ..routed_link()
        };

        let route = route(&source, &target, &link, &[source.clone(), target.clone()]);

        let start = route.points[0];
        let end = *route.points.last().unwrap();
        assert!((start.y - (source.y + NODE_H)).abs() < EPS);
        assert!((start.x - 260.0).abs() < EPS);
        assert!((end.x - (target.x + NODE_W)).abs() < EPS);
        assert!((end.y - 323.0).abs() < EPS);
        assert!(is_orthogonal(&route.points));
    }

    #[test]
    fn custom_routes_detour_around_people_instead_of_drawing_through_cards() {
        let source = person("source", 0.0, 0.0);
        let target = person("target", 360.0, 0.0);
        let blocker = person("blocker", 180.0, -4.0);
        let link = Link {
            route: vec![[180.0, 19.0], [180.0, 80.0]],
            start_port: Some(1),
            end_port: Some(0),
            ..routed_link()
        };
        let people = [source.clone(), target.clone(), blocker];
        let route = route(&source, &target, &link, &people);
        assert!(
            route
                .points
                .iter()
                .any(|point| (point.x - 170.0).abs() < EPS && (point.y - 19.0).abs() < EPS)
        );
        assert_route_avoids_cards(&route.points, &obstacle_rects(&people));
    }

    #[test]
    fn reshaping_keeps_the_endpoint_ports_selected_before_the_drag() {
        let source = person("source", 200.0, 40.0);
        let target = person("target", -60.0, 300.0);
        let link = Link {
            route: vec![[320.0, 150.0], [140.0, 323.0]],
            start_port: Some(3),
            end_port: Some(1),
            ..routed_link()
        };
        let route = route(&source, &target, &link, &[source.clone(), target.clone()]);

        assert_eq!(route.start_port, 3);
        assert_eq!(route.end_port, 1);
        assert!((route.points[0].y - (source.y + NODE_H)).abs() < EPS);
        assert!((route.points.last().unwrap().x - (target.x + NODE_W)).abs() < EPS);
    }

    #[test]
    fn dragged_handle_controls_the_connection_preview_port() {
        let source = person("source", 0.0, 0.0);
        let obstacles = Obstacles::build(std::slice::from_ref(&source));
        let preview =
            connection_preview(&source, Point::new(-100.0, 100.0), None, &obstacles, Some(0));

        assert!((preview[0].x - source.x).abs() < EPS);
        assert!(preview[0].y >= source.y && preview[0].y <= source.y + NODE_H);
    }

    #[test]
    fn search_counts_the_bend_needed_to_leave_a_port() {
        // the right port faces the target: a straight line beats every other port pair
        let from = person("from", 0.0, 0.0);
        let to = person("to", 400.0, 0.0);
        let route = route(&from, &to, &link("l", "from", "to"), &[from.clone(), to.clone()]);

        assert_eq!(route.points.len(), 2);
        assert_eq!(route.start_port, 1);
        assert_eq!(route.end_port, 0);
    }

    #[test]
    fn walled_in_targets_fall_back_to_an_orthogonal_line() {
        let from = person("from", 0.0, 0.0);
        let to = person("to", 600.0, 0.0);
        let mut people = vec![from.clone(), to.clone()];
        // a closed ring of cards around "to"
        for k in 0..6 {
            let x = 440.0 + k as f32 * 100.0;
            people.push(person(&format!("top{k}"), x, -120.0));
            people.push(person(&format!("bottom{k}"), x, 120.0));
        }
        for k in 0..3 {
            let y = -120.0 + k as f32 * 60.0 + 60.0;
            people.push(person(&format!("left{k}"), 440.0, y - 60.0));
            people.push(person(&format!("right{k}"), 940.0, y - 60.0));
        }
        let route = route(&from, &to, &link("l", "from", "to"), &people);
        assert!(is_orthogonal(&route.points));
    }

    fn sample_data(people: Vec<Person>, links: Vec<Link>) -> Data {
        Data::from_parts(people, Vec::new(), links, Vec::new())
    }

    #[test]
    fn cached_routes_match_fresh_routes_after_incremental_moves() {
        let mut people = Vec::new();
        for row in 0..4 {
            for column in 0..5 {
                people.push(person(
                    &format!("p{row}{column}"),
                    column as f32 * 160.0,
                    row as f32 * 140.0,
                ));
            }
        }
        let mut links = Vec::new();
        for k in 0..12 {
            let a = format!("p{}{}", k % 4, k % 5);
            let b = format!("p{}{}", (k + 2) % 4, (k * 3 + 1) % 5);
            if a != b {
                links.push(link(&format!("l{k}"), &a, &b));
            }
        }
        let mut data = sample_data(people, links);
        let mut cached = Routes::default();
        cached.sync(&data);

        for step in 0..6 {
            let i = (step * 7) % data.people.len();
            data.people[i].x += 37.0;
            data.people[i].y -= 23.0;
            cached.sync(&data);

            let mut fresh = Routes::default();
            fresh.sync(&data);
            for link in &data.links {
                assert_eq!(
                    cached.get(&link.id).map(|r| &r.points),
                    fresh.get(&link.id).map(|r| &r.points),
                    "step {step}: {} differs",
                    link.id
                );
            }
        }
    }

    #[test]
    fn nudging_separates_overlapping_lines_and_keeps_them_clear() {
        // two parallel links between the same pair of rows share one channel
        let people = vec![
            person("a", 0.0, 0.0),
            person("b", 160.0, 0.0),
            person("c", 0.0, 200.0),
            person("d", 160.0, 200.0),
        ];
        let links = vec![link("ad", "a", "d"), link("bc", "b", "c")];
        let data = sample_data(people.clone(), links);
        let mut routes = Routes::default();
        routes.sync(&data);

        let first = routes.get("ad").unwrap();
        let second = routes.get("bc").unwrap();
        let obstacles = obstacle_rects(&people);
        for route in [first, second] {
            assert!(is_orthogonal(&route.points));
            assert!(
                route
                    .points
                    .windows(2)
                    .skip(1)
                    .take(route.points.len().saturating_sub(3))
                    .all(|pair| segment_clear(pair[0], pair[1], &obstacles))
            );
        }
        let horizontal = |route: &Route| -> Vec<(f32, f32, f32)> {
            route
                .points
                .windows(2)
                .filter(|pair| (pair[0].y - pair[1].y).abs() < EPS)
                .map(|pair| (pair[0].y, pair[0].x.min(pair[1].x), pair[0].x.max(pair[1].x)))
                .collect()
        };
        for (y1, a1, b1) in horizontal(first) {
            for (y2, a2, b2) in horizontal(second) {
                let shared = a1.max(a2) < b1.min(b2) - EPS;
                assert!(
                    !shared || (y1 - y2).abs() > 1.0,
                    "lines overlap at y={y1}: {:?} / {:?}",
                    first.points,
                    second.points
                );
            }
        }
    }

    #[test]
    fn legs_never_fold_back_onto_the_segment_that_reached_a_bend() {
        let obstacles = Obstacles::default();
        let found = find_path(
            &[Terminal {
                back: Some(Point::new(0.0, 100.0)),
                ..Terminal::free(Point::ORIGIN)
            }],
            &[Terminal::free(Point::new(0.0, 200.0))],
            &obstacles,
            &mut Window::default(),
        );
        let first_step = found.points[1];
        assert!(
            !((first_step.x).abs() < EPS && first_step.y > 0.0),
            "{:?} folds back",
            found.points
        );
        assert!(is_orthogonal(&found.points));
    }

    #[test]
    fn a_stored_port_is_replaced_when_the_first_bend_moves_beside_the_card() {
        // the line was locked to the bottom edge, then its first segment was dragged up
        // next to the card's right side (video: loop under "Maria")
        let source = person("source", 200.0, 40.0);
        let target = person("target", 200.0, 400.0);
        let link = Link {
            route: vec![[360.0, 60.0], [360.0, 300.0]],
            start_port: Some(3),
            end_port: Some(2),
            ..routed_link()
        };
        let route = route(&source, &target, &link, &[source.clone(), target.clone()]);

        assert_eq!(route.start_port, 1);
        assert!((route.points[0].x - (source.x + NODE_W)).abs() < EPS);
        assert!(is_orthogonal(&route.points));
    }

    #[test]
    fn nudging_never_stretches_a_u_shaped_detour() {
        // video: the last segment into "Oliver" was pulled up to the middle of the gap
        let cards = [
            person("a", 0.0, 0.0),
            person("b", 300.0, 300.0),
            person("c", 200.0, 500.0),
        ];
        let obstacles = Obstacles::build(&cards);
        let mut lines = vec![Line {
            points: vec![
                Point::new(120.0, 23.0),
                Point::new(160.0, 23.0),
                Point::new(160.0, 400.0),
                Point::new(330.0, 400.0),
                Point::new(330.0, 346.0),
            ],
            fixed: vec![false; 5],
            cards: [card(&cards[0]), card(&cards[1])],
        }];
        let mut scratch = Vec::new();
        nudge(&mut lines, &obstacles, Orient::Horizontal, &mut scratch);

        let bottom_y = lines[0].points[2].y;
        assert!(bottom_y <= 400.0 + NUDGE_U_SLACK, "moved to {bottom_y}");
        assert!(bottom_y >= 346.0 + NUDGE_MIN_LEG);
    }

    #[test]
    fn user_edited_lines_are_not_nudged() {
        let people = vec![person("a", 0.0, 0.0), person("b", 300.0, 300.0)];
        let mut edited = link("edited", "a", "b");
        edited.route = vec![[200.0, 23.0], [200.0, 323.0]];
        let data = sample_data(people.clone(), vec![edited.clone(), link("auto", "a", "b")]);
        let mut routes = Routes::default();
        routes.sync(&data);

        let alone = route(&people[0], &people[1], &edited, &people);
        assert_eq!(routes.get("edited").unwrap().points, alone.points);
    }

    #[test]
    fn automatic_labels_avoid_each_other() {
        let people = vec![person("a", 0.0, 0.0), person("b", 400.0, 0.0)];
        let links = vec![link("one", "a", "b"), link("two", "a", "b")];
        let data = sample_data(people, links);
        let mut routes = Routes::default();
        routes.sync(&data);
        let width = label_width("related to", LABEL_TEXT);
        let rect = |id: &str| {
            let route = routes.get(id).unwrap();
            label_rect(route.label_at, route.label_centered, width)
        };
        assert!(!overlaps(&rect("one"), &rect("two")));
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use std::time::Instant;

    /// `cargo test --release bench_demo_board -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench_demo_board() {
        let (clans, people, links) = crate::demo::generate(40, 250);
        let mut data = Data::from_parts(people, clans, links, Vec::new());
        let t = Instant::now();
        crate::layout::auto_layout(&mut data);
        println!("auto_layout: {} people in {:?}", data.people.len(), t.elapsed());

        let mut routes = Routes::default();
        let t = Instant::now();
        routes.sync(&data);
        println!("full routing: {} lines in {:?}", data.links.len(), t.elapsed());

        let t = Instant::now();
        routes.sync(&data);
        println!("sync without changes: {:?}", t.elapsed());

        let t = Instant::now();
        let mut lines = Vec::new();
        for link in &data.links {
            lines.push(routes.get(&link.id).unwrap().points.len());
        }
        let mut total = std::time::Duration::ZERO;
        let steps = 60;
        for step in 0..steps {
            let i = (step * 997) % data.people.len();
            data.people[i].x += 7.0;
            data.people[i].y += 3.0;
            let t = Instant::now();
            routes.sync(&data);
            total += t.elapsed();
        }
        println!("drag step (1 card moved): {:?} avg", total / steps as u32);
        let _ = t;

        let t = Instant::now();
        let mut obstacles = Obstacles::default();
        obstacles = std::mem::replace(&mut obstacles, Obstacles::build(&data.people));
        let _ = obstacles;
        println!("obstacle index build: {:?}", t.elapsed());
    }
}
