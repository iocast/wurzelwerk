//! Deterministic layered family layout (Sugiyama framework).
//!
//! 1. **Peers** - people joined by undirected lines (married, siblings, …) form one
//!    component, placed side by side on one row; partners end up next to each other.
//! 2. **Layering** - longest-path generations over the parent → child DAG of the
//!    components. Root components are pulled down next to their first children so that
//!    they do not float far above them; parent cycles stay on the root row.
//! 3. **Crossing reduction** - per clan group: DFS initial order, then alternating
//!    barycentre sweeps with adjacent transpositions. Crossings are counted exactly
//!    (inversions between layer pairs) and the best ordering seen is kept.
//! 4. **Coordinates** - each row is a 1-D quadratic program "sit at the mean of your
//!    parents and children" under minimum-separation constraints, solved exactly with
//!    pool-adjacent-violators; alternating down/up sweeps converge to straight lines
//!    with parents centred over their children.
//! 5. **Packing** - clan groups are packed with a skyline bottom-left packer into a
//!    strip with roughly the aspect ratio of the board.

use std::collections::{HashSet, VecDeque};

use crate::model::{Data, LinkStyle, NODE_H, NODE_W, is_parent_label, link_style};

const GAP_X: f32 = 34.0;
const GAP_Y: f32 = 86.0;
const GAP_BLOCK: f32 = 120.0;
/// Maximum ordering sweeps, and sweeps without improvement before giving up.
const ORDER_PASSES: usize = 24;
const ORDER_PATIENCE: usize = 4;
const TRANSPOSE_ROUNDS: usize = 8;
const COORDINATE_PASSES: usize = 12;
/// Pull of a component without parents/children toward its current place.
const ISOLATED_WEIGHT: f32 = 0.05;
/// Width / height of the strip the clan groups are packed into.
const BOARD_ASPECT: f32 = 1.6;

pub fn auto_layout(data: &mut Data) {
    let person_count = data.people.len();
    if person_count == 0 {
        return;
    }

    // 1. peers -------------------------------------------------------------
    let mut union = UnionFind::new(person_count);
    let mut parent_person_edges = Vec::new();
    let mut peers = vec![Vec::new(); person_count];
    for link in &data.links {
        let (Some(&from), Some(&to)) = (
            data.person_idx.get(&link.from),
            data.person_idx.get(&link.to),
        ) else {
            continue;
        };
        if from == to {
            continue;
        }
        if is_parent_label(&link.label) {
            parent_person_edges.push((from, to));
        } else if link_style(&link.label) == LinkStyle::Undirected {
            union.join(from, to);
            peers[from].push(to);
            peers[to].push(from);
        }
    }
    for list in &mut peers {
        list.sort_unstable();
        list.dedup();
    }

    let mut members = Vec::with_capacity(person_count);
    let mut root_to_component = vec![usize::MAX; person_count];
    for person in 0..person_count {
        let root = union.root(person);
        if root_to_component[root] == usize::MAX {
            root_to_component[root] = members.len();
            members.push(Vec::new());
        }
        members[root_to_component[root]].push(person);
    }
    let component_of: Vec<usize> = (0..person_count)
        .map(|person| root_to_component[union.root(person)])
        .collect();
    let component_count = members.len();

    let mut children = vec![Vec::new(); component_count];
    let mut parents = vec![Vec::new(); component_count];
    let mut person_parents = vec![Vec::new(); person_count];
    let mut unique_edges = HashSet::new();
    for &(from, to) in &parent_person_edges {
        person_parents[to].push(from);
        let (parent, child) = (component_of[from], component_of[to]);
        if parent == child || !unique_edges.insert((parent, child)) {
            continue;
        }
        children[parent].push(child);
        parents[child].push(parent);
    }

    // 2. layering ------------------------------------------------------------
    let generations = generations(&parents, &children);

    let no_clan = data.clans.len();
    let component_group: Vec<usize> = members
        .iter()
        .map(|people| {
            people
                .iter()
                .filter_map(|&person| {
                    data.people[person]
                        .clans
                        .iter()
                        .filter_map(|clan| data.clan_idx.get(clan).copied())
                        .min()
                })
                .min()
                .unwrap_or(no_clan)
        })
        .collect();
    let group_count = no_clan + 1;
    let mut groups = vec![Vec::new(); group_count];
    for component in 0..component_count {
        groups[component_group[component]].push(component);
    }

    let names: Vec<String> = members
        .iter()
        .map(|people| {
            people
                .iter()
                .map(|&person| data.people[person].name.to_lowercase())
                .min()
                .unwrap_or_default()
        })
        .collect();
    let widths: Vec<f32> = members
        .iter()
        .map(|people| people.len() as f32 * (NODE_W + GAP_X) - GAP_X)
        .collect();

    // 3. + 4. per clan group ---------------------------------------------------
    let mut state = Placement {
        centre: vec![0.0; component_count],
        row_of: vec![0; component_count],
        slot: vec![0; component_count],
        rank: vec![usize::MAX; component_count],
    };
    let mut group_rows = vec![Vec::new(); group_count];
    let mut extents = vec![(0.0f32, 0.0f32); group_count];
    for (group, components) in groups.iter().enumerate() {
        if components.is_empty() {
            continue;
        }
        let graph = Graph {
            parents: &parents,
            children: &children,
            widths: &widths,
            group_of: &component_group,
            group,
        };
        let mut rows = state.initial_rows(components, &generations, &graph, &names);
        state.reduce_crossings(&mut rows, &graph);
        state.assign_coordinates(&rows, &graph);

        let left = components
            .iter()
            .map(|&c| state.centre[c] - widths[c] / 2.0)
            .fold(f32::INFINITY, f32::min);
        let mut width: f32 = 0.0;
        for &c in components {
            state.centre[c] -= left;
            width = width.max(state.centre[c] + widths[c] / 2.0);
        }
        extents[group] = (width, rows.len() as f32 * (NODE_H + GAP_Y) - GAP_Y);
        group_rows[group] = rows;
    }

    // 5. packing -----------------------------------------------------------------
    let origins = pack(&extents);
    for (group, rows) in group_rows.iter().enumerate() {
        let (origin_x, origin_y) = origins[group];
        for (row_index, row) in rows.iter().enumerate() {
            for &component in row {
                let parent_centre = |person: usize| {
                    let (sum, count) = person_parents[person]
                        .iter()
                        .map(|&parent| component_of[parent])
                        .filter(|&parent| component_group[parent] == group)
                        .fold((0.0, 0usize), |(sum, count), parent| {
                            (sum + state.centre[parent], count + 1)
                        });
                    (count > 0).then(|| sum / count as f32)
                };
                let ordered = order_members(&members[component], &peers, parent_centre);
                let left = state.centre[component] - widths[component] / 2.0;
                for (k, &person) in ordered.iter().enumerate() {
                    data.people[person].x = origin_x + left + k as f32 * (NODE_W + GAP_X);
                    data.people[person].y = origin_y + row_index as f32 * (NODE_H + GAP_Y);
                }
            }
        }
    }
}

/// Longest-path layers; parentless components move down next to their first child.
fn generations(parents: &[Vec<usize>], children: &[Vec<usize>]) -> Vec<i32> {
    let count = parents.len();
    let mut generation = vec![0i32; count];
    let mut indegree: Vec<usize> = parents.iter().map(Vec::len).collect();
    let mut ready: VecDeque<usize> = (0..count).filter(|&c| indegree[c] == 0).collect();
    let mut topological = Vec::with_capacity(count);
    while let Some(parent) = ready.pop_front() {
        topological.push(parent);
        for &child in &children[parent] {
            generation[child] = generation[child].max(generation[parent] + 1);
            indegree[child] -= 1;
            if indegree[child] == 0 {
                ready.push_back(child);
            }
        }
    }
    // Cyclic parent relationships are kept together at the root generation
    // instead of letting a bounded relaxation push them arbitrarily far away.
    let mut processed = vec![false; count];
    for &c in &topological {
        processed[c] = true;
    }
    for c in 0..count {
        if !processed[c] {
            generation[c] = 0;
        }
    }
    for &c in topological.iter().rev() {
        if parents[c].is_empty()
            && let Some(first_child) = children[c].iter().map(|&k| generation[k]).min()
        {
            generation[c] = generation[c].max(first_child - 1);
        }
    }
    generation
}

/// Parent/child adjacency restricted to one clan group.
struct Graph<'a> {
    parents: &'a [Vec<usize>],
    children: &'a [Vec<usize>],
    widths: &'a [f32],
    group_of: &'a [usize],
    group: usize,
}

impl Graph<'_> {
    fn adjacent(&self, component: usize, upward: bool) -> impl Iterator<Item = usize> + '_ {
        let list = if upward {
            &self.parents[component]
        } else {
            &self.children[component]
        };
        list.iter()
            .copied()
            .filter(move |&other| self.group_of[other] == self.group)
    }

    fn neighbours(&self, component: usize) -> impl Iterator<Item = usize> + '_ {
        self.adjacent(component, true)
            .chain(self.adjacent(component, false))
    }
}

/// Per-component layout state (indexed by component).
struct Placement {
    centre: Vec<f32>,
    row_of: Vec<usize>,
    slot: Vec<usize>,
    rank: Vec<usize>,
}

impl Placement {
    /// Rows by generation, ordered by a DFS from the top row so families stay together.
    fn initial_rows(
        &mut self,
        components: &[usize],
        generations: &[i32],
        graph: &Graph,
        names: &[String],
    ) -> Vec<Vec<usize>> {
        let min_generation = components
            .iter()
            .map(|&c| generations[c])
            .min()
            .unwrap_or(0);
        for &c in components {
            self.row_of[c] = (generations[c] - min_generation) as usize;
        }
        let by_name = |a: &usize, b: &usize| names[*a].cmp(&names[*b]).then(a.cmp(b));
        let mut starts = components.to_vec();
        starts.sort_by(|a, b| self.row_of[*a].cmp(&self.row_of[*b]).then(by_name(a, b)));

        let mut counter = 0;
        let mut stack = Vec::new();
        for &start in &starts {
            stack.push(start);
            while let Some(c) = stack.pop() {
                if self.rank[c] != usize::MAX {
                    continue;
                }
                self.rank[c] = counter;
                counter += 1;
                let mut next: Vec<usize> = graph
                    .adjacent(c, false)
                    .filter(|&k| self.rank[k] == usize::MAX)
                    .collect();
                next.sort_by(|a, b| by_name(b, a));
                stack.extend(next);
            }
        }

        let row_count = components.iter().map(|&c| self.row_of[c]).max().unwrap_or(0) + 1;
        let mut rows = vec![Vec::new(); row_count];
        for &c in &starts {
            rows[self.row_of[c]].push(c);
        }
        for row in &mut rows {
            row.sort_by_key(|&c| self.rank[c]);
        }
        for row in &rows {
            self.pack_row(row, graph);
        }
        rows
    }

    /// Left-aligned packing; also refreshes the slot indices.
    fn pack_row(&mut self, row: &[usize], graph: &Graph) {
        let mut x = 0.0;
        for (slot, &c) in row.iter().enumerate() {
            self.slot[c] = slot;
            self.centre[c] = x + graph.widths[c] / 2.0;
            x += graph.widths[c] + GAP_X;
        }
    }

    fn reduce_crossings(&mut self, rows: &mut [Vec<usize>], graph: &Graph) {
        let mut best = self.crossings(rows, graph);
        let mut best_rows = rows.to_vec();
        let mut stale = 0;
        for pass in 0..ORDER_PASSES {
            if best == 0 {
                break;
            }
            let downward = pass % 2 == 0;
            let order: Vec<usize> = if downward {
                (1..rows.len()).collect()
            } else {
                (0..rows.len().saturating_sub(1)).rev().collect()
            };
            for r in order {
                let adjacent = if downward { r - 1 } else { r + 1 };
                self.sort_by_barycentre(&mut rows[r], adjacent, downward, graph);
                self.pack_row(&rows[r], graph);
            }
            self.transpose(rows, graph);
            let crossings = self.crossings(rows, graph);
            if crossings < best {
                best = crossings;
                best_rows = rows.to_vec();
                stale = 0;
            } else {
                stale += 1;
                if stale >= ORDER_PATIENCE {
                    break;
                }
            }
        }
        for (row, best_row) in rows.iter_mut().zip(best_rows) {
            *row = best_row;
            self.pack_row(row, graph);
        }
    }

    fn sort_by_barycentre(
        &self,
        row: &mut [usize],
        adjacent: usize,
        upward: bool,
        graph: &Graph,
    ) {
        let mut keyed: Vec<(f32, usize, usize)> = row
            .iter()
            .enumerate()
            .map(|(slot, &c)| {
                let (sum, count) = graph
                    .adjacent(c, upward)
                    .filter(|&n| self.row_of[n] == adjacent)
                    .fold((0.0, 0usize), |(sum, count), n| {
                        (sum + self.centre[n], count + 1)
                    });
                let key = if count == 0 {
                    self.centre[c]
                } else {
                    sum / count as f32
                };
                (key, slot, c)
            })
            .collect();
        keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (place, (_, _, c)) in row.iter_mut().zip(keyed) {
            *place = c;
        }
    }

    /// Swaps neighbours while that removes crossings with the adjacent rows.
    fn transpose(&mut self, rows: &mut [Vec<usize>], graph: &Graph) {
        for (r, row) in rows.iter_mut().enumerate() {
            for _ in 0..TRANSPOSE_ROUNDS {
                let mut improved = false;
                for i in 0..row.len().saturating_sub(1) {
                    let (u, v) = (row[i], row[i + 1]);
                    if self.pair_crossings(v, u, r, graph) < self.pair_crossings(u, v, r, graph) {
                        row.swap(i, i + 1);
                        self.slot[u] = i + 1;
                        self.slot[v] = i;
                        improved = true;
                    }
                }
                if !improved {
                    break;
                }
            }
            self.pack_row(row, graph);
        }
    }

    /// Crossings between the edges of `left` and `right` (in that order) on row `r`.
    fn pair_crossings(&self, left: usize, right: usize, r: usize, graph: &Graph) -> usize {
        let mut total = 0;
        for (upward, adjacent) in [(true, r.checked_sub(1)), (false, Some(r + 1))] {
            let Some(adjacent) = adjacent else {
                continue;
            };
            for a in graph
                .adjacent(left, upward)
                .filter(|&n| self.row_of[n] == adjacent)
            {
                total += graph
                    .adjacent(right, upward)
                    .filter(|&n| self.row_of[n] == adjacent && self.slot[n] < self.slot[a])
                    .count();
            }
        }
        total
    }

    /// Exact crossing count between consecutive rows (inversions of the edge list).
    fn crossings(&self, rows: &[Vec<usize>], graph: &Graph) -> usize {
        let mut total = 0;
        let mut edges = Vec::new();
        for r in 0..rows.len().saturating_sub(1) {
            edges.clear();
            for &upper in &rows[r] {
                for lower in graph.adjacent(upper, false) {
                    if self.row_of[lower] == r + 1 {
                        edges.push((self.slot[upper], self.slot[lower]));
                    }
                }
            }
            edges.sort_unstable();
            // Fenwick tree over lower slots: count earlier edges ending further right
            let size = rows[r + 1].len();
            let mut tree = vec![0usize; size + 1];
            for (inserted, &(_, lower)) in edges.iter().enumerate() {
                let mut at_most = 0;
                let mut i = lower + 1;
                while i > 0 {
                    at_most += tree[i];
                    i &= i - 1;
                }
                total += inserted - at_most;
                let mut i = lower + 1;
                while i <= size {
                    tree[i] += 1;
                    i += i.isolate_lowest_one();
                }
            }
        }
        total
    }

    /// Alternating sweeps; each row is solved exactly for its neighbours' mean positions.
    fn assign_coordinates(&mut self, rows: &[Vec<usize>], graph: &Graph) {
        let mut targets = Vec::new();
        let mut weights = Vec::new();
        let mut gaps = Vec::new();
        for pass in 0..COORDINATE_PASSES {
            let order: Vec<usize> = if pass % 2 == 0 {
                (0..rows.len()).collect()
            } else {
                (0..rows.len()).rev().collect()
            };
            for r in order {
                let row = &rows[r];
                if row.is_empty() {
                    continue;
                }
                targets.clear();
                weights.clear();
                gaps.clear();
                for &c in row {
                    let (sum, count) = graph
                        .neighbours(c)
                        .fold((0.0, 0usize), |(sum, count), n| {
                            (sum + self.centre[n], count + 1)
                        });
                    if count == 0 {
                        targets.push(self.centre[c]);
                        weights.push(ISOLATED_WEIGHT);
                    } else {
                        targets.push(sum / count as f32);
                        weights.push(count as f32);
                    }
                }
                for pair in row.windows(2) {
                    gaps.push((graph.widths[pair[0]] + graph.widths[pair[1]]) / 2.0 + GAP_X);
                }
                for (&c, x) in row.iter().zip(solve_row(&targets, &weights, &gaps)) {
                    self.centre[c] = x;
                }
            }
        }
    }
}

/// Minimises `Σ wᵢ (xᵢ - tᵢ)²` subject to `xᵢ₊₁ - xᵢ ≥ gapᵢ` (order preserving).
///
/// Substituting `yᵢ = xᵢ - Σ_{k<i} gapₖ` turns it into isotonic regression, solved
/// exactly in O(n) by pool-adjacent-violators.
fn solve_row(targets: &[f32], weights: &[f32], gaps: &[f32]) -> Vec<f32> {
    let n = targets.len();
    let mut offsets = Vec::with_capacity(n);
    let mut offset = 0.0;
    for i in 0..n {
        offsets.push(offset);
        offset += gaps.get(i).copied().unwrap_or(0.0);
    }
    // blocks of (weighted sum, weight, length)
    let mut blocks: Vec<(f32, f32, usize)> = Vec::with_capacity(n);
    for i in 0..n {
        let mut block = (weights[i] * (targets[i] - offsets[i]), weights[i], 1);
        while let Some(&(sum, weight, len)) = blocks.last() {
            if sum / weight < block.0 / block.1 {
                break;
            }
            blocks.pop();
            block = (sum + block.0, weight + block.1, len + block.2);
        }
        blocks.push(block);
    }
    let mut positions = Vec::with_capacity(n);
    for (sum, weight, len) in blocks {
        let y = sum / weight;
        for _ in 0..len {
            positions.push(y + offsets[positions.len()]);
        }
    }
    positions
}

/// Members of a peer component left to right: a walk along the peer links that starts at
/// an outer member (partners stay adjacent), flipped so people sit on their parents' side.
fn order_members(
    members: &[usize],
    peers: &[Vec<usize>],
    parent_centre: impl Fn(usize) -> Option<f32>,
) -> Vec<usize> {
    if members.len() <= 1 {
        return members.to_vec();
    }
    let inside = |person: &usize| members.binary_search(person).is_ok();
    let degree = |person: usize| peers[person].iter().filter(|p| inside(p)).count();
    let mut visited = HashSet::with_capacity(members.len());
    let mut ordered = Vec::with_capacity(members.len());
    let start = members
        .iter()
        .copied()
        .min_by_key(|&person| (degree(person), person))
        .unwrap_or(members[0]);
    let mut stack = vec![start];
    while ordered.len() < members.len() {
        let Some(person) = stack.pop().or_else(|| {
            members
                .iter()
                .copied()
                .find(|person| !visited.contains(person))
        }) else {
            break;
        };
        if !visited.insert(person) {
            continue;
        }
        ordered.push(person);
        let mut next: Vec<usize> = peers[person]
            .iter()
            .copied()
            .filter(|p| inside(p) && !visited.contains(p))
            .collect();
        // leaves (e.g. a spouse) first, so they end up right next to their partner
        next.sort_by_key(|&p| std::cmp::Reverse((degree(p), p)));
        stack.extend(next);
    }

    let half = ordered.len() / 2;
    let side_mean = |people: &[usize]| {
        let (sum, count) = people
            .iter()
            .filter_map(|&person| parent_centre(person))
            .fold((0.0, 0usize), |(sum, count), x| (sum + x, count + 1));
        (count > 0).then(|| sum / count as f32)
    };
    if let (Some(left), Some(right)) = (
        side_mean(&ordered[..half]),
        side_mean(&ordered[ordered.len() - half..]),
    ) && left > right
    {
        ordered.reverse();
    }
    ordered
}

/// Skyline bottom-left packing of the group rectangles (tallest first).
fn pack(extents: &[(f32, f32)]) -> Vec<(f32, f32)> {
    const EPS: f32 = 0.01;
    let mut origins = vec![(0.0f32, 0.0f32); extents.len()];
    let mut items: Vec<usize> = (0..extents.len())
        .filter(|&group| extents[group].0 > 0.0)
        .collect();
    items.sort_by(|&a, &b| {
        extents[b]
            .1
            .total_cmp(&extents[a].1)
            .then(extents[b].0.total_cmp(&extents[a].0))
            .then(a.cmp(&b))
    });
    let size = |group: usize| (extents[group].0 + GAP_BLOCK, extents[group].1 + GAP_BLOCK);
    let area: f32 = items
        .iter()
        .map(|&group| size(group).0 * size(group).1)
        .sum();
    let widest = items
        .iter()
        .map(|&group| size(group).0)
        .fold(0.0, f32::max);
    let strip = (area * BOARD_ASPECT).sqrt().max(widest);

    // skyline segments (x, width, height), sorted by x and covering 0..strip
    let mut skyline: Vec<(f32, f32, f32)> = vec![(0.0, strip, 0.0)];
    for group in items {
        let (w, h) = size(group);
        let mut best: Option<(f32, f32)> = None;
        for &(x, _, _) in &skyline {
            if x + w > strip + EPS {
                break;
            }
            let y = skyline
                .iter()
                .filter(|&&(sx, sw, _)| sx < x + w - EPS && sx + sw > x + EPS)
                .map(|&(_, _, sy)| sy)
                .fold(0.0, f32::max);
            if best.is_none_or(|(by, bx)| y < by - EPS || ((y - by).abs() <= EPS && x < bx)) {
                best = Some((y, x));
            }
        }
        let (y, x) = best.unwrap_or((0.0, 0.0));
        origins[group] = (x, y);

        let mut next = Vec::with_capacity(skyline.len() + 2);
        for &(sx, sw, sy) in &skyline {
            let end = sx + sw;
            if end <= x + EPS || sx >= x + w - EPS {
                next.push((sx, sw, sy));
                continue;
            }
            if sx < x {
                next.push((sx, x - sx, sy));
            }
            if end > x + w {
                next.push((x + w, end - x - w, sy));
            }
        }
        next.push((x, w, y + h));
        next.sort_by(|a, b| a.0.total_cmp(&b.0));
        skyline.clear();
        for segment in next {
            match skyline.last_mut() {
                Some(last) if (last.2 - segment.2).abs() <= EPS => last.1 += segment.1,
                _ => skyline.push(segment),
            }
        }
    }
    origins
}

struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(len: usize) -> Self {
        Self {
            parent: (0..len).collect(),
            rank: vec![0; len],
        }
    }

    fn root(&mut self, item: usize) -> usize {
        let mut root = item;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        let mut item = item;
        while self.parent[item] != root {
            let next = self.parent[item];
            self.parent[item] = root;
            item = next;
        }
        root
    }

    fn join(&mut self, left: usize, right: usize) {
        let (mut left, mut right) = (self.root(left), self.root(right));
        if left == right {
            return;
        }
        if self.rank[left] < self.rank[right] {
            std::mem::swap(&mut left, &mut right);
        }
        self.parent[right] = left;
        if self.rank[left] == self.rank[right] {
            self.rank[left] += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Clan, Link, Person};

    fn person(id: &str, name: &str) -> Person {
        Person {
            id: id.into(),
            name: name.into(),
            born: String::new(),
            died: String::new(),
            x: 0.0,
            y: 0.0,
            clans: Vec::new(),
        }
    }

    fn link(id: &str, from: &str, to: &str, label: &str) -> Link {
        Link {
            id: id.into(),
            from: from.into(),
            to: to.into(),
            label: label.into(),
            route: Vec::new(),
            start_port: None,
            end_port: None,
            label_position: None,
        }
    }

    fn rects_overlap(a: &Person, b: &Person) -> bool {
        a.x < b.x + NODE_W && b.x < a.x + NODE_W && a.y < b.y + NODE_H && b.y < a.y + NODE_H
    }

    #[test]
    fn auto_layout_keeps_partners_together_and_places_children_below() {
        let mut data = Data::from_parts(
            vec![
                person("a", "Ada"),
                person("b", "Bob"),
                person("c", "Child"),
                person("d", "Unrelated"),
            ],
            vec![Clan {
                id: "family".into(),
                name: "Family".into(),
                color: [0.2, 0.4, 0.6],
                label_offset: [0.0, 0.0],
            }],
            vec![
                link("partners", "a", "b", "married"),
                link("mother", "a", "c", "mother of"),
                link("father", "b", "c", "father of"),
            ],
            Vec::new(),
        );
        data.people[0].clans.push("family".into());
        data.people[1].clans.push("family".into());
        data.people[2].clans.push("family".into());

        auto_layout(&mut data);

        assert_eq!(data.people[0].y, data.people[1].y);
        assert!(data.people[2].y > data.people[0].y);
        let first_positions: Vec<_> = data
            .people
            .iter()
            .map(|person| (person.x, person.y))
            .collect();
        auto_layout(&mut data);
        assert_eq!(
            first_positions,
            data.people
                .iter()
                .map(|person| (person.x, person.y))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn cyclic_parent_links_do_not_create_unbounded_generations() {
        let mut data = Data::from_parts(
            vec![person("a", "Ada"), person("b", "Bob"), person("c", "Child")],
            Vec::new(),
            vec![
                link("ab", "a", "b", "parent of"),
                link("ba", "b", "a", "parent of"),
                link("bc", "b", "c", "parent of"),
            ],
            Vec::new(),
        );

        auto_layout(&mut data);

        assert!(
            data.people
                .iter()
                .all(|person| person.y.is_finite() && person.y < 1000.0)
        );
    }

    #[test]
    fn parents_are_centred_over_their_children() {
        let mut data = Data::from_parts(
            vec![
                person("a", "Ada"),
                person("b", "Bob"),
                person("k1", "Kim"),
                person("k2", "Lou"),
                person("k3", "Max"),
            ],
            Vec::new(),
            vec![
                link("m", "a", "b", "married"),
                link("1", "a", "k1", "parents of"),
                link("2", "a", "k2", "parents of"),
                link("3", "a", "k3", "parents of"),
            ],
            Vec::new(),
        );

        auto_layout(&mut data);

        let couple = (data.people[0].x + data.people[1].x) / 2.0;
        let kids = (data.people[2].x + data.people[4].x) / 2.0;
        assert!((couple - kids).abs() < 1.0, "couple {couple} kids {kids}");
    }

    #[test]
    fn crossing_reduction_untangles_swapped_families() {
        // two couples whose children are listed in the "wrong" order by name
        let mut data = Data::from_parts(
            vec![
                person("a", "Anna"),
                person("b", "Ben"),
                person("c", "Carl"),
                person("d", "Dora"),
                person("x", "Zoe"),
                person("y", "Yan"),
                person("z", "Abe"),
                person("w", "Ada"),
            ],
            Vec::new(),
            vec![
                link("ab", "a", "b", "married"),
                link("cd", "c", "d", "married"),
                link("1", "a", "x", "parents of"),
                link("2", "a", "y", "parents of"),
                link("3", "c", "z", "parents of"),
                link("4", "c", "w", "parents of"),
            ],
            Vec::new(),
        );

        auto_layout(&mut data);

        let x = |i: usize| data.people[i].x;
        let left_family = x(0) < x(2);
        let first_kids = x(4).max(x(5));
        let second_kids = x(6).min(x(7));
        assert_eq!(left_family, first_kids < second_kids);
    }

    #[test]
    fn layout_never_overlaps_cards() {
        let (clans, people, links) = crate::demo::generate(3, 60);
        let mut data = Data::from_parts(people, clans, links, Vec::new());

        auto_layout(&mut data);

        for (i, a) in data.people.iter().enumerate() {
            for b in &data.people[i + 1..] {
                assert!(!rects_overlap(a, b), "{} overlaps {}", a.name, b.name);
            }
        }
    }

    #[test]
    fn row_solver_respects_separation_and_targets() {
        let xs = solve_row(&[0.0, 0.0, 100.0], &[1.0, 1.0, 1.0], &[10.0, 10.0]);
        assert!((xs[1] - xs[0] - 10.0).abs() < 1e-4);
        assert!((xs[0] + xs[1]) / 2.0 < 1e-4);
        assert!((xs[2] - 100.0).abs() < 1e-4);
    }

    #[test]
    fn skyline_packing_never_overlaps_groups() {
        let extents = [(400.0, 300.0), (200.0, 500.0), (300.0, 100.0), (150.0, 150.0)];
        let origins = pack(&extents);
        for i in 0..extents.len() {
            for j in i + 1..extents.len() {
                let (a, b) = (origins[i], origins[j]);
                let overlap = a.0 < b.0 + extents[j].0
                    && b.0 < a.0 + extents[i].0
                    && a.1 < b.1 + extents[j].1
                    && b.1 < a.1 + extents[i].1;
                assert!(!overlap, "{i} overlaps {j}");
            }
        }
    }
}
