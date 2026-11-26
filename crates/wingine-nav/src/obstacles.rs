//! Boxes on the plane, sorted into square cells, so that a question about a place or a way asks
//! only the boxes near it.
//!
//! Every answer is the one asking every box in order of ids would give, to the bit, whatever the
//! cells' size: the cells only pass over boxes that couldn't change it. Each cell lists, by id,
//! every box within the index's reach of it, and a little more, which rounding can't get past. A
//! question about a place, within that reach, asks the boxes of the place's cell alone; a
//! question about a way asks those of the cells it crosses, from its start, near enough the way to
//! be entered. A question asked outside the index's bounds, or further than its reach, asks every
//! box, and so does a question about a way among so few boxes that walking the cells would cost
//! more.

use crate::area::{Area, Point};

/// Up to how many boxes a question about a way asks every box, as asking them costs less than
/// walking the cells: measured by `benches/nav.rs`.
const FEW: usize = 24;

/// Boxes on the plane, with their sides along the axes, indexed in square cells.
#[derive(Clone, Debug)]
pub struct Obstacles {
    bounds: Area,
    cell: f32,
    reach: f32,
    /// How much further than `reach` each cell lists boxes, for rounding.
    slack: f32,
    columns: usize,
    rows: usize,
    /// Every box added, by id, and whether it's still there.
    areas: Vec<Area>,
    present: Vec<bool>,
    live: usize,
    /// The ids of the boxes near each cell, ascending, cell by cell, row by row: each cell's run
    /// of `items` starts at `starts[cell]` and ends where the next one's does.
    starts: Vec<u32>,
    items: Vec<u32>,
    /// Counts every box added and removed.
    version: u64,
}

impl Obstacles {
    /// An empty index over `bounds`, in square cells `cell` across, for questions about bodies
    /// that reach no further than `reach` from their middle.
    ///
    /// # Panics
    ///
    /// If `cell` isn't more than 0, or `reach` is less than 0, or either isn't finite.
    pub fn new(bounds: Area, cell: f32, reach: f32) -> Self {
        assert!(
            cell > 0.0 && cell.is_finite(),
            "a cell must be wider than 0, not {cell}"
        );
        assert!(
            reach >= 0.0 && reach.is_finite(),
            "the reach can't be less than 0: {reach}"
        );
        // Truncating a float to an integer is exact.
        let columns = ((bounds.max.x - bounds.min.x) / cell) as usize + 1;
        let rows = ((bounds.max.y - bounds.min.y) / cell) as usize + 1;
        let largest = bounds
            .min
            .x
            .abs()
            .max(bounds.max.x.abs())
            .max(bounds.min.y.abs())
            .max(bounds.max.y.abs());
        Self {
            bounds,
            cell,
            reach,
            slack: (largest + cell + reach) * 1e-4,
            columns,
            rows,
            areas: Vec::new(),
            present: Vec::new(),
            live: 0,
            starts: vec![0; columns * rows + 1],
            items: Vec::new(),
            version: 0,
        }
    }

    /// The area it indexes.
    pub fn bounds(&self) -> Area {
        self.bounds
    }

    /// How far from their middle the bodies it's asked about can reach and still be answered
    /// from the cells.
    pub fn reach(&self) -> f32 {
        self.reach
    }

    /// How many boxes it has.
    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// A number that changes as a box is added or removed, for what's made from the boxes to
    /// know it's out of date.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The box `id`, unless there's none or it's been removed.
    pub fn area(&self, id: usize) -> Option<Area> {
        self.present
            .get(id)
            .copied()
            .unwrap_or(false)
            .then(|| self.areas[id])
    }

    /// Every box, and its id, by id.
    pub fn iter(&self) -> impl Iterator<Item = (usize, Area)> + '_ {
        self.areas
            .iter()
            .enumerate()
            .filter(|&(id, _)| self.present[id])
            .map(|(id, &area)| (id, area))
    }

    /// Adds a box. Returns its id: how many were added before it.
    pub fn add(&mut self, area: Area) -> usize {
        let id = self.areas.len();
        self.areas.push(area);
        self.present.push(true);
        self.live += 1;
        self.reindex();
        id
    }

    /// Takes the box `id` out, if it's there. Its id isn't given to another.
    pub fn remove(&mut self, id: usize) -> bool {
        if !self.present.get(id).copied().unwrap_or(false) {
            return false;
        }
        self.present[id] = false;
        self.live -= 1;
        self.reindex();
        true
    }

    /// The first box that a body reaching `radius` from its middle enters, going from `a` to `b`,
    /// and how far along, as a share of the way. Boxes either end is inside are passed over, as
    /// is a way along a box's edge. Ties go to the lower id.
    pub fn first(&self, a: Point, b: Point, radius: f32) -> Option<(usize, f32)> {
        let consider = |best: &mut Option<(usize, f32)>, id: usize, area: Area| {
            let grown = area.grown(radius);
            if grown.surrounds(a) || grown.surrounds(b) {
                return;
            }
            if let Some(at) = area.entry(a, b, radius)
                && best.is_none_or(|(least_id, least)| at < least || (at == least && id < least_id))
            {
                *best = Some((id, at));
            }
        };
        let mut best = None;
        if !self.walks(a, b, radius) {
            for (id, area) in self.iter() {
                consider(&mut best, id, area);
            }
            return best;
        }
        // Boxes that don't meet the way's own box can't be entered on it.
        let span = Area::from_corners(a, b).grown(radius + self.slack);
        let length = (b - a).length();
        // Past this share of the way beyond a cell, nothing first entered later can be nearer.
        let margin = if length > 0.0 {
            self.slack / length
        } else {
            0.0
        };
        // The boxes asked lately, which the next cells along mostly list again.
        let mut asked = [u32::MAX; 8];
        let mut latest = 0;
        self.walk(a, b, |cell, _, exit| {
            for &id in self.cell(cell) {
                if asked.contains(&id) {
                    continue;
                }
                asked[latest % asked.len()] = id;
                latest += 1;
                let area = self.areas[id as usize];
                if area.overlaps(span) {
                    consider(&mut best, id as usize, area);
                }
            }
            !best.is_some_and(|(_, at)| at < exit - margin)
        });
        best
    }

    /// Calls `visit` with each box that a body reaching `radius` from its middle enters, going
    /// from `a` to `b`, and how far along, as a share of the way: 0 for a box `a` is inside. Each
    /// box once, in an order of the index's own, which is the same for the same boxes and cells.
    pub fn crossed(&self, a: Point, b: Point, radius: f32, mut visit: impl FnMut(usize, f32)) {
        if !self.walks(a, b, radius) {
            for (id, area) in self.iter() {
                if let Some(at) = area.entry(a, b, radius) {
                    visit(id, at);
                }
            }
            return;
        }
        let span = Area::from_corners(a, b).grown(radius + self.slack);
        self.walk(a, b, |cell, enter, exit| {
            for &id in self.cell(cell) {
                let area = self.areas[id as usize];
                // Each box in the one cell whose stretch of the way it's entered in.
                if area.overlaps(span)
                    && let Some(at) = area.entry(a, b, radius)
                    && at >= enter
                    && at < exit
                {
                    visit(id as usize, at);
                }
            }
            true
        });
    }

    /// Calls `visit` with each box within `reach` of `point`, and how far it is, by id.
    pub fn near(&self, point: Point, reach: f32, mut visit: impl FnMut(usize, f32)) {
        if self.answers(point, reach) {
            for &id in self.cell_at(point) {
                let distance = self.areas[id as usize].distance(point);
                if distance <= reach {
                    visit(id as usize, distance);
                }
            }
        } else {
            for (id, area) in self.iter() {
                let distance = area.distance(point);
                if distance <= reach {
                    visit(id, distance);
                }
            }
        }
    }

    /// The first box, by id, that `point` is inside, grown by `margin`: where a body reaching
    /// `margin` from its middle can't stand.
    pub fn inside(&self, point: Point, margin: f32) -> Option<usize> {
        let inside = |area: Area| area.grown(margin).surrounds(point);
        if self.answers(point, margin) {
            self.cell_at(point)
                .iter()
                .map(|&id| id as usize)
                .find(|&id| inside(self.areas[id]))
        } else {
            self.iter()
                .find(|&(_, area)| inside(area))
                .map(|(id, _)| id)
        }
    }

    /// Moves a body at `point`, reaching `radius` from its middle, out of each box it's inside,
    /// by id, across the side of each it's nearest and onto its edge. Calls `out` with the id of
    /// each box it's moved out of and the side's outward direction.
    pub fn push_out(&self, point: &mut Point, radius: f32, mut out: impl FnMut(usize, Point)) {
        // The boxes from this id on are still to be asked.
        let mut after = 0;
        loop {
            if !self.answers(*point, radius) {
                for id in after..self.areas.len() {
                    if self.present[id]
                        && let Some(side) = self.areas[id].push_out(point, radius)
                    {
                        out(id, side);
                    }
                }
                return;
            }
            // Those of the point's cell, until one moves it, and then those of its new cell.
            let mut moved = false;
            for &id in self.cell_at(*point) {
                let id = id as usize;
                if id < after {
                    continue;
                }
                if let Some(side) = self.areas[id].push_out(point, radius) {
                    out(id, side);
                    after = id + 1;
                    moved = true;
                    break;
                }
            }
            if !moved {
                return;
            }
        }
    }

    /// Whether a question about a body reaching `reach` at `point` can be answered from the
    /// cells.
    fn answers(&self, point: Point, reach: f32) -> bool {
        reach <= self.reach && self.bounds.contains(point)
    }

    /// Whether a question about the way from `a` to `b` is answered by walking the cells.
    fn walks(&self, a: Point, b: Point, radius: f32) -> bool {
        self.live > FEW && self.answers(a, radius) && self.answers(b, radius)
    }

    fn column(&self, x: f32) -> usize {
        // A negative float truncates to 0, so a point before the bounds is in the first cell.
        (((x - self.bounds.min.x) / self.cell) as usize).min(self.columns - 1)
    }

    fn row(&self, y: f32) -> usize {
        (((y - self.bounds.min.y) / self.cell) as usize).min(self.rows - 1)
    }

    fn cell(&self, cell: usize) -> &[u32] {
        &self.items[self.starts[cell] as usize..self.starts[cell + 1] as usize]
    }

    fn cell_at(&self, point: Point) -> &[u32] {
        self.cell(self.row(point.y) * self.columns + self.column(point.x))
    }

    /// The columns and rows of the cells within reach of `area`, and the slack: none if it's out
    /// of reach of the bounds.
    fn span(&self, area: Area) -> Option<((usize, usize), (usize, usize))> {
        let near = area.grown(self.reach + self.slack);
        if !near.overlaps(self.bounds) {
            return None;
        }
        Some((
            (self.column(near.min.x), self.column(near.max.x)),
            (self.row(near.min.y), self.row(near.max.y)),
        ))
    }

    /// Lists every box in the cells it's near, by id, as a counting sort.
    fn reindex(&mut self) {
        self.version += 1;
        self.starts.clear();
        self.starts.resize(self.columns * self.rows + 1, 0);
        let spans: Vec<_> = self
            .iter()
            .filter_map(|(id, area)| self.span(area).map(|span| (id, span)))
            .collect();
        for &(_, (columns, rows)) in &spans {
            for row in rows.0..=rows.1 {
                for column in columns.0..=columns.1 {
                    self.starts[row * self.columns + column + 1] += 1;
                }
            }
        }
        for cell in 0..self.columns * self.rows {
            self.starts[cell + 1] += self.starts[cell];
        }
        self.items.clear();
        self.items
            .resize(self.starts[self.columns * self.rows] as usize, 0);
        let mut next = self.starts.clone();
        for (id, (columns, rows)) in spans {
            for row in rows.0..=rows.1 {
                for column in columns.0..=columns.1 {
                    let cell = row * self.columns + column;
                    self.items[next[cell] as usize] = id as u32;
                    next[cell] += 1;
                }
            }
        }
    }

    /// Calls `visit` with each cell the way from `a` to `b` crosses, in order, both in the
    /// bounds, with the shares of the way at which it enters and leaves the cell: the stretches
    /// follow on from 0, and the last runs on for ever. Stops when `visit` returns false.
    fn walk(&self, a: Point, b: Point, mut visit: impl FnMut(usize, f32, f32) -> bool) {
        let way = b - a;
        let (mut column, mut row) = (self.column(a.x), self.row(a.y));
        let (last_column, last_row) = (self.column(b.x), self.row(b.y));
        // Along each axis: which way the cells go, the share of the way at which it leaves the
        // first, and how much more each cell after takes. Summed, rounding drifts by far less
        // than the slack.
        let axis = |index: usize, origin: f32, from: f32, delta: f32| {
            if delta > 0.0 {
                let across = (origin + (index + 1) as f32 * self.cell - from) / delta;
                (1, across, self.cell / delta)
            } else if delta < 0.0 {
                let across = (origin + index as f32 * self.cell - from) / delta;
                (-1, across, -self.cell / delta)
            } else {
                (0, f32::INFINITY, f32::INFINITY)
            }
        };
        let (step_x, mut across_x, each_x) = axis(column, self.bounds.min.x, a.x, way.x);
        let (step_y, mut across_y, each_y) = axis(row, self.bounds.min.y, a.y, way.y);
        let mut enter = 0.0_f32;
        loop {
            let along_x = across_x <= across_y;
            let mut exit = if along_x { across_x } else { across_y };
            let next = if along_x {
                column
                    .checked_add_signed(step_x)
                    .filter(|&next| next < self.columns)
                    .map(|next| (next, row))
            } else {
                row.checked_add_signed(step_y)
                    .filter(|&next| next < self.rows)
                    .map(|next| (column, next))
            };
            let last = (column == last_column && row == last_row) || exit >= 1.0 || next.is_none();
            if last {
                exit = f32::INFINITY;
            }
            if !visit(row * self.columns + column, enter, exit) || last {
                return;
            }
            (column, row) = next.expect("a way goes on to a cell in the bounds");
            if along_x {
                across_x += each_x;
            } else {
                across_y += each_y;
            }
            enter = exit;
        }
    }
}
