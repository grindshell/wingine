//! The index against asking every box, over fields a seed scatters, and the ways round them.

use wingine_nav::{Area, Obstacles, Point, Way, Ways};

/// Xorshift, so a field and its questions are the same on every run.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 % 100_000) as f32 / 100_000.0
    }

    fn between(&mut self, low: f32, high: f32) -> f32 {
        low + self.next() * (high - low)
    }

    fn point(&mut self, reach: f32) -> Point {
        Point::new(self.between(-reach, reach), self.between(-reach, reach))
    }
}

fn bounds() -> Area {
    Area::from_corners(Point::new(-100.0, -100.0), Point::new(100.0, 100.0))
}

/// `count` boxes scattered over and a little past `bounds()`, some touching or overlapping, with
/// every seventh taken out again.
fn field(seed: u32, count: usize, cell: f32) -> Obstacles {
    let mut rng = Rng(seed);
    let mut obstacles = Obstacles::new(bounds(), cell, 8.0);
    for index in 0..count {
        let at = rng.point(110.0);
        let size = Point::new(rng.between(0.5, 20.0), rng.between(0.5, 20.0));
        let id = obstacles.add(Area::from_corners(at, at + size));
        // Another flush against its side now and then.
        if index % 5 == 0 {
            let side = Point::new(at.x + size.x, at.y);
            obstacles.add(Area::from_corners(side, side + Point::new(3.0, size.y)));
        }
        if index % 7 == 3 {
            assert!(obstacles.remove(id));
        }
    }
    obstacles
}

/// Questions: points scattered over and past the bounds, and some on the boxes' corners and
/// edges, where rounding is closest.
fn points(obstacles: &Obstacles, seed: u32, count: usize) -> Vec<Point> {
    let mut rng = Rng(seed);
    let mut points: Vec<Point> = (0..count).map(|_| rng.point(105.0)).collect();
    for (_, area) in obstacles.iter().take(count / 4) {
        points.push(area.min);
        points.push(Point::new(area.max.x, rng.between(area.min.y, area.max.y)));
        points.push(area.corners(0.4)[2]);
    }
    points
}

const RADII: [f32; 6] = [0.0, 0.4, 1.0, 8.0, 9.5, 30.0];

fn first_of_all(obstacles: &Obstacles, a: Point, b: Point, radius: f32) -> Option<(usize, u32)> {
    let mut best: Option<(usize, f32)> = None;
    for (id, area) in obstacles.iter() {
        let grown = area.grown(radius);
        if grown.surrounds(a) || grown.surrounds(b) {
            continue;
        }
        if let Some(at) = area.entry(a, b, radius)
            && best.is_none_or(|(_, least)| at < least)
        {
            best = Some((id, at));
        }
    }
    best.map(|(id, at)| (id, at.to_bits()))
}

/// Each field, at three sizes of cell, and every question of `ask`.
fn every_field(mut ask: impl FnMut(&Obstacles, &[Point])) {
    for seed in [1, 2, 3] {
        for count in [1, 12, 60, 200] {
            for cell in [3.0, 8.0, 25.0] {
                let obstacles = field(seed, count, cell);
                let points = points(&obstacles, seed * 31 + count as u32, 120);
                ask(&obstacles, &points);
            }
        }
    }
}

#[test]
fn the_first_box_on_a_way_is_the_one_every_box_asked_gives() {
    every_field(|obstacles, points| {
        for pair in points.windows(2) {
            // Each way, along each axis from its start too, as walls' sides run.
            let (a, b) = (pair[0], pair[1]);
            for (a, b) in [(a, b), (a, Point::new(b.x, a.y)), (a, Point::new(a.x, b.y))] {
                for radius in RADII {
                    let got = obstacles
                        .first(a, b, radius)
                        .map(|(id, at)| (id, at.to_bits()));
                    assert_eq!(got, first_of_all(obstacles, a, b, radius), "{a:?} to {b:?}");
                }
            }
        }
    });
}

#[test]
fn the_boxes_a_way_crosses_are_those_every_box_asked_gives() {
    every_field(|obstacles, points| {
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            for radius in RADII {
                let mut got = Vec::new();
                obstacles.crossed(a, b, radius, |id, at| got.push((id, at.to_bits())));
                got.sort();
                let all: Vec<(usize, u32)> = obstacles
                    .iter()
                    .filter_map(|(id, area)| area.entry(a, b, radius).map(|at| (id, at.to_bits())))
                    .collect();
                assert_eq!(got, all, "{a:?} to {b:?}, {radius}");
            }
        }
    });
}

#[test]
fn the_boxes_near_a_place_are_those_every_box_asked_gives() {
    every_field(|obstacles, points| {
        for &point in points {
            for reach in RADII {
                let mut got = Vec::new();
                obstacles.near(point, reach, |id, distance| {
                    got.push((id, distance.to_bits()))
                });
                let all: Vec<(usize, u32)> = obstacles
                    .iter()
                    .map(|(id, area)| (id, area.distance(point)))
                    .filter(|&(_, distance)| distance <= reach)
                    .map(|(id, distance)| (id, distance.to_bits()))
                    .collect();
                assert_eq!(got, all, "{point:?}, {reach}");

                let inside = obstacles
                    .iter()
                    .find(|(_, area)| area.grown(reach).surrounds(point))
                    .map(|(id, _)| id);
                assert_eq!(obstacles.inside(point, reach), inside, "{point:?}, {reach}");
            }
        }
    });
}

#[test]
fn a_body_is_pushed_out_as_every_box_asked_in_turn_would() {
    every_field(|obstacles, points| {
        for &point in points {
            for radius in RADII {
                let mut got = point;
                let mut sides = Vec::new();
                obstacles.push_out(&mut got, radius, |id, side| sides.push((id, side)));
                let mut all = point;
                let mut all_sides = Vec::new();
                for (id, area) in obstacles.iter() {
                    if let Some(side) = area.push_out(&mut all, radius) {
                        all_sides.push((id, side));
                    }
                }
                assert_eq!(
                    (got.x.to_bits(), got.y.to_bits()),
                    (all.x.to_bits(), all.y.to_bits())
                );
                assert_eq!(sides, all_sides, "{point:?}, {radius}");
            }
        }
    });
}

#[test]
fn a_box_taken_out_is_asked_about_no_more() {
    let mut obstacles = Obstacles::new(bounds(), 8.0, 8.0);
    let wall = Area::from_corners(Point::new(-10.0, -1.0), Point::new(10.0, 1.0));
    let (a, b) = (Point::new(0.0, -20.0), Point::new(0.0, 20.0));
    let id = obstacles.add(wall);
    assert_eq!(obstacles.first(a, b, 0.4).map(|(id, _)| id), Some(id));
    let version = obstacles.version();
    assert!(obstacles.remove(id));
    assert!(!obstacles.remove(id), "it's gone already");
    assert_ne!(obstacles.version(), version);
    assert_eq!(obstacles.first(a, b, 0.4), None);
    assert_eq!(obstacles.inside(Point::ZERO, 0.0), None);
    assert!(obstacles.is_empty());
    // Ids aren't given again.
    assert_eq!(obstacles.add(wall), id + 1);
}

/// How long a way is from `from`.
fn length(from: Point, way: &Way) -> f32 {
    let mut at = from;
    let mut length = 0.0;
    for &point in way.points() {
        length += (point - at).length();
        at = point;
    }
    length
}

/// The shortest way by every corner that sees another, turning or not: what the ways' joins
/// leave out must never make one longer.
fn shortest_of_all(obstacles: &Obstacles, radius: f32, out: f32, from: Point, to: Point) -> f32 {
    if obstacles.first(from, to, radius).is_none() {
        return (to - from).length();
    }
    let mut points = vec![from, to];
    for (_, area) in obstacles.iter() {
        for corner in area.corners(out) {
            if obstacles.bounds().contains(corner) && obstacles.inside(corner, radius).is_none() {
                points.push(corner);
            }
        }
    }
    let mut cost = vec![f32::INFINITY; points.len()];
    let mut done = vec![false; points.len()];
    cost[0] = 0.0;
    loop {
        let Some(next) = (0..points.len())
            .filter(|&i| !done[i] && cost[i].is_finite())
            .min_by(|&i, &j| cost[i].total_cmp(&cost[j]))
        else {
            return f32::INFINITY;
        };
        if next == 1 {
            return cost[1];
        }
        done[next] = true;
        for other in 0..points.len() {
            if !done[other]
                && obstacles
                    .first(points[next], points[other], radius)
                    .is_none()
            {
                cost[other] = cost[other].min(cost[next] + (points[other] - points[next]).length());
            }
        }
    }
}

#[test]
fn a_way_found_is_the_shortest_and_walks_into_nothing() {
    for seed in [4, 5, 6, 9, 10, 11] {
        for count in [6, 20, 47, 100] {
            let obstacles = field(seed, count, 8.0);
            let mut ways = Ways::new(&obstacles, 0.4, 0.6);
            let mut way = Way::new();
            let mut rng = Rng(seed * 7 + count as u32);
            for _ in 0..20 {
                let (from, to) = (rng.point(95.0), rng.point(95.0));
                if obstacles.inside(from, 0.4).is_some() {
                    continue;
                }
                let found = ways.find(&obstacles, from, to, &mut way);
                let shortest = shortest_of_all(&obstacles, 0.4, 1.0, from, to);
                if !found {
                    assert!(
                        shortest.is_infinite() || obstacles.inside(to, 0.4).is_some(),
                        "no way from {from:?} to {to:?}, though there's one {shortest} long"
                    );
                    assert!(!ways.reachable(&obstacles, from, to));
                    continue;
                }
                assert!(ways.reachable(&obstacles, from, to));
                let got = length(from, &way);
                assert!(
                    (got - shortest).abs() <= 1e-3 * shortest.max(1.0),
                    "{got} long from {from:?} to {to:?}, where {shortest} is the shortest"
                );
                let mut at = from;
                for &point in way.points() {
                    assert_eq!(obstacles.first(at, point, 0.4), None, "{at:?} to {point:?}");
                    at = point;
                }
                assert_eq!(way.end(), Some(to));
            }
        }
    }
}

/// Walls of a pen round the origin, 20 across and a metre thick, its side toward `+x` left out
/// if `open`.
fn pen(open: bool) -> Obstacles {
    let mut obstacles = Obstacles::new(bounds(), 8.0, 8.0);
    let wall = |a: (f32, f32), b: (f32, f32)| {
        Area::from_corners(Point::new(a.0, a.1), Point::new(b.0, b.1))
    };
    obstacles.add(wall((-10.0, -10.0), (10.0, -9.0)));
    obstacles.add(wall((-10.0, 9.0), (10.0, 10.0)));
    obstacles.add(wall((-10.0, -10.0), (-9.0, 10.0)));
    if !open {
        obstacles.add(wall((9.0, -10.0), (10.0, 10.0)));
    }
    obstacles
}

#[test]
fn a_way_out_of_a_pocket_goes_back_the_way_it_opens() {
    let obstacles = pen(true);
    let mut ways = Ways::new(&obstacles, 0.4, 0.6);
    let mut way = Way::new();
    // From inside, to behind its back wall: out of the open side and round.
    let (from, to) = (Point::new(-5.0, 0.0), Point::new(-20.0, 0.0));
    assert!(ways.find(&obstacles, from, to, &mut way));
    assert!(
        way.points().iter().any(|point| point.x > 10.0),
        "{:?}",
        way.points()
    );
    // Walked, by a body that heads where the way says, it never walks into a wall.
    let mut at = from;
    for _ in 0..2000 {
        let Some(toward) = way.toward(&obstacles, at, 0.4) else {
            panic!("the way ran out");
        };
        let step = toward - at;
        let length = step.length();
        if length < 0.05 {
            break;
        }
        at = at + step * (0.05 / length);
        assert_eq!(
            obstacles.inside(at, 0.4),
            None,
            "walked into a wall at {at:?}"
        );
    }
    assert!((at - to).length() < 0.05, "it stopped at {at:?}");
}

#[test]
fn there_is_no_way_into_a_closed_pen_until_a_wall_is_taken_out() {
    let mut obstacles = pen(false);
    let mut ways = Ways::new(&obstacles, 0.4, 0.6);
    let mut way = Way::new();
    let (inside, outside) = (Point::new(0.0, 0.0), Point::new(30.0, 0.0));
    assert!(!ways.reachable(&obstacles, outside, inside));
    assert!(!ways.find(&obstacles, outside, inside, &mut way));
    assert!(way.is_empty());
    assert!(!ways.find(&obstacles, inside, outside, &mut way));
    // Inside a wall is nowhere to go.
    assert!(!ways.find(&obstacles, outside, Point::new(0.0, 9.5), &mut way));
    // With the side toward `+x` taken out, the ways catch up.
    obstacles.remove(3);
    assert!(ways.find(&obstacles, outside, inside, &mut way));
    assert_eq!(way.end(), Some(inside));
}

#[test]
fn a_way_in_the_clear_goes_straight_there() {
    let obstacles = pen(false);
    let mut ways = Ways::new(&obstacles, 0.4, 0.6);
    let mut way = Way::new();
    let (from, to) = (Point::new(30.0, -30.0), Point::new(30.0, 30.0));
    assert!(ways.find(&obstacles, from, to, &mut way));
    assert_eq!(way.points(), [to]);
    assert_eq!(way.toward(&obstacles, from, 0.4), Some(to));
}

/// FNV-1a over the bits of every way found over seeded fields: the same on every platform, or
/// the ways aren't, and games that replay would drift.
#[test]
fn the_same_fields_give_the_same_ways_on_every_platform() {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut fold = |bits: u32| {
        for byte in bits.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for seed in [7, 8] {
        let obstacles = field(seed, 47, 8.0);
        let mut ways = Ways::new(&obstacles, 0.4, 0.6);
        fold(ways.len() as u32);
        let mut way = Way::new();
        let mut rng = Rng(seed);
        for _ in 0..60 {
            let (from, to) = (rng.point(95.0), rng.point(95.0));
            fold(u32::from(ways.find(&obstacles, from, to, &mut way)));
            for point in way.points() {
                fold(point.x.to_bits());
                fold(point.y.to_bits());
            }
        }
    }
    assert_eq!(hash, 3_183_234_137_555_634_245);
}
