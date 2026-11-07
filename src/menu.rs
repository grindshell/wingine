//! Menus that games drive: a selection among items, moved by actions and the mouse.
//!
//! A [`Menu`] neither holds its items nor draws them. Each update, the game passes where its items
//! are and which can be picked, as [`MenuItem`]s, with the frame's [`Input`], and the menu says
//! what happened, if anything: a [`MenuEvent`]. The game shows the selected item as it likes, from
//! [`Menu::selected`].
//!
//! - The direction actions move the selection by where the items are, so a list goes up and down
//!   or across and a grid every way: to the nearest item that way of those lined up with the
//!   selected one, or with none, round to the farthest of them the other way, unless
//!   [`Menu::wrap`] is off. Only with none lined up either way does it go to the nearest that
//!   isn't, as down from the end of a row over a shorter one. A held key moves it again each
//!   time the system repeats it.
//! - The confirm action picks the selected item, and the cancel action cancels the menu.
//! - The mouse selects the item it moves onto, only as it moves, so the keys choose while it
//!   rests, and clicking an item picks it.
//! - Items that aren't enabled are passed over every way, and never picked.
//!
//! Its tests don't need a GPU.

use crate::{
    input::{Input, MouseButton},
    math::Rect,
};

/// An item of a [`Menu`], as the game passes it each update.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuItem {
    /// Where it is, in the coordinates of the mouse: canvas pixels, for a menu drawn without a
    /// camera.
    pub rect: Rect,
    /// Whether it can be selected and picked.
    pub enabled: bool,
}

impl MenuItem {
    /// An item that can be picked, at `rect`.
    pub fn new(rect: Rect) -> Self {
        Self {
            rect,
            enabled: true,
        }
    }
}

/// The actions a [`Menu`] answers to, by name, as [`Input::bind`] binds them. An action that
/// isn't bound is never pressed, so a menu without one goes without it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuActions {
    pub up: String,
    pub down: String,
    pub left: String,
    pub right: String,
    /// Picks the selected item.
    pub confirm: String,
    /// Cancels the menu.
    pub cancel: String,
}

impl Default for MenuActions {
    /// `"up"`, `"down"`, `"left"`, `"right"`, `"confirm"` and `"cancel"`.
    fn default() -> Self {
        Self {
            up: "up".to_owned(),
            down: "down".to_owned(),
            left: "left".to_owned(),
            right: "right".to_owned(),
            confirm: "confirm".to_owned(),
            cancel: "cancel".to_owned(),
        }
    }
}

/// What happened to a [`Menu`] in an update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEvent {
    /// The player moved the selection to this item, by an action or the mouse.
    Moved(usize),
    /// The player picked this item, by the confirm action or a click.
    Picked(usize),
    /// The player cancelled the menu.
    Cancelled,
}

/// A selection among items, which [`Menu::update`] moves by the player's input: see the
/// [module docs](self).
#[derive(Clone, Debug)]
pub struct Menu {
    pub actions: MenuActions,
    /// Whether moving past the last item that way goes round to the farthest the other way. On
    /// by default.
    pub wrap: bool,
    /// Whether the mouse selects and picks items. On by default.
    pub mouse: bool,
    selected: Option<usize>,
    /// Where the cursor was at the last update, if it was in the window, so the menu can tell
    /// when it moves.
    cursor: Option<[f32; 2]>,
}

impl Default for Menu {
    fn default() -> Self {
        Self::new()
    }
}

impl Menu {
    /// A menu answering to the default actions, which wraps and takes the mouse, with nothing
    /// selected until it's first refreshed or updated.
    pub fn new() -> Self {
        Self {
            actions: MenuActions::default(),
            wrap: true,
            mouse: true,
            selected: None,
            cursor: None,
        }
    }

    /// The selected item, by its index among the items the menu was last given.
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Selects item `index`, or nothing. Until the menu is refreshed or updated, it may not be an
    /// item that can be picked.
    pub fn select(&mut self, index: Option<usize>) {
        self.selected = index;
    }

    /// Notes where the cursor is, without moving the selection, as a menu opening under it does:
    /// otherwise the cursor counts as moving onto the item it's over at the next update, as it
    /// does coming into the window.
    pub fn track_cursor(&mut self, input: &Input) {
        self.cursor = input.mouse_position();
    }

    /// Moves the selection onto an item that can be picked, if it isn't on one, as
    /// [`Menu::update`] does first: to the next one after it that can be, going round from the
    /// first, or with none, to nothing.
    pub fn refresh(&mut self, items: &[MenuItem]) {
        let enabled = |index: usize| items.get(index).is_some_and(|item| item.enabled);
        if self.selected.is_some_and(enabled) {
            return;
        }
        let start = self
            .selected
            .filter(|&index| index < items.len())
            .unwrap_or(0);
        self.selected = (0..items.len())
            .map(|offset| (start + offset) % items.len())
            .find(|&index| enabled(index));
    }

    /// Takes the player's input for this frame, and returns what they did, if anything. `items`
    /// are the menu's items as they are now.
    ///
    /// Clicking an item picks it; the confirm action picks the selected item, while there is
    /// one; the cancel action cancels the menu. Otherwise, the mouse moving onto an item and the
    /// direction actions move the selection, and the event says where it went if it moved.
    pub fn update(&mut self, items: &[MenuItem], input: &Input) -> Option<MenuEvent> {
        self.refresh(items);
        let before = self.selected;
        if self.mouse {
            let cursor = input.mouse_position();
            let moved = cursor != self.cursor;
            self.cursor = cursor;
            let pointed = cursor.and_then(|point| {
                items
                    .iter()
                    .position(|item| item.enabled && item.rect.contains(point))
            });
            if let Some(pointed) = pointed {
                if input.mouse_pressed(MouseButton::Left) {
                    self.selected = Some(pointed);
                    return Some(MenuEvent::Picked(pointed));
                }
                if moved {
                    self.selected = Some(pointed);
                }
            }
        }
        if let Some(selected) = self.selected
            && input.pressed(&self.actions.confirm)
        {
            return Some(MenuEvent::Picked(selected));
        }
        if input.pressed(&self.actions.cancel) {
            return Some(MenuEvent::Cancelled);
        }
        let directions = [
            (&self.actions.up, Axis::Y, -1.0),
            (&self.actions.down, Axis::Y, 1.0),
            (&self.actions.left, Axis::X, -1.0),
            (&self.actions.right, Axis::X, 1.0),
        ];
        let pushed = directions
            .into_iter()
            .find(|(action, ..)| input.pressed(action) || input.repeated(action));
        if let (Some((_, axis, sign)), Some(from)) = (pushed, self.selected)
            && let Some(next) = neighbor(items, from, axis, sign, self.wrap)
        {
            self.selected = Some(next);
        }
        match self.selected {
            Some(selected) if self.selected != before => Some(MenuEvent::Moved(selected)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
}

/// An item's extent along an axis: where it starts, its middle, and where it ends.
fn extent(rect: Rect, axis: Axis) -> [f32; 3] {
    let (start, length) = match axis {
        Axis::X => (rect.x, rect.w),
        Axis::Y => (rect.y, rect.h),
    };
    [start, start + length / 2.0, start + length]
}

/// The item to move to from item `from` along `axis`, the way `sign` says, among those enabled:
/// of those lined up with it, the nearest whose middle is past its far edge, or if the menu wraps,
/// the farthest whose middle is before its near edge; and only with none lined up either way,
/// the same of the rest.
fn neighbor(items: &[MenuItem], from: usize, axis: Axis, sign: f32, wrap: bool) -> Option<usize> {
    let across = match axis {
        Axis::X => Axis::Y,
        Axis::Y => Axis::X,
    };
    let here = items[from].rect;
    let [start, middle, end] = extent(here, axis);
    let [low, centre, high] = extent(here, across);
    // From the middle of `from` along the way to go, so negative before it.
    let (near, far) = if sign > 0.0 {
        (start, end)
    } else {
        (end, start)
    };
    let candidates: Vec<Candidate> = items
        .iter()
        .enumerate()
        .filter(|&(index, item)| index != from && item.enabled)
        .map(|(index, item)| {
            let [_, along, _] = extent(item.rect, axis);
            let [their_low, their_centre, their_high] = extent(item.rect, across);
            // Either's middle is across from the other.
            let lined_up =
                (low..=high).contains(&their_centre) || (their_low..=their_high).contains(&centre);
            Candidate {
                index,
                lined_up,
                past: (along - far) * sign > 0.0,
                before: (along - near) * sign < 0.0,
                distance: ((along - middle) * sign).abs(),
                off: (their_centre - centre).abs(),
            }
        })
        .collect();
    // An aligned item behind us still marks this row or column's end when wrapping is off.
    // Only leave it diagonally when there are no aligned items in either direction.
    let lined_up = candidates
        .iter()
        .any(|candidate| candidate.lined_up && (candidate.past || candidate.before));
    candidates
        .iter()
        .filter(|candidate| candidate.past && (candidate.lined_up || !lined_up))
        .min_by(|a, b| a.order(b, false))
        .or_else(|| {
            candidates
                .iter()
                .filter(|candidate| wrap && candidate.before && (candidate.lined_up || !lined_up))
                .min_by(|a, b| a.order(b, true))
        })
        .map(|candidate| candidate.index)
}

/// An item the selection might move to.
#[derive(Clone, Copy)]
struct Candidate {
    index: usize,
    lined_up: bool,
    /// Whether its middle is past the selected item's far edge, the way to go.
    past: bool,
    /// Whether its middle is before the selected item's near edge.
    before: bool,
    /// How far its middle is from the selected item's, the way to go and across.
    distance: f32,
    off: f32,
}

impl Candidate {
    /// Which of two candidates to prefer: the one lined up, then the nearer, or when wrapping,
    /// the farther, then the one less far across.
    fn order(&self, other: &Self, farthest: bool) -> std::cmp::Ordering {
        let distance = if farthest {
            other.distance.total_cmp(&self.distance)
        } else {
            self.distance.total_cmp(&other.distance)
        };
        other
            .lined_up
            .cmp(&self.lined_up)
            .then(distance)
            .then(self.off.total_cmp(&other.off))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Key;

    fn input() -> Input {
        let mut input = Input::new();
        input.bind("up", [Key::Up]);
        input.bind("down", [Key::Down]);
        input.bind("left", [Key::Left]);
        input.bind("right", [Key::Right]);
        input.bind("confirm", [Key::Z]);
        input.bind("cancel", [Key::X]);
        input
    }

    /// Updates `menu` with `key` pressed, if any, then ends the frame.
    fn press(
        menu: &mut Menu,
        items: &[MenuItem],
        input: &mut Input,
        key: Key,
    ) -> Option<MenuEvent> {
        input.key_down(key);
        let event = menu.update(items, input);
        input.key_up(key);
        input.end_frame();
        event
    }

    fn idle(menu: &mut Menu, items: &[MenuItem], input: &mut Input) -> Option<MenuEvent> {
        let event = menu.update(items, input);
        input.end_frame();
        event
    }

    /// `count` items in a column, each 40 by 10, with a gap of 2.
    fn column(count: usize) -> Vec<MenuItem> {
        (0..count)
            .map(|i| MenuItem::new(Rect::new(20.0, 10.0 + 12.0 * i as f32, 40.0, 10.0)))
            .collect()
    }

    /// Items in rows of `columns`, each 20 by 10, the last row as long as `count` leaves it.
    fn grid(columns: usize, count: usize) -> Vec<MenuItem> {
        (0..count)
            .map(|i| {
                let [column, row] = [(i % columns) as f32, (i / columns) as f32];
                MenuItem::new(Rect::new(24.0 * column, 12.0 * row, 20.0, 10.0))
            })
            .collect()
    }

    #[test]
    fn a_column_goes_up_and_down_and_wraps() {
        let (mut menu, items, mut input) = (Menu::new(), column(3), input());
        assert_eq!(menu.selected(), None, "nothing until it's given items");
        assert_eq!(idle(&mut menu, &items, &mut input), None);
        assert_eq!(menu.selected(), Some(0), "the first");
        let down = press(&mut menu, &items, &mut input, Key::Down);
        assert_eq!(down, Some(MenuEvent::Moved(1)));
        press(&mut menu, &items, &mut input, Key::Down);
        let wrapped = press(&mut menu, &items, &mut input, Key::Down);
        assert_eq!(wrapped, Some(MenuEvent::Moved(0)), "round to the top");
        let up = press(&mut menu, &items, &mut input, Key::Up);
        assert_eq!(up, Some(MenuEvent::Moved(2)), "round to the bottom");
        // Nothing is beside them.
        assert_eq!(press(&mut menu, &items, &mut input, Key::Left), None);
        assert_eq!(press(&mut menu, &items, &mut input, Key::Right), None);
        assert_eq!(menu.selected(), Some(2));

        menu.wrap = false;
        assert_eq!(press(&mut menu, &items, &mut input, Key::Down), None);
        assert_eq!(menu.selected(), Some(2), "it stops at the end");
    }

    #[test]
    fn confirming_picks_and_cancelling_cancels() {
        let (mut menu, items, mut input) = (Menu::new(), column(3), input());
        press(&mut menu, &items, &mut input, Key::Down);
        let picked = press(&mut menu, &items, &mut input, Key::Z);
        assert_eq!(picked, Some(MenuEvent::Picked(1)));
        let cancelled = press(&mut menu, &items, &mut input, Key::X);
        assert_eq!(cancelled, Some(MenuEvent::Cancelled));

        // Without items, there's nothing to pick, but the menu still cancels.
        let mut empty = Menu::new();
        assert_eq!(press(&mut empty, &[], &mut input, Key::Z), None);
        let cancelled = press(&mut empty, &[], &mut input, Key::X);
        assert_eq!(cancelled, Some(MenuEvent::Cancelled));
    }

    #[test]
    fn actions_are_named_by_the_game() {
        let (mut menu, items, mut input) = (Menu::new(), column(3), input());
        input.bind("interact", [Key::Space]);
        menu.actions.confirm = "interact".to_owned();
        assert_eq!(press(&mut menu, &items, &mut input, Key::Z), None);
        let picked = press(&mut menu, &items, &mut input, Key::Space);
        assert_eq!(picked, Some(MenuEvent::Picked(0)));

        // An action that isn't bound does nothing.
        menu.actions.down = "unbound".to_owned();
        assert_eq!(press(&mut menu, &items, &mut input, Key::Down), None);
    }

    #[test]
    fn held_keys_move_again_as_they_repeat() {
        let (mut menu, items, mut input) = (Menu::new(), column(4), input());
        input.key_down(Key::Down);
        assert_eq!(menu.update(&items, &input), Some(MenuEvent::Moved(1)));
        input.end_frame();
        assert_eq!(
            menu.update(&items, &input),
            None,
            "held, but not repeated yet"
        );
        input.end_frame();
        input.key_repeat(Key::Down);
        assert_eq!(menu.update(&items, &input), Some(MenuEvent::Moved(2)));
        input.end_frame();
        input.key_repeat(Key::Down);
        assert_eq!(menu.update(&items, &input), Some(MenuEvent::Moved(3)));
    }

    #[test]
    fn grids_are_crossed_by_where_their_items_are() {
        // Three across, and two in the last row.
        let (mut menu, items, mut input) = (Menu::new(), grid(3, 5), input());
        let mut go = |key| {
            press(&mut menu, &items, &mut input, key);
            menu.selected().unwrap()
        };
        assert_eq!(go(Key::Right), 1);
        assert_eq!(go(Key::Down), 4, "straight down");
        assert_eq!(go(Key::Right), 3, "round the row");
        assert_eq!(go(Key::Left), 4);
        assert_eq!(go(Key::Up), 1, "straight up");
        assert_eq!(go(Key::Right), 2);
        // Nothing else is in the last column, so down goes to the nearest below.
        assert_eq!(go(Key::Down), 4);
        assert_eq!(go(Key::Right), 3);
        assert_eq!(go(Key::Down), 0, "round the column");
        assert_eq!(go(Key::Up), 3);
        assert_eq!(go(Key::Left), 4, "round the row");
    }

    #[test]
    fn non_wrapping_grids_stop_at_the_end_of_a_row_or_column() {
        for transpose in [false, true] {
            let mut items = grid(3, 5);
            if transpose {
                for item in &mut items {
                    item.rect = Rect::new(item.rect.y, item.rect.x, item.rect.h, item.rect.w);
                }
            }
            let (mut menu, mut input) = (Menu::new(), input());
            menu.wrap = false;
            menu.select(Some(4));
            let forward = if transpose { Key::Down } else { Key::Right };
            assert_eq!(press(&mut menu, &items, &mut input, forward), None);
            assert_eq!(menu.selected(), Some(4));

            // Wrapping stays within that row or column.
            menu.wrap = true;
            assert_eq!(
                press(&mut menu, &items, &mut input, forward),
                Some(MenuEvent::Moved(3))
            );
            // A lone item in its column or row can still go diagonally to the shorter one.
            menu.wrap = false;
            menu.select(Some(2));
            let across = if transpose { Key::Right } else { Key::Down };
            assert_eq!(
                press(&mut menu, &items, &mut input, across),
                Some(MenuEvent::Moved(4))
            );
            // An item that isn't enabled doesn't keep the selection in its row or column.
            items[3].enabled = false;
            assert_eq!(
                press(&mut menu, &items, &mut input, forward),
                Some(MenuEvent::Moved(2))
            );
        }
    }

    #[test]
    fn rows_of_items_of_different_heights_only_go_across() {
        // Lined up along their tops, so their middles aren't level.
        let items = [
            MenuItem::new(Rect::new(0.0, 0.0, 20.0, 10.0)),
            MenuItem::new(Rect::new(24.0, 0.0, 20.0, 14.0)),
            MenuItem::new(Rect::new(48.0, 0.0, 20.0, 6.0)),
        ];
        let (mut menu, mut input) = (Menu::new(), input());
        assert_eq!(press(&mut menu, &items, &mut input, Key::Down), None);
        assert_eq!(press(&mut menu, &items, &mut input, Key::Up), None);
        let right = press(&mut menu, &items, &mut input, Key::Right);
        assert_eq!(right, Some(MenuEvent::Moved(1)));
        assert_eq!(press(&mut menu, &items, &mut input, Key::Down), None);
    }

    #[test]
    fn items_that_arent_enabled_are_passed_over() {
        let mut items = column(4);
        items[0].enabled = false;
        items[2].enabled = false;
        let (mut menu, mut input) = (Menu::new(), input());
        menu.refresh(&items);
        assert_eq!(menu.selected(), Some(1), "the first that's enabled");
        let down = press(&mut menu, &items, &mut input, Key::Down);
        assert_eq!(down, Some(MenuEvent::Moved(3)));
        let wrapped = press(&mut menu, &items, &mut input, Key::Down);
        assert_eq!(wrapped, Some(MenuEvent::Moved(1)));

        // The selection moves off an item that stops being enabled, to the next that is, without
        // an event, since the player didn't move it.
        items[1].enabled = false;
        assert_eq!(idle(&mut menu, &items, &mut input), None);
        assert_eq!(menu.selected(), Some(3));

        // The mouse passes over them too, and clicking one does nothing.
        input.set_mouse_position(Some([30.0, 15.0]));
        assert_eq!(idle(&mut menu, &items, &mut input), None);
        input.mouse_down(MouseButton::Left);
        assert_eq!(idle(&mut menu, &items, &mut input), None);
        input.mouse_up(MouseButton::Left);

        // With none enabled, nothing is selected or picked.
        let none = [MenuItem {
            enabled: false,
            ..items[0]
        }];
        assert_eq!(press(&mut menu, &none, &mut input, Key::Z), None);
        assert_eq!(menu.selected(), None);
    }

    #[test]
    fn the_mouse_selects_as_it_moves_and_picks_by_clicking() {
        let (mut menu, items, mut input) = (Menu::new(), column(3), input());
        // Opening under the cursor, the menu keeps its selection until the cursor moves.
        input.set_mouse_position(Some([30.0, 27.0]));
        menu.track_cursor(&input);
        assert_eq!(idle(&mut menu, &items, &mut input), None);
        assert_eq!(menu.selected(), Some(0));
        // Moving onto an item selects it.
        input.set_mouse_position(Some([31.0, 27.0]));
        assert_eq!(
            idle(&mut menu, &items, &mut input),
            Some(MenuEvent::Moved(1))
        );
        // While it rests, the keys choose.
        let down = press(&mut menu, &items, &mut input, Key::Down);
        assert_eq!(down, Some(MenuEvent::Moved(2)));
        assert_eq!(idle(&mut menu, &items, &mut input), None);
        assert_eq!(menu.selected(), Some(2));
        // Moving between items, or off them, changes nothing.
        input.set_mouse_position(Some([30.0, 21.0]));
        assert_eq!(idle(&mut menu, &items, &mut input), None);
        input.set_mouse_position(None);
        assert_eq!(idle(&mut menu, &items, &mut input), None);
        // Coming back into the window onto an item is moving onto it.
        input.set_mouse_position(Some([30.0, 15.0]));
        assert_eq!(
            idle(&mut menu, &items, &mut input),
            Some(MenuEvent::Moved(0))
        );

        // Clicking outside the items does nothing; clicking one picks it, moving or not.
        input.set_mouse_position(Some([5.0, 15.0]));
        input.mouse_down(MouseButton::Left);
        assert_eq!(idle(&mut menu, &items, &mut input), None);
        input.mouse_up(MouseButton::Left);
        input.set_mouse_position(Some([30.0, 39.0]));
        idle(&mut menu, &items, &mut input);
        input.mouse_down(MouseButton::Left);
        assert_eq!(
            idle(&mut menu, &items, &mut input),
            Some(MenuEvent::Picked(2))
        );
        input.mouse_up(MouseButton::Left);

        // A menu that doesn't know where the cursor was takes it as having moved there.
        let mut fresh = Menu::new();
        let moved = idle(&mut fresh, &items, &mut input);
        assert_eq!(moved, Some(MenuEvent::Moved(2)));

        // A menu that doesn't take the mouse ignores it.
        menu.mouse = false;
        input.set_mouse_position(Some([30.0, 15.0]));
        input.mouse_down(MouseButton::Left);
        assert_eq!(idle(&mut menu, &items, &mut input), None);
        assert_eq!(menu.selected(), Some(2));
    }
}
