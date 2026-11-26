//! Obstacles on a plane, and the ways round them: for crowds of bodies that walk among walls and
//! buildings, in games written in Rust and in native libraries, which compile it in.
//!
//! - [`Obstacles`] indexes boxes with their sides along the axes, in square cells, and answers
//!   what a body asks of them: the first it walks into on a way ([`Obstacles::first`]), those a
//!   way passes through ([`Obstacles::crossed`]), those near a place ([`Obstacles::near`],
//!   [`Obstacles::inside`]), and where a body that walked into some stands once it's out
//!   ([`Obstacles::push_out`]). Each answer is the one asking every box in order of ids would
//!   give, to the bit; the cells only make it quicker.
//! - [`Ways`] finds the shortest way round them ([`Ways::find`]), as the corners to pass, which a
//!   body walks with a [`Way`], and knows without searching when there's none
//!   ([`Ways::reachable`]).
//!
//! It's 2D: a game on the ground of a 3D view gives its `x` and `z` as a [`Point`]'s `x` and
//! `y`. It uses `f32` and only `+`, `-`, `*`, `/` and `sqrt`, keeps every list in order, and
//! settles every tie by the lower id, so the same boxes and questions give the same answers on
//! every platform, as a game that replays needs. It knows nothing of the engine, so a library's
//! tests use it with no engine running.
//!
//! ```
//! use wingine_nav::{Area, Obstacles, Point, Way, Ways};
//!
//! // A field 100 metres square, in cells 8 across, for bodies reaching up to 8 from their middle.
//! let field = Area::from_corners(Point::new(-50.0, -50.0), Point::new(50.0, 50.0));
//! let mut obstacles = Obstacles::new(field, 8.0, 8.0);
//! // A wall across the way.
//! obstacles.add(Area::from_corners(Point::new(-10.0, -1.0), Point::new(10.0, 1.0)));
//!
//! let (from, to) = (Point::new(0.0, -20.0), Point::new(0.0, 20.0));
//! assert_eq!(obstacles.first(from, to, 0.4).map(|(id, _)| id), Some(0));
//!
//! // Bodies reaching 0.4, passing 0.6 further off each corner.
//! let mut ways = Ways::new(&obstacles, 0.4, 0.6);
//! let mut way = Way::new();
//! assert!(ways.find(&obstacles, from, to, &mut way));
//! // Round one end of the wall: two corners, then there.
//! assert_eq!(way.points().len(), 3);
//! assert_eq!(way.end(), Some(to));
//! ```

mod area;
mod obstacles;
mod ways;

pub use area::{Area, Point};
pub use obstacles::Obstacles;
pub use ways::{Way, Ways};
