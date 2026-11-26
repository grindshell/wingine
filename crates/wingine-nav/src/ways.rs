//! The ways round an index's boxes: the boxes' corners, a little way out, and which of them see
//! each other, searched for the shortest way between two points.
//!
//! Round boxes, no way is shorter than one from corner to corner, so the corners are all a way
//! needs (a visibility graph). Every two corners that see each other are joined: leaving out the
//! joins a way could never turn at would make fewer, but the ways found among the boxes a seed
//! scatters were then longer, now and then, than the shortest. Corners joined, however far round,
//! are kept in groups, so a way that doesn't exist can be known without searching for it
//! ([`Ways::reachable`]).
//!
//! A search runs A* from every corner, as if the start saw it, with the straight line to the end
//! as its guess, taking the lower-numbered corner of two as near. It asks whether the start sees a
//! corner, and whether a corner sees the end, only as it comes to the corner, so it asks about
//! few of them: most are further round than the way it finds.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::area::Point;
use crate::obstacles::Obstacles;

/// No corner, in a list of them.
const NONE: u32 = u32::MAX;

/// The ways round an index's boxes for bodies reaching `radius` from their middle, passing
/// `clearance` further off each corner.
#[derive(Clone, Debug)]
pub struct Ways {
    radius: f32,
    clearance: f32,
    /// The index's version the corners are of.
    version: Option<u64>,
    /// Each box's corners, out by the radius and the clearance, by the box's id, but for those out
    /// of the bounds or where a body can't stand.
    corners: Vec<Point>,
    /// Each corner's joins, as runs of `joined` and `lengths`, from `starts[corner]` up to the
    /// next corner's.
    starts: Vec<u32>,
    joined: Vec<u32>,
    lengths: Vec<f32>,
    /// Each corner's group: the lowest corner it's joined to, however far round.
    groups: Vec<u32>,
    search: Search,
}

/// What a search keeps, kept between searches so they don't allocate.
#[derive(Clone, Debug, Default)]
struct Search {
    /// How far each corner is from the start, by the best way so far that it's known to see.
    cost: Vec<f32>,
    /// The corner each was come to from, or `NONE` for one the start sees.
    came: Vec<u32>,
    closed: Vec<bool>,
    /// Which groups have a corner that sees the end, for `reachable`.
    groups: Vec<bool>,
    open: BinaryHeap<Open>,
}

/// A corner to search on from, by how long a way through it could be at least.
#[derive(Clone, Copy, Debug)]
struct Open {
    least: f32,
    corner: u32,
    /// Whether it's straight from the start, which isn't known to see it yet.
    direct: bool,
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
    /// The least first, out of a heap that gives the greatest; of two as short, the lower corner,
    /// and of one corner's two, the one straight from the start.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .least
            .total_cmp(&self.least)
            .then(other.corner.cmp(&self.corner))
            .then(self.direct.cmp(&other.direct))
    }
}

impl Ways {
    /// The ways round the boxes of `obstacles` for bodies reaching `radius` from their middle,
    /// passing `clearance` further off each corner.
    ///
    /// # Panics
    ///
    /// If `radius` or `clearance` is less than 0, or either isn't finite.
    pub fn new(obstacles: &Obstacles, radius: f32, clearance: f32) -> Self {
        assert!(
            radius >= 0.0 && radius.is_finite(),
            "a radius can't be less than 0: {radius}"
        );
        assert!(
            clearance >= 0.0 && clearance.is_finite(),
            "a clearance can't be less than 0: {clearance}"
        );
        let mut ways = Self {
            radius,
            clearance,
            version: None,
            corners: Vec::new(),
            starts: Vec::new(),
            joined: Vec::new(),
            lengths: Vec::new(),
            groups: Vec::new(),
            search: Search::default(),
        };
        ways.update(obstacles);
        ways
    }

    /// How many corners its ways turn at.
    pub fn len(&self) -> usize {
        self.corners.len()
    }

    pub fn is_empty(&self) -> bool {
        self.corners.is_empty()
    }

    /// Catches up with boxes added to `obstacles` or taken out since it was made, or last caught
    /// up, by making its corners again. `find` and `reachable` call it first.
    pub fn update(&mut self, obstacles: &Obstacles) {
        if self.version == Some(obstacles.version()) {
            return;
        }
        self.version = Some(obstacles.version());
        let bounds = obstacles.bounds();
        let out = self.radius + self.clearance;
        self.corners.clear();
        for (_, area) in obstacles.iter() {
            for at in area.corners(out) {
                if bounds.contains(at) && obstacles.inside(at, self.radius).is_none() {
                    self.corners.push(at);
                }
            }
        }

        let count = self.corners.len();
        let mut joins: Vec<Vec<(u32, f32)>> = vec![Vec::new(); count];
        for (i, &a) in self.corners.iter().enumerate() {
            for (j, &b) in self.corners.iter().enumerate().skip(i + 1) {
                if obstacles.first(a, b, self.radius).is_none() {
                    let length = (b - a).length();
                    joins[i].push((j as u32, length));
                    joins[j].push((i as u32, length));
                }
            }
        }
        self.starts.clear();
        self.joined.clear();
        self.lengths.clear();
        for list in &joins {
            self.starts.push(self.joined.len() as u32);
            for &(corner, length) in list {
                self.joined.push(corner);
                self.lengths.push(length);
            }
        }
        self.starts.push(self.joined.len() as u32);

        // Each group by its lowest corner: joined pairs merged, the higher root under the lower.
        let mut parents: Vec<u32> = (0..count as u32).collect();
        fn root(parents: &mut [u32], mut corner: u32) -> u32 {
            while parents[corner as usize] != corner {
                let up = parents[parents[corner as usize] as usize];
                parents[corner as usize] = up;
                corner = up;
            }
            corner
        }
        for (corner, list) in joins.iter().enumerate() {
            for &(other, _) in list {
                let (a, b) = (root(&mut parents, corner as u32), root(&mut parents, other));
                parents[a.max(b) as usize] = a.min(b);
            }
        }
        self.groups = (0..count as u32)
            .map(|corner| root(&mut parents, corner))
            .collect();
    }

    /// Whether a body at `from`, reaching the ways' radius, can get to `to` at all. It asks
    /// whether each end sees each corner, but doesn't search.
    pub fn reachable(&mut self, obstacles: &Obstacles, from: Point, to: Point) -> bool {
        self.update(obstacles);
        if obstacles.inside(to, self.radius).is_some() {
            return false;
        }
        if obstacles.first(from, to, self.radius).is_none() {
            return true;
        }
        let groups = &mut self.search.groups;
        groups.clear();
        groups.resize(self.corners.len(), false);
        for (index, &corner) in self.corners.iter().enumerate() {
            if obstacles.first(corner, to, self.radius).is_none() {
                groups[self.groups[index] as usize] = true;
            }
        }
        self.corners.iter().enumerate().any(|(index, &corner)| {
            groups[self.groups[index] as usize]
                && obstacles.first(from, corner, self.radius).is_none()
        })
    }

    /// Fills `way` with the shortest way for a body at `from`, reaching the ways' radius, to
    /// `to`: the corners to pass, in order, then `to`. Returns false, with `way` empty, if `to`
    /// is inside a box, grown by the radius, or can't be got to from `from`: which takes the
    /// longest to find out, as it asks about every corner the start sees.
    pub fn find(&mut self, obstacles: &Obstacles, from: Point, to: Point, way: &mut Way) -> bool {
        self.update(obstacles);
        way.clear();
        if obstacles.inside(to, self.radius).is_some() {
            return false;
        }
        if obstacles.first(from, to, self.radius).is_none() {
            way.points.push(to);
            return true;
        }

        let count = self.corners.len();
        let search = &mut self.search;
        search.cost.clear();
        search.cost.resize(count, f32::INFINITY);
        search.came.clear();
        search.came.resize(count, NONE);
        search.closed.clear();
        search.closed.resize(count, false);
        search.open.clear();
        for (index, &corner) in self.corners.iter().enumerate() {
            search.open.push(Open {
                least: (corner - from).length() + (to - corner).length(),
                corner: index as u32,
                direct: true,
            });
        }
        let mut best = (f32::INFINITY, NONE);
        while let Some(Open {
            least,
            corner,
            direct,
        }) = search.open.pop()
        {
            if least >= best.0 {
                break;
            }
            let index = corner as usize;
            if search.closed[index] {
                continue;
            }
            let at = self.corners[index];
            if direct {
                // Straight from the start, if it sees the corner and there's no better way known.
                let cost = (at - from).length();
                if cost >= search.cost[index] || obstacles.first(from, at, self.radius).is_some() {
                    continue;
                }
                search.cost[index] = cost;
                search.came[index] = NONE;
            }
            search.closed[index] = true;
            let cost = search.cost[index];
            if obstacles.first(at, to, self.radius).is_none() {
                let whole = cost + (to - at).length();
                if whole < best.0 {
                    best = (whole, corner);
                }
            }
            for join in self.starts[index]..self.starts[index + 1] {
                let next = self.joined[join as usize];
                let further = cost + self.lengths[join as usize];
                if !search.closed[next as usize] && further < search.cost[next as usize] {
                    search.cost[next as usize] = further;
                    search.came[next as usize] = corner;
                    search.open.push(Open {
                        least: further + (to - self.corners[next as usize]).length(),
                        corner: next,
                        direct: false,
                    });
                }
            }
        }
        if best.1 == NONE {
            return false;
        }
        let mut corner = best.1;
        while corner != NONE {
            way.points.push(self.corners[corner as usize]);
            corner = search.came[corner as usize];
        }
        way.points.reverse();
        way.points.push(to);
        true
    }
}

/// A way found, kept by what walks it, and reused for the next.
#[derive(Clone, Debug, Default)]
pub struct Way {
    points: Vec<Point>,
    /// The point it heads for.
    next: usize,
}

impl Way {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets it.
    pub fn clear(&mut self) {
        self.points.clear();
        self.next = 0;
    }

    /// Whether there's nowhere to go: none found, or it's been cleared.
    pub fn is_empty(&self) -> bool {
        self.next >= self.points.len()
    }

    /// The points still to go, the one it heads for first, and its end last.
    pub fn points(&self) -> &[Point] {
        &self.points[self.next.min(self.points.len())..]
    }

    /// Where it ends.
    pub fn end(&self) -> Option<Point> {
        self.points.last().copied()
    }

    /// Where a body at `at`, reaching `radius` from its middle, heads: the point it heads for,
    /// or one further on, once it's within `radius` of that point or sees the one after it. Its
    /// end once it gets there, and none if it's empty.
    pub fn toward(&mut self, obstacles: &Obstacles, at: Point, radius: f32) -> Option<Point> {
        while self.next + 1 < self.points.len() {
            let point = self.points[self.next];
            let after = self.points[self.next + 1];
            if (point - at).length() <= radius || obstacles.first(at, after, radius).is_none() {
                self.next += 1;
            } else {
                break;
            }
        }
        self.points.get(self.next).copied()
    }
}
