//! Grid-based movement: characters that walk from tile to tile, as in a top-down RPG.

/// One of the four ways to move on a grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    pub const ALL: [Self; 4] = [Self::Up, Self::Down, Self::Left, Self::Right];

    /// The change in `[column, row]` of moving one tile this way. Rows grow downward.
    pub const fn offset(self) -> [i32; 2] {
        match self {
            Self::Up => [0, -1],
            Self::Down => [0, 1],
            Self::Left => [-1, 0],
            Self::Right => [1, 0],
        }
    }

    pub const fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    /// The tile next to `tile` this way.
    pub const fn neighbor(self, [column, row]: [i32; 2]) -> [i32; 2] {
        let [dx, dy] = self.offset();
        [column + dx, row + dy]
    }
}

/// Something that walks a grid one tile at a time, such as the player or an NPC. It stands on a
/// tile, steps to a neighboring one over a fixed time, and faces the way it last tried to go.
///
/// Each frame, call [`update`](Self::update) to advance the step under way, then, if it isn't
/// [moving](Self::is_moving), [`step`](Self::step) to start the next one. A step started in the
/// same frame the last one finished picks up the time left over, so walking several tiles keeps
/// an even pace.
#[derive(Clone, Debug)]
pub struct GridMover {
    tile: [i32; 2],
    step: Option<Step>,
    facing: Direction,
    step_time: f32,
    /// Seconds past the end of a step that finished in the latest update, to give a step started
    /// before the next update.
    overshoot: f32,
}

#[derive(Clone, Copy, Debug)]
struct Step {
    from: [i32; 2],
    /// From 0 at `from` to 1 at the destination.
    progress: f32,
}

impl GridMover {
    /// Standing on `tile` and facing down, taking `step_time` seconds to walk one tile.
    ///
    /// # Panics
    ///
    /// If `step_time` isn't a positive number of seconds.
    pub fn new(tile: [i32; 2], step_time: f32) -> Self {
        Self::try_new(tile, step_time).unwrap_or_else(|err| panic!("{err}"))
    }

    /// [`GridMover::new`], or what's wrong with `step_time`.
    pub(crate) fn try_new(tile: [i32; 2], step_time: f32) -> Result<Self, String> {
        Ok(Self {
            tile,
            step: None,
            facing: Direction::Down,
            step_time: check_step_time(step_time)?,
            overshoot: 0.0,
        })
    }

    /// The tile it stands on, or while stepping, the tile it's stepping onto. That tile counts as
    /// taken from the moment the step starts, so two movers can't step onto the same one.
    pub fn tile(&self) -> [i32; 2] {
        self.tile
    }

    /// Where it is, in tiles: partway between two tiles during a step. Convert it to world
    /// coordinates with [`Tilemap::tile_to_world`](crate::tilemap::Tilemap::tile_to_world).
    pub fn position(&self) -> [f32; 2] {
        let to = self.tile.map(|v| v as f32);
        match self.step {
            Some(Step { from, progress }) => {
                let from = from.map(|v| v as f32);
                let progress = progress.min(1.0);
                [0, 1].map(|i| from[i] + (to[i] - from[i]) * progress)
            }
            None => to,
        }
    }

    pub fn facing(&self) -> Direction {
        self.facing
    }

    /// Turns to face `direction` without moving, as when an NPC turns to talk.
    pub fn face(&mut self, direction: Direction) {
        self.facing = direction;
    }

    /// Whether a step is under way.
    pub fn is_moving(&self) -> bool {
        self.step.is_some()
    }

    /// Seconds taken to walk one tile.
    pub fn step_time(&self) -> f32 {
        self.step_time
    }

    /// Changes how long walking one tile takes, as when starting to run. A step under way keeps
    /// the share of it already walked.
    ///
    /// # Panics
    ///
    /// If `seconds` isn't a positive number of seconds.
    pub fn set_step_time(&mut self, seconds: f32) {
        self.try_set_step_time(seconds)
            .unwrap_or_else(|err| panic!("{err}"));
    }

    /// [`GridMover::set_step_time`], or what's wrong with `seconds`.
    pub(crate) fn try_set_step_time(&mut self, seconds: f32) -> Result<(), String> {
        self.step_time = check_step_time(seconds)?;
        Ok(())
    }

    /// Moves straight to `tile`, ending any step under way, as when entering another map.
    pub fn place(&mut self, tile: [i32; 2]) {
        self.tile = tile;
        self.step = None;
        self.overshoot = 0.0;
    }

    /// Advances the step under way by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        self.overshoot = 0.0;
        let Some(step) = &mut self.step else {
            return;
        };
        step.progress += dt / self.step_time;
        if step.progress >= 1.0 {
            self.overshoot = (step.progress - 1.0) * self.step_time;
            self.step = None;
        }
    }

    /// Turns toward `direction` and starts stepping onto the next tile that way, unless
    /// `blocked` says that tile can't be entered, such as with
    /// [`Tilemap::is_solid`](crate::tilemap::Tilemap::is_solid). Returns whether a step started.
    ///
    /// Does nothing while a step is under way.
    pub fn step(&mut self, direction: Direction, blocked: impl FnOnce([i32; 2]) -> bool) -> bool {
        if self.is_moving() {
            return false;
        }
        self.facing = direction;
        let next = direction.neighbor(self.tile);
        if blocked(next) {
            return false;
        }
        self.step = Some(Step {
            from: self.tile,
            progress: self.overshoot / self.step_time,
        });
        self.overshoot = 0.0;
        self.tile = next;
        true
    }
}

/// `seconds`, if it's a step time a [`GridMover`] accepts.
fn check_step_time(seconds: f32) -> Result<f32, String> {
    if !(seconds.is_finite() && seconds > 0.0) {
        return Err(format!(
            "step time must be a positive number of seconds, not {seconds}"
        ));
    }
    Ok(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(_: [i32; 2]) -> bool {
        false
    }

    #[test]
    fn neighbors_follow_the_offsets() {
        assert_eq!(Direction::Up.neighbor([3, 3]), [3, 2]);
        assert_eq!(Direction::Down.neighbor([3, 3]), [3, 4]);
        assert_eq!(Direction::Left.neighbor([3, 3]), [2, 3]);
        assert_eq!(Direction::Right.neighbor([3, 3]), [4, 3]);
        for direction in Direction::ALL {
            let there = direction.neighbor([0, 0]);
            assert_eq!(direction.opposite().neighbor(there), [0, 0]);
        }
    }

    #[test]
    fn a_step_moves_between_tiles_over_its_time() {
        let mut mover = GridMover::new([2, 2], 0.5);
        assert!(mover.step(Direction::Right, open));
        assert_eq!(mover.tile(), [3, 2], "the destination is taken at once");
        assert_eq!(mover.position(), [2.0, 2.0]);
        mover.update(0.25);
        assert_eq!(mover.position(), [2.5, 2.0]);
        assert!(mover.is_moving());
        mover.update(0.25);
        assert_eq!(mover.position(), [3.0, 2.0]);
        assert!(!mover.is_moving());
    }

    #[test]
    fn steps_wait_for_the_one_under_way() {
        let mut mover = GridMover::new([0, 0], 0.5);
        assert!(mover.step(Direction::Down, open));
        assert!(!mover.step(Direction::Right, open));
        assert_eq!(mover.tile(), [0, 1]);
        assert_eq!(mover.facing(), Direction::Down);
    }

    #[test]
    fn blocked_steps_only_turn() {
        let mut mover = GridMover::new([0, 0], 0.5);
        let mut checked = None;
        let stepped = mover.step(Direction::Left, |tile| {
            checked = Some(tile);
            true
        });
        assert!(!stepped);
        assert_eq!(checked, Some([-1, 0]));
        assert_eq!(mover.tile(), [0, 0]);
        assert_eq!(mover.facing(), Direction::Left);
        assert!(!mover.is_moving());
    }

    #[test]
    fn walking_on_keeps_the_time_left_over() {
        let mut mover = GridMover::new([0, 0], 0.5);
        mover.step(Direction::Right, open);
        mover.update(0.625);
        assert!(!mover.is_moving());
        mover.step(Direction::Right, open);
        assert_eq!(mover.position(), [1.25, 0.0]);
    }

    #[test]
    fn left_over_time_expires_after_a_frame() {
        let mut mover = GridMover::new([0, 0], 0.5);
        mover.step(Direction::Right, open);
        mover.update(0.625);
        mover.update(0.1);
        mover.step(Direction::Right, open);
        assert_eq!(mover.position(), [1.0, 0.0], "standing still in between");
    }

    #[test]
    fn placing_ends_the_step() {
        let mut mover = GridMover::new([0, 0], 0.5);
        mover.step(Direction::Right, open);
        mover.place([5, 5]);
        assert!(!mover.is_moving());
        assert_eq!(mover.position(), [5.0, 5.0]);
    }

    #[test]
    #[should_panic(expected = "positive")]
    fn step_time_must_be_positive() {
        GridMover::new([0, 0], 0.0);
    }
}
