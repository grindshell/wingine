//! The Wingine editor: a wingine game that edits a game's scenes, laid out like Godot's editor.
//! It's run by its executable, `wingine-editor`; this library is what it's made of, for tests.

pub mod docks;
pub mod document;
pub mod edit;
pub mod editor;
pub mod instance;
pub mod layout;
pub mod play;
pub mod project;
pub mod theme;
pub mod viewport;

pub use editor::Editor;
