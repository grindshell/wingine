# `wingine-nav`

A Rust crate of obstacles on a plane and the ways round them, for crowds of bodies that walk
among walls and buildings. A [native library](../native-libraries.md) compiles it in, as a game
written in Rust can: it knows nothing of the engine, so nothing of it crosses into Luau, and a
library's tests use it with no engine running. Luau can't reach it.

```toml
[dependencies]
wingine-nav = { path = "path/to/wingine/crates/wingine-nav" }
```

Everything below is in the crate's root, `wingine_nav`. For its full Rust docs, run
`cargo doc --open -p wingine-nav` in the engine's repository.

- [`Point` and `Area`](#point-and-area): places and boxes, and what one box answers
- [`Obstacles`](#obstacles): boxes indexed in cells, and what a body asks of them
- [`Ways`](#ways) and [`Way`](#way): the shortest way round them, and walking it
- [The same on every platform](#the-same-on-every-platform)
- [What it costs](#what-it-costs)

It's 2D: a game on the ground of a 3D view gives its `x` and `z` as a point's `x` and `y`.

## `Point` and `Area`

```rust
pub struct Point { pub x: f32, pub y: f32 }
pub struct Area { pub min: Point, pub max: Point }
```

A `Point` is a place or a direction, with `+`, `-`, `*` by a number and `length`. An `Area` is a
box with its sides along the axes, made with `Area::from_corners(a, b)`, whichever two opposite
corners they are. What one box answers by itself:

| Method | Returns |
|---|---|
| `contains(point)` | Whether the point is in it, or on its edge |
| `surrounds(point)` | Whether the point is inside it: its edge isn't |
| `overlaps(other)` | Whether two boxes share any point, an edge's included |
| `grown(margin)` | It, wider by `margin` on every side |
| `distance(point)` | How far the point is from it, 0 inside |
| `entry(a, b, margin)` | Where the way from `a` to `b` first enters it, grown by `margin`, as a share of the way, 0 if `a` is inside: `None` if it never passes through its inside. A way along its edge doesn't |
| `push_out(&mut point, margin)` | Moves a point inside it, grown by `margin`, across the side it's nearest onto the edge, and returns the side's outward direction |
| `corners(margin)` | Its four corners, grown by `margin`: the least, along `x`, the greatest, along `y` |

## `Obstacles`

```rust
let field = Area::from_corners(Point::new(-200.0, -300.0), Point::new(200.0, 300.0));
let mut obstacles = Obstacles::new(field, 8.0, 8.0);
let wall = obstacles.add(Area::from_corners(Point::new(-10.0, -1.0), Point::new(10.0, 1.0)));
```

`Obstacles::new(bounds, cell, reach)` is an empty index over `bounds`, in square cells `cell`
across, for bodies that reach no further than `reach` from their middle. `add` returns a box's
id, how many were added before it; `remove(id)` takes it out, and its id isn't given again.
`area(id)`, `iter()` (every box and its id, by id), `len`, `bounds` and `reach` read it back, and
`version()` changes with every box added or removed.

What a body asks of the boxes:

| Method | Answers |
|---|---|
| `first(a, b, radius)` | The first box a body reaching `radius` enters going from `a` to `b`, and how far along, as a share of the way. Boxes either end is inside are passed over. Ties go to the lower id |
| `crossed(a, b, radius, visit)` | Calls `visit(id, share)` with every box the way enters, each once, in an order of the index's own |
| `near(point, reach, visit)` | Calls `visit(id, distance)` with every box within `reach` of the point, by id |
| `inside(point, margin)` | The first box, by id, the point is inside, grown by `margin`: where a body can't stand |
| `push_out(&mut point, radius, out)` | Moves a body out of each box it's inside, by id, as `Area::push_out` does, calling `out(id, side)` for each |

**Every answer is the one asking every box in order of ids would give, to the bit**, whatever
the size of the cells: they only pass over boxes that couldn't change it. So a game can move a
loop over its walls onto the index and play the same. A question about a place, within the
index's reach, asks only the boxes near the place's cell; a question about a way asks those of
the cells along it, near enough the way to be entered, and stops once nothing further on could
come first. A question asked outside the bounds, or further than the reach, asks every box, and
so does a question about a way among 24 boxes or fewer, which it takes less time to ask than to
walk the cells.

Choose a cell about as wide as the reach, and the reach as far as the furthest place question
asks: a unit's cover within 8 metres, say. The cells list each box within the reach of them, so a
long reach in small cells lists each box many times.

## `Ways`

```rust
// Bodies reaching 0.4 from their middle, passing 0.6 further off each corner.
let mut ways = Ways::new(&obstacles, 0.4, 0.6);
let mut way = Way::new();
if ways.find(&obstacles, from, to, &mut way) {
    // way.points(): the corners to pass, then `to`.
}
```

`Ways::new(obstacles, radius, clearance)` is the ways round an index's boxes for bodies reaching
`radius`, through each box's corners, grown by the radius and the clearance, but for those out of
the bounds or where a body can't stand. Every two corners that see each other are joined (a
visibility graph), and round boxes no way is shorter than one along those joins.

- `find(obstacles, from, to, &mut way)` fills `way` with the shortest way, and returns false if
  `to` is inside a box or can't be got to. A way in the clear is just `to`. It searches by A*,
  asking whether the start sees a corner and whether a corner sees the end only as it comes to
  each, so it asks about few; a way that doesn't exist takes it longest to find out.
- `reachable(obstacles, from, to)` says whether there's a way at all, without searching: corners
  joined, however far round, are kept in groups, and it asks only which groups each end sees.
- `update(obstacles)` makes the corners again once boxes have been added or removed, which
  `find` and `reachable` call first: so a box removed in the middle of a game opens its ways at
  once, at the cost of making them all again.

## `Way`

A way found, kept by whatever walks it and handed to `find` again for the next. `points()` is
what's left of it, `end()` where it ends, `is_empty()` whether there's nowhere to go, and
`clear()` forgets it.

`way.toward(obstacles, at, radius)` is where a body at `at` heads: the point it heads for, or one
further on, once the body is within `radius` of that point or sees the one after it. So a body
pushed aside by the crowd still takes the straightest line it can, asking one question a frame.

## The same on every platform

The crate uses `f32` and only `+`, `-`, `*`, `/` and `sqrt`, keeps every list in order, and
settles every tie by the lower id or corner, so the same boxes and questions give the same answers
on every platform, as a game that replays needs. A test pins the ways found over seeded fields to
one number, the same on Windows and Linux.

## What it costs

`cargo bench -p wingine-nav` times a field 400 by 600 metres in cells of 8, with boxes 2 to 12
metres by 1 to 7 scattered over 130 by 200 between 1000 bodies and a point 260 metres up the field
they're all headed for. Medians on a Ryzen 7 5800X, for what 1000 bodies ask:

| Boxes | `first`, by the cells | Every box asked | `push_out`, by the cells | Every box asked | `near` 8, by the cells | Every box asked |
|---:|---:|---:|---:|---:|---:|---:|
| 20 | 0.051 ms | 0.098 ms | 0.007 ms | 0.014 ms | 0.006 ms | 0.033 ms |
| 47 | 0.100 ms | 0.227 ms | 0.007 ms | 0.034 ms | 0.005 ms | 0.078 ms |
| 100 | 0.075 ms | 0.494 ms | 0.007 ms | 0.068 ms | 0.006 ms | 0.163 ms |
| 400 | 0.098 ms | 2.074 ms | 0.007 ms | 0.260 ms | 0.005 ms | 0.643 ms |

And the ways, made, and found from each of the 1000 bodies:

| Boxes | Corners | Making them | 1000 ways found |
|---:|---:|---:|---:|
| 20 | 80 | 0.47 ms | 1.7 ms |
| 47 | 182 | 2.0 ms | 5.9 ms |
| 100 | 359 | 7.4 ms | 31.7 ms |
| 400 | 928 | 80 ms | 560 ms |

Making the ways asks of every two corners whether they see each other, so it grows with the
square of the corners, and searches grow with their joins: past a hundred boxes, a game finds a
way for a group, not for each of its bodies.
