//! What the index and the ways cost, against asking every box. Run with
//! `cargo bench -p wingine-nav`.
//!
//! A field 400 by 600 metres, in cells 8 across, for bodies reaching up to 8 from their middle,
//! with 6 to 400 boxes, 2 to 12 metres by 1 to 7, scattered by a seed over 130 by 200 between
//! 1000 bodies, in rows of ten, and a point 260 metres up the field they're all headed for: a
//! crowd ordered across a ground of walls and buildings. Each case reports the median of
//! [`RUNS`] runs of what a frame of 1000 bodies would ask, or of making the ways.

use std::hint::black_box;
use std::time::Instant;

use wingine_nav::{Area, Obstacles, Point, Way, Ways};

const RUNS: usize = 31;
const BODIES: usize = 1000;
const RADIUS: f32 = 0.4;

fn main() {
    println!("1000 bodies' questions, and the ways, the median of {RUNS} runs:");
    for count in [6, 20, 47, 100, 400] {
        let obstacles = field(count);
        let bodies = bodies(&obstacles);
        let target = Point::new(7.0, -260.0);
        println!("{count} boxes:");

        report(
            "the first in the way, by the cells",
            time(|| {
                for &body in &bodies {
                    black_box(obstacles.first(body, target, RADIUS));
                }
            }),
        );
        report(
            "the first in the way, every box asked",
            time(|| {
                for &body in &bodies {
                    black_box(first_of_all(&obstacles, body, target));
                }
            }),
        );
        report(
            "pushed out, by the cells",
            time(|| {
                for &body in &bodies {
                    let mut at = body;
                    obstacles.push_out(&mut at, RADIUS, |_, _| {});
                    black_box(at);
                }
            }),
        );
        report(
            "pushed out, every box asked",
            time(|| {
                for &body in &bodies {
                    let mut at = body;
                    for (_, area) in obstacles.iter() {
                        area.push_out(&mut at, RADIUS);
                    }
                    black_box(at);
                }
            }),
        );
        report(
            "those within 8, by the cells",
            time(|| {
                for &body in &bodies {
                    let mut near = 0;
                    obstacles.near(body, 8.0, |_, _| near += 1);
                    black_box(near);
                }
            }),
        );
        report(
            "those within 8, every box asked",
            time(|| {
                for &body in &bodies {
                    let near = obstacles
                        .iter()
                        .filter(|(_, area)| area.distance(body) <= 8.0)
                        .count();
                    black_box(near);
                }
            }),
        );

        let mut ways = Ways::new(&obstacles, RADIUS, 0.6);
        report(
            &format!("making the ways, {} corners", ways.len()),
            time(|| {
                black_box(Ways::new(&obstacles, RADIUS, 0.6));
            }),
        );
        let mut way = Way::new();
        let mut found = 0;
        report(
            "1000 ways found, one from each body",
            time(|| {
                found = 0;
                for &body in &bodies {
                    found += usize::from(ways.find(&obstacles, body, target, &mut way));
                }
            }),
        );
        println!("  ({found} of {BODIES} found)");
        report(
            "1000 bodies walking a way: where each heads",
            time(|| {
                for &body in &bodies {
                    black_box(way.clone().toward(&obstacles, body, RADIUS));
                }
            }),
        );
    }
}

/// `count` boxes, scattered as `main` says.
fn field(count: usize) -> Obstacles {
    let bounds = Area::from_corners(Point::new(-200.0, -400.0), Point::new(200.0, 200.0));
    let mut obstacles = Obstacles::new(bounds, 8.0, 8.0);
    let mut seed = 7_u32;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed % 10_000) as f32 / 10_000.0
    };
    for _ in 0..count {
        let at = Point::new(-60.0 + next() * 130.0, -230.0 + next() * 200.0);
        let size = Point::new(2.0 + next() * 10.0, 1.0 + next() * 6.0);
        obstacles.add(Area::from_corners(at, at + size));
    }
    obstacles
}

/// Bodies in rows of ten, a metre and a half apart, from the origin down the field, out of any
/// box.
fn bodies(obstacles: &Obstacles) -> Vec<Point> {
    (0..BODIES)
        .map(|index| {
            let mut at = Point::new((index % 10) as f32 * 1.5, (index / 10) as f32 * 1.5);
            obstacles.push_out(&mut at, RADIUS, |_, _| {});
            at
        })
        .collect()
}

fn first_of_all(obstacles: &Obstacles, a: Point, b: Point) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32)> = None;
    for (id, area) in obstacles.iter() {
        let grown = area.grown(RADIUS);
        if grown.surrounds(a) || grown.surrounds(b) {
            continue;
        }
        if let Some(at) = area.entry(a, b, RADIUS)
            && best.is_none_or(|(_, least)| at < least)
        {
            best = Some((id, at));
        }
    }
    best
}

/// The median of [`RUNS`] runs of `run`, in milliseconds.
fn time(mut run: impl FnMut()) -> f64 {
    let mut times: Vec<f64> = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            run();
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    times.sort_by(f64::total_cmp);
    times[RUNS / 2]
}

fn report(case: &str, milliseconds: f64) {
    println!("  {case}: {milliseconds:.3} ms");
}
