//! The editor: a wingine game whose interface is all egui, laid out as Godot's editor is. The
//! menus, the main screens and Play run along the top; the Scene dock sits over the FileSystem
//! dock on the left, the Inspector on the right, and Output and Tiles in a panel along the bottom;
//! the open scenes are tabs over the viewport in the middle.

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use wingine::{
    Context, Game,
    graphics::Canvas,
    gui::egui::{self, Key, KeyboardShortcut, Modifiers},
    math::Affine,
    scene::KindDef,
};

use crate::{
    docks::{
        filesystem::{self, FileSystem},
        inspector::{Inspector, Services},
        output::{Level, Output},
        scene_tree::{self, Addable, SceneTree},
        tiles::{Palette, Target, Tiles},
    },
    document::Document,
    edit::{self, NodePath},
    play::Play,
    project::{AssetCache, Project},
    theme,
    viewport::{self, Tool, Viewport},
};

/// How often the project's folder is read again, in seconds, to see files changed elsewhere.
const RESCAN_TIME: f32 = 3.0;

/// The big views in the middle of the editor, switched between at the top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainScreen {
    /// The viewport of the open scene.
    TwoD,
    /// Editing scripts, which a later part of the milestone brings.
    Script,
}

/// The panels of the bottom panel, switched between by the tabs along its bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BottomTab {
    Output,
    Tiles,
}

/// What's waiting for an answer about unsaved changes before it closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Closing {
    Tab(usize),
    Window,
}

pub struct Editor {
    project: Option<Project>,
    documents: Vec<Document>,
    /// The index of the tab shown, if there are any.
    current: usize,
    screen: MainScreen,
    files: FileSystem,
    tree: SceneTree,
    inspector: Inspector,
    viewport: Viewport,
    output: Output,
    tiles: Tiles,
    /// Whether the panel along the bottom is open, and which of its panels it shows.
    bottom_open: bool,
    bottom_tab: BottomTab,
    /// The scene and node selected last frame, to notice a tile layer being selected.
    last_selected: Option<(String, NodePath)>,
    play: Option<Play>,
    /// What plays the game: an executable that runs the game it's given after `--run`.
    runner: PathBuf,
    /// Environment variables for the game played.
    play_env: Vec<(OsString, OsString)>,
    cache: AssetCache,
    /// The folder being chosen in the Open dialog, while it's open.
    opening: Option<String>,
    closing: Option<Closing>,
    themed: bool,
    /// Seconds until the project's folder is read again.
    rescan_in: f32,
}

impl Editor {
    /// An editor of the game in `folder`, or of nothing yet, to open a game from the start.
    pub fn new(folder: Option<PathBuf>) -> Self {
        let mut editor = Self {
            project: None,
            documents: Vec::new(),
            current: 0,
            screen: MainScreen::TwoD,
            files: FileSystem::default(),
            tree: SceneTree::default(),
            inspector: Inspector::default(),
            viewport: Viewport::default(),
            output: Output::default(),
            tiles: Tiles::default(),
            bottom_open: true,
            bottom_tab: BottomTab::Output,
            last_selected: None,
            play: None,
            runner: std::env::current_exe().unwrap_or_else(|_| "wingine-editor".into()),
            play_env: Vec::new(),
            cache: AssetCache::default(),
            opening: None,
            closing: None,
            themed: false,
            rescan_in: RESCAN_TIME,
        };
        if let Some(folder) = folder
            && let Err(err) = editor.open_project(folder)
        {
            editor.output.push(Level::Error, err);
        }
        editor
    }

    /// Opens the game in `folder`, closing the one open now, if any.
    pub fn open_project(&mut self, folder: impl Into<PathBuf>) -> Result<(), String> {
        let project = Project::open(folder)?;
        self.stop();
        self.documents.clear();
        self.current = 0;
        self.cache = AssetCache::default();
        let message = format!("Opened {}", project.name);
        if let Err(err) = &project.config {
            self.output.push(Level::Error, format!("conf.luau: {err}"));
        }
        if !project.has_main() {
            self.output.push(
                Level::Warning,
                "The folder has no main.luau, so there's no game to play.",
            );
        }
        self.project = Some(project);
        self.output.push(Level::Info, message);
        Ok(())
    }

    /// Opens the scene file at `path` in the project, or shows its tab if it's open.
    pub fn open_scene(&mut self, path: &str) -> Result<(), String> {
        if let Some(i) = self.documents.iter().position(|doc| doc.path == path) {
            self.current = i;
            return Ok(());
        }
        let project = self.project.as_ref().ok_or("no game is open")?;
        let document = Document::open(&project.folder, path).map_err(|err| err.to_string())?;
        self.documents.push(document);
        self.current = self.documents.len() - 1;
        Ok(())
    }

    pub fn project(&self) -> Option<&Project> {
        self.project.as_ref()
    }

    pub fn documents(&self) -> &[Document] {
        &self.documents
    }

    /// The scene shown.
    pub fn document(&self) -> Option<&Document> {
        self.documents.get(self.current)
    }

    pub fn document_mut(&mut self) -> Option<&mut Document> {
        self.documents.get_mut(self.current)
    }

    pub fn output(&self) -> &Output {
        &self.output
    }

    pub fn tool(&self) -> Tool {
        self.viewport.tool
    }

    /// What painting tiles paints with.
    pub fn tiles(&self) -> &Tiles {
        &self.tiles
    }

    pub fn bottom_tab(&self) -> BottomTab {
        self.bottom_tab
    }

    /// Where `point` of the open scene was in the window, in points, as the viewport last showed
    /// it: for tests, to click on nodes.
    pub fn scene_to_window(&self, point: [f32; 2]) -> Option<[f32; 2]> {
        let pos = self.viewport.scene_to_window(point)?;
        Some([pos.x, pos.y])
    }

    pub fn is_playing(&self) -> bool {
        self.play.is_some()
    }

    /// Plays games with `runner` instead of the editor's own executable, with `env` set: as tests
    /// do, which run in an executable of their own.
    pub fn set_runner(&mut self, runner: impl Into<PathBuf>, env: Vec<(OsString, OsString)>) {
        self.runner = runner.into();
        self.play_env = env;
    }

    /// Saves the scene shown.
    pub fn save(&mut self) {
        let (Some(project), Some(document)) = (&self.project, self.documents.get_mut(self.current))
        else {
            return;
        };
        match document.save(&project.folder) {
            Ok(()) => {
                let message = format!("Saved {}", document.path);
                self.output.push(Level::Info, message);
                // Other scenes may place it.
                for document in &mut self.documents {
                    document.rebuild();
                }
            }
            Err(err) => self.output.push(Level::Error, err.to_string()),
        }
    }

    /// Saves every scene with unsaved changes.
    pub fn save_all(&mut self) {
        let current = self.current;
        for i in 0..self.documents.len() {
            if self.documents[i].is_dirty() {
                self.current = i;
                self.save();
            }
        }
        self.current = current;
    }

    /// Plays the game, saving every scene first, as Godot does.
    pub fn play(&mut self) {
        let Some(project) = &self.project else {
            return;
        };
        if !project.has_main() {
            self.output
                .push(Level::Error, "The game has no main.luau to play.");
            return;
        }
        self.stop();
        self.save_all();
        let Some(project) = &self.project else {
            return;
        };
        let env = self
            .play_env
            .iter()
            .map(|(name, value)| (name.as_os_str(), value.as_os_str()));
        match Play::start(&self.runner, &project.folder, env) {
            Ok(play) => {
                self.output
                    .push(Level::Info, format!("Playing {}", project.name));
                self.play = Some(play);
            }
            Err(err) => self
                .output
                .push(Level::Error, format!("Couldn't play the game: {err}")),
        }
    }

    pub fn stop(&mut self) {
        if let Some(mut play) = self.play.take() {
            play.stop();
            self.take_play_output(&play);
            self.output.push(Level::Info, "Stopped the game.");
        }
    }

    fn take_play_output(&mut self, play: &Play) {
        for line in play.lines() {
            self.output.push_game_line(line);
        }
    }

    /// Closes the tab at `index`, asking first if it has unsaved changes, unless `force`.
    fn close_tab(&mut self, index: usize, force: bool) {
        if index >= self.documents.len() {
            return;
        }
        if !force && self.documents[index].is_dirty() {
            self.closing = Some(Closing::Tab(index));
            return;
        }
        self.documents.remove(index);
        if self.current > index || self.current == self.documents.len() {
            self.current = self.current.saturating_sub(1);
        }
    }

    /// Does what the Scene dock asks.
    fn apply(&mut self, action: scene_tree::Action) {
        let tileset = self
            .project
            .as_ref()
            .and_then(|project| project.files_ending(".tileset.luau").into_iter().next())
            .unwrap_or_default();
        let Some(document) = self.documents.get_mut(self.current) else {
            return;
        };
        let result = match action {
            scene_tree::Action::Select(path) => {
                document.select(Some(path));
                Ok(())
            }
            scene_tree::Action::SelectInner { instance, names } => {
                document.select_inner(instance, names);
                Ok(())
            }
            scene_tree::Action::Add { parent, kind } => {
                let sheet = document
                    .file()
                    .sheets
                    .keys()
                    .next()
                    .cloned()
                    .unwrap_or_default();
                let node = edit::new_node(kind, &sheet, &tileset, "");
                edit::add(document.edit(), &parent, node).map(|path| {
                    document.select(Some(path));
                })
            }
            scene_tree::Action::Instance { parent, scene } => {
                add_instance(document, &parent, &scene, [0.0, 0.0])
            }
            scene_tree::Action::Rename(path, name) => edit::rename(document.edit(), &path, &name),
            scene_tree::Action::Duplicate(path) => {
                if let Some(copy) = edit::duplicate(document.edit(), &path) {
                    document.select(Some(copy));
                }
                Ok(())
            }
            scene_tree::Action::Delete(path) => {
                edit::remove(document.edit(), &path);
                document.select(None);
                Ok(())
            }
            scene_tree::Action::Move {
                from,
                parent,
                index,
            } => edit::move_node(document.edit(), &from, &parent, index).map(|path| {
                document.select(Some(path));
            }),
        };
        // Each is a change of its own.
        document.end_change();
        if let Err(err) = result {
            self.output.push(Level::Error, err);
        }
    }

    fn shortcuts(&mut self, ui: &mut egui::Ui) {
        let typing = ui.ctx().egui_wants_keyboard_input();
        let shortcut = |modifiers, key| KeyboardShortcut::new(modifiers, key);
        let pressed = |ui: &mut egui::Ui, modifiers, key| {
            ui.input_mut(|input| input.consume_shortcut(&shortcut(modifiers, key)))
        };
        if pressed(ui, Modifiers::COMMAND, Key::S) {
            self.save();
        }
        if pressed(ui, Modifiers::NONE, Key::F5) {
            self.play();
        }
        if pressed(ui, Modifiers::NONE, Key::F8) {
            self.stop();
        }
        if typing {
            return;
        }
        let redo = pressed(ui, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
            || pressed(ui, Modifiers::COMMAND, Key::Y);
        if redo && let Some(document) = self.document_mut() {
            document.redo();
        }
        if pressed(ui, Modifiers::COMMAND, Key::Z)
            && let Some(document) = self.document_mut()
        {
            document.undo();
        }
        let selected = self
            .document()
            .and_then(|document| document.selected.clone())
            .filter(|path| !path.is_empty());
        // Escape closes menus first.
        if !egui::Popup::is_any_open(ui.ctx())
            && pressed(ui, Modifiers::NONE, Key::Escape)
            && let Some(document) = self.document_mut()
        {
            document.select(None);
        }
        if let Some(path) = selected {
            if pressed(ui, Modifiers::COMMAND, Key::D) {
                self.apply(scene_tree::Action::Duplicate(path.clone()));
            }
            if pressed(ui, Modifiers::NONE, Key::Delete) {
                self.apply(scene_tree::Action::Delete(path.clone()));
            }
            if pressed(ui, Modifiers::NONE, Key::F2)
                && let Some(document) = self.documents.get(self.current)
            {
                self.tree.rename(document, path);
            }
        }
        for (tool, _, key) in Tool::ALL {
            let key = Key::from_name(key).expect("tools' keys are letters");
            if pressed(ui, Modifiers::NONE, key) {
                self.viewport.tool = tool;
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, ctx: &mut Context) {
        let has_document = !self.documents.is_empty();
        let can_play = self.project.as_ref().is_some_and(Project::has_main);
        let bar = ui.max_rect();
        // The menus take the bar's row; the main screens and Play go over it, in the middle and
        // at the right.
        let mut middle = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_center_size(
                    bar.center(),
                    egui::vec2(180.0, bar.height()),
                ))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        middle.selectable_value(&mut self.screen, MainScreen::TwoD, "🖼 2D");
        middle
            .add_enabled(
                false,
                egui::Button::selectable(self.screen == MainScreen::Script, "📜 Script"),
            )
            .on_disabled_hover_text("Editing scripts comes in a later part of the milestone");
        let mut right = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        let playing = self.play.is_some();
        if right
            .add_enabled(playing, egui::Button::new("⏹ Stop"))
            .on_hover_text("Stop the game (F8)")
            .clicked()
        {
            self.stop();
        }
        if right
            .add_enabled(can_play && !playing, egui::Button::new("▶ Play"))
            .on_hover_text("Save every scene and play the game (F5)")
            .clicked()
        {
            self.play();
        }
        {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("Scene", |ui| {
                    if ui
                        .add_enabled(
                            has_document,
                            egui::Button::new("Save").shortcut_text("Ctrl+S"),
                        )
                        .clicked()
                    {
                        self.save();
                    }
                    if ui
                        .add_enabled(has_document, egui::Button::new("Save All"))
                        .clicked()
                    {
                        self.save_all();
                    }
                    if ui
                        .add_enabled(has_document, egui::Button::new("Close Scene"))
                        .clicked()
                    {
                        self.close_tab(self.current, false);
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() && self.close_requested(ctx) {
                        ctx.quit();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    let (can_undo, can_redo) = self
                        .document()
                        .map_or((false, false), |doc| (doc.can_undo(), doc.can_redo()));
                    if ui
                        .add_enabled(can_undo, egui::Button::new("Undo").shortcut_text("Ctrl+Z"))
                        .clicked()
                        && let Some(document) = self.document_mut()
                    {
                        document.undo();
                    }
                    if ui
                        .add_enabled(can_redo, egui::Button::new("Redo").shortcut_text("Ctrl+Y"))
                        .clicked()
                        && let Some(document) = self.document_mut()
                    {
                        document.redo();
                    }
                });
                ui.menu_button("Project", |ui| {
                    if ui.button("Open Folder…").clicked() {
                        self.start_opening();
                    }
                    let open = self.project.is_some();
                    if ui
                        .add_enabled(open, egui::Button::new("Read Files Again"))
                        .clicked()
                        && let Some(project) = &mut self.project
                    {
                        project.rescan();
                    }
                });
                ui.menu_button("Help", |ui| {
                    ui.label("Wingine's editor, laid out like Godot's.");
                    ui.label("Middle-drag pans, the wheel zooms, Q W E S pick tools.");
                    ui.label(
                        "Select a tile layer to paint it: the right button erases, Ctrl picks.",
                    );
                });
            });
        }
    }

    fn start_opening(&mut self) {
        let start = self
            .project
            .as_ref()
            .map(|project| project.folder.clone())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        self.opening = Some(start.display().to_string());
    }

    /// The Open dialog: a folder's path, and the folders in it to go into.
    fn open_dialog(&mut self, ui: &mut egui::Ui) {
        let Some(path) = &mut self.opening else {
            return;
        };
        let mut open = None;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("open dialog")).show(ui.ctx(), |ui| {
            ui.set_width(480.0);
            ui.heading("Open a Game");
            ui.horizontal(|ui| {
                if ui.button("⬆").on_hover_text("The folder above").clicked()
                    && let Some(parent) = Path::new(path.as_str()).parent()
                {
                    *path = parent.display().to_string();
                }
                ui.add(egui::TextEdit::singleline(path).desired_width(f32::INFINITY));
            });
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for (name, folder, is_game) in subfolders(Path::new(path.as_str())) {
                        let icon = if is_game { "🎮" } else { "📁" };
                        let response = ui.selectable_label(false, format!("{icon} {name}"));
                        if response.clicked() {
                            *path = folder.display().to_string();
                        }
                        if response.double_clicked() || (response.clicked() && is_game) {
                            *path = folder.display().to_string();
                        }
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                let is_folder = Path::new(path.as_str()).is_dir();
                if ui
                    .add_enabled(is_folder, egui::Button::new("Open"))
                    .clicked()
                {
                    open = Some(PathBuf::from(path.as_str()));
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if let Some(folder) = open {
            self.opening = None;
            if let Err(err) = self.open_project(folder) {
                self.output.push(Level::Error, err);
            }
        } else if cancel {
            self.opening = None;
        }
    }

    /// Asks what to do about unsaved changes, for a tab or the window closing.
    fn closing_dialog(&mut self, ui: &mut egui::Ui, ctx: &mut Context) {
        let Some(closing) = self.closing else {
            return;
        };
        let names: Vec<String> = match closing {
            Closing::Tab(i) => self
                .documents
                .get(i)
                .map(|doc| doc.name().to_owned())
                .into_iter()
                .collect(),
            Closing::Window => self
                .documents
                .iter()
                .filter(|doc| doc.is_dirty())
                .map(|doc| doc.name().to_owned())
                .collect(),
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("closing dialog")).show(ui.ctx(), |ui| {
            ui.heading("Unsaved Changes");
            ui.label(format!("Save changes to {} first?", names.join(", ")));
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    answer = Some(true);
                }
                if ui.button("Don't Save").clicked() {
                    answer = Some(false);
                }
                if ui.button("Cancel").clicked() {
                    self.closing = None;
                }
            });
        });
        let Some(save) = answer else {
            return;
        };
        self.closing = None;
        match closing {
            Closing::Tab(i) => {
                if save {
                    let current = self.current;
                    self.current = i;
                    self.save();
                    self.current = current;
                }
                self.close_tab(i, true);
            }
            Closing::Window => {
                if save {
                    self.save_all();
                }
                ctx.quit();
            }
        }
    }

    /// The open scenes' tabs, along the top of the middle.
    fn tabs(&mut self, ui: &mut egui::Ui) {
        let mut close = None;
        ui.horizontal(|ui| {
            for (i, document) in self.documents.iter().enumerate() {
                let title = if document.is_dirty() {
                    format!("{}*", document.name())
                } else {
                    document.name().to_owned()
                };
                if ui.selectable_label(i == self.current, title).clicked() {
                    self.current = i;
                }
                if ui
                    .small_button("✖")
                    .on_hover_text("Close the scene")
                    .clicked()
                {
                    close = Some(i);
                }
                ui.separator();
            }
        });
        if let Some(i) = close {
            self.close_tab(i, false);
        }
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 4.0);
                ui.heading("Wingine");
                ui.label("Open a game's folder to edit its scenes.");
                if ui.button("Open Folder…").clicked() {
                    self.start_opening();
                }
                let examples = subfolders(Path::new("examples"));
                let games: Vec<_> = examples
                    .into_iter()
                    .filter(|(_, _, is_game)| *is_game)
                    .collect();
                if !games.is_empty() {
                    ui.add_space(12.0);
                    ui.label("Or one of the examples:");
                    for (name, folder, _) in games {
                        if ui.button(format!("🎮 {name}")).clicked()
                            && let Err(err) = self.open_project(folder)
                        {
                            self.output.push(Level::Error, err);
                        }
                    }
                }
            });
        });
    }
}

/// Places the scene file at `scene` in `document`, as the last child of `parent`, at `at` in the
/// parent's coordinates.
fn add_instance(
    document: &mut Document,
    parent: &[usize],
    scene: &str,
    [x, y]: [f32; 2],
) -> Result<(), String> {
    if scene == document.path {
        return Err("a scene can't be placed in itself".into());
    }
    let mut node = edit::new_node("instance", "", "", scene);
    node.x = x;
    node.y = y;
    let path = edit::add(document.edit(), parent, node)?;
    document.select(Some(path));
    Ok(())
}

/// Places the scene file at `scene` in `document` at `at`, a point of the scene, in the selected
/// node, or the tilemap of a selected layer, or the scene itself if nothing is selected.
fn place_instance(document: &mut Document, scene: &str, at: [f32; 2]) -> Result<(), String> {
    let mut parent = document.selected.clone().unwrap_or_default();
    if let Some(node) = edit::node(document.file(), &parent)
        && matches!(node.kind, KindDef::TileLayer(_))
    {
        parent.pop();
    }
    // From the scene's coordinates to the parent's.
    let to_parent = document
        .live_node(&parent)
        .zip(document.preview.as_ref())
        .and_then(|(id, scene)| scene.transform(id).inverse())
        .unwrap_or(Affine::IDENTITY);
    add_instance(document, &parent, scene, to_parent.apply(at))
}

/// What the Tiles panel paints: the selected layer of `document`, with the tiles of its map's
/// tileset, `palette`, or the layers of a selected tilemap.
fn tiles_target<'a>(
    document: Option<&'a Document>,
    selected: Option<&(String, NodePath)>,
    palette: Option<&'a Result<Palette, String>>,
) -> Target<'a> {
    let (Some(document), Some((_, path))) = (document, selected) else {
        return Target::Nothing;
    };
    let Some(node) = edit::node(document.file(), path) else {
        return Target::Nothing;
    };
    match (&node.kind, palette) {
        (KindDef::TileLayer(_), Some(palette)) => Target::Layer {
            name: &node.name,
            palette: palette.as_ref().map_err(String::as_str),
        },
        (KindDef::Tilemap(_), _) => Target::Map(
            node.children
                .iter()
                .enumerate()
                .filter(|(_, child)| matches!(child.kind, KindDef::TileLayer(_)))
                .map(|(i, child)| {
                    let mut layer = path.clone();
                    layer.push(i);
                    (child.name.clone(), layer)
                })
                .collect(),
        ),
        _ => Target::Nothing,
    }
}

/// The folders in `folder`, by name, with their paths and whether each holds a game.
fn subfolders(folder: &Path) -> Vec<(String, PathBuf, bool)> {
    let Ok(read) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<_> = read
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let path = entry.path();
            let is_game = path.join("main.luau").is_file();
            (!name.starts_with('.')).then_some((name, path, is_game))
        })
        .collect();
    found.sort();
    found
}

impl Game for Editor {
    fn update(&mut self, _ctx: &mut Context, dt: f32) {
        if let Some(mut play) = self.play.take() {
            self.take_play_output(&play);
            match play.exited() {
                Some(status) => {
                    let message = match status.code() {
                        Some(0) => "The game ended.".to_owned(),
                        Some(code) => format!("The game ended with code {code}."),
                        None => "The game was ended.".to_owned(),
                    };
                    self.output.push(Level::Info, message);
                }
                None => self.play = Some(play),
            }
        }
        self.rescan_in -= dt;
        if self.rescan_in <= 0.0 {
            self.rescan_in = RESCAN_TIME;
            if let Some(project) = &mut self.project {
                project.rescan();
                // Images and files the scenes use may have changed.
                for document in &mut self.documents {
                    document.rebuild();
                }
            }
        }
    }

    fn gui(&mut self, ctx: &mut Context, ui: &mut egui::Ui) {
        if !self.themed {
            theme::apply(ui.ctx());
            self.themed = true;
        }
        self.output.take_logged();
        self.shortcuts(ui);

        egui::Panel::top("top bar").show(ui, |ui| self.top_bar(ui, ctx));
        if self.project.is_none() {
            self.welcome(ui);
            self.open_dialog(ui);
            return;
        }

        let project = self.project.as_ref().expect("a game is open");
        let canvas_size = project.canvas_size();
        let folder = project.folder.clone();
        if let Some(document) = self.documents.get_mut(self.current)
            && let Some(error) = document.refresh(ctx, &mut self.cache, &folder, canvas_size)
        {
            self.output.push(Level::Error, error);
        }

        let mut tree_actions = Vec::new();
        let mut file_action = None;
        egui::Panel::left("left dock")
            .resizable(true)
            .default_size(280.0)
            .show(ui, |ui| {
                egui::Panel::bottom("filesystem dock")
                    .resizable(true)
                    .default_size(ui.available_height() * 0.45)
                    .show(ui, |ui| {
                        let project = self.project.as_ref().expect("a game is open");
                        file_action = self.files.ui(ui, &project.name, &project.files);
                    });
                egui::CentralPanel::default().show(ui, |ui| {
                    let Some(document) = self.documents.get(self.current) else {
                        ui.strong("Scene");
                        ui.weak("Double-click a scene in the FileSystem dock to open it.");
                        return;
                    };
                    let project = self.project.as_ref().expect("a game is open");
                    let tilesets = !project.files_ending(".tileset.luau").is_empty();
                    let scenes = project.files_ending(".scene.luau");
                    let addable = Addable {
                        sheets: !document.file().sheets.is_empty(),
                        tilesets,
                        scenes: &scenes,
                    };
                    tree_actions = self.tree.ui(ui, document, &addable);
                });
            });
        for action in tree_actions {
            self.apply(action);
        }
        match file_action {
            Some(filesystem::Action::Open(path)) => {
                if let Err(err) = self.open_scene(&path) {
                    self.output.push(Level::Error, err);
                }
            }
            Some(filesystem::Action::Instance(scene)) => {
                if let Some(document) = self.documents.get_mut(self.current) {
                    let parent = document.selected.clone().unwrap_or_default();
                    let result = add_instance(document, &parent, &scene, [0.0, 0.0]);
                    document.end_change();
                    if let Err(err) = result {
                        self.output.push(Level::Error, err);
                    }
                }
            }
            Some(filesystem::Action::Refresh) => {
                if let Some(project) = &mut self.project {
                    project.rescan();
                }
            }
            None => {}
        }

        egui::Panel::right("inspector dock")
            .resizable(true)
            .default_size(320.0)
            .show(ui, |ui| {
                ui.strong("Inspector");
                ui.separator();
                let project = self.project.as_ref().expect("a game is open");
                let images = project.files_ending(".png");
                let tilesets = project.files_ending(".tileset.luau");
                let scenes = project.files_ending(".scene.luau");
                let Some(document) = self.documents.get_mut(self.current) else {
                    ui.weak("Open a scene to see its nodes' fields.");
                    return;
                };
                let mut services = Services {
                    ctx,
                    cache: &mut self.cache,
                    folder: &project.folder,
                    images: &images,
                    tilesets: &tilesets,
                    scenes: &scenes,
                };
                self.inspector.ui(ui, document, &mut services);
            });

        // Selecting a tile layer shows the tiles to paint it with, as Godot does.
        let selected = self.documents.get(self.current).and_then(|document| {
            let path = document
                .selected
                .clone()
                .filter(|_| document.inner().is_none())?;
            Some((document.path.clone(), path))
        });
        if selected != self.last_selected {
            if let Some((_, path)) = &selected
                && let Some(document) = self.documents.get(self.current)
                && let Some(node) = edit::node(document.file(), path)
                && matches!(node.kind, KindDef::TileLayer(_))
            {
                self.bottom_tab = BottomTab::Tiles;
                self.bottom_open = true;
            }
            self.last_selected = selected.clone();
        }
        // The tileset of the selected tilemap, or of the map of the selected layer.
        let tileset = self.documents.get(self.current).and_then(|document| {
            let (_, path) = selected.as_ref()?;
            let (_, map) = edit::tilemap_at(document.file(), path)?;
            Some(map.tileset.clone())
        });
        let palette = tileset.map(|tileset| Palette::load(ctx, &mut self.cache, &folder, &tileset));

        egui::Panel::bottom("bottom tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                for (tab, name) in [(BottomTab::Output, "Output"), (BottomTab::Tiles, "Tiles")] {
                    let shown = self.bottom_open && self.bottom_tab == tab;
                    if ui.selectable_label(shown, name).clicked() {
                        self.bottom_open = !shown;
                        self.bottom_tab = tab;
                    }
                }
                ui.add_enabled(false, egui::Button::selectable(false, "Animation"))
                    .on_disabled_hover_text("Editing clips comes in a later part of the milestone");
            });
        });
        let mut select_layer = None;
        egui::Panel::bottom("bottom panel")
            .resizable(true)
            .default_size(180.0)
            .show_collapsible(ui, &mut self.bottom_open, |ui| {
                match self.bottom_tab {
                    BottomTab::Output => self.output.ui(ui),
                    BottomTab::Tiles => {
                        let document = self.documents.get(self.current);
                        let target = tiles_target(document, selected.as_ref(), palette.as_ref());
                        select_layer = self.tiles.ui(ui, target);
                    }
                }
                // Its size stays put whatever it shows.
                ui.expand_to_include_rect(ui.max_rect());
            });
        if let Some(path) = select_layer
            && let Some(document) = self.documents.get_mut(self.current)
        {
            document.select(Some(path));
        }

        let mut viewport_action = None;
        egui::CentralPanel::default().show(ui, |ui| {
            self.tabs(ui);
            let document = self.documents.get_mut(self.current);
            self.viewport.toolbar(ui, document, canvas_size);
            ui.separator();
            match self.documents.get_mut(self.current) {
                Some(document) => {
                    let palette = palette.as_ref().and_then(|palette| palette.as_ref().ok());
                    viewport_action =
                        self.viewport
                            .ui(ui, ctx, document, canvas_size, &mut self.tiles, palette);
                }
                None => {
                    ui.centered_and_justified(|ui| {
                        ui.weak("Double-click a scene in the FileSystem dock to open it.");
                    });
                }
            }
        });
        if let Some(viewport::Action::Instance { scene, at }) = viewport_action
            && let Some(document) = self.documents.get_mut(self.current)
        {
            let result = place_instance(document, &scene, at);
            document.end_change();
            if let Err(err) = result {
                self.output.push(Level::Error, err);
            }
        }

        self.open_dialog(ui);
        self.closing_dialog(ui, ctx);

        // A change ends when nothing's being dragged or typed.
        let busy =
            ui.input(|input| input.pointer.any_down()) || ui.ctx().egui_wants_keyboard_input();
        if !busy {
            for document in &mut self.documents {
                document.end_change();
            }
        }
    }

    fn draw(&mut self, canvas: &mut Canvas) {
        canvas.clear(wingine::graphics::Color::rgb8(0x1d, 0x22, 0x29));
    }

    fn close_requested(&mut self, _ctx: &mut Context) -> bool {
        if self.documents.iter().any(Document::is_dirty) {
            self.closing = Some(Closing::Window);
            return false;
        }
        true
    }
}
