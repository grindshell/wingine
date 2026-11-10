//! The editor: a wingine game whose interface is all egui, laid out as Godot's editor is. The
//! menus, the main screens and Play run along the top; the Scene dock sits over the FileSystem
//! dock on the left, the Inspector on the right, and Output, Tiles and Animation in a panel along
//! the bottom; in the middle, the 2D screen shows the open scenes as tabs over the viewport, and
//! the Script screen the open scripts.

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use wingine::{
    Context, Game, Waker,
    graphics::Canvas,
    gui::egui::{self, Key, KeyboardShortcut, Modifiers},
    math::Affine,
    scene::{KindDef, NodeDef, Placement3, SceneFile},
    watch::Watcher,
};

use crate::{
    docks::{
        animation::Animation,
        filesystem::{self, FileSystem},
        inspector::{Inspector, Services},
        output::{Level, Output},
        scene_tree::{self, Addable, SceneTree},
        tiles::{Palette, Target, Tiles},
    },
    document::{Document, Reloaded},
    edit::{self, NodePath},
    play::Play,
    project::{self, AssetCache, Project},
    script_editor::{self, ScriptEditor},
    theme,
    viewport::{self, Tool, Viewport},
};

/// How often the editor looks again for a game that has closed its output to end, in seconds.
const ENDING_CHECK_TIME: f32 = 0.05;

/// The longest the editor waits for the last of what a game that's ended printed to be read.
const LAST_OUTPUT_TIME: Duration = Duration::from_millis(500);

/// The big views in the middle of the editor, switched between at the top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainScreen {
    /// The viewport of the open scene.
    TwoD,
    /// The open scripts, edited.
    Script,
}

/// The panels of the bottom panel, switched between by the tabs along its bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BottomTab {
    Output,
    Tiles,
    /// The clips of the open scene, one on a timeline that poses the preview.
    Animation,
}

/// What's waiting for an answer about unsaved changes before it closes.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Closing {
    Tab(usize),
    Script(usize),
    Window,
    /// The game open, for the game in this folder.
    Project(PathBuf),
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
    animation: Animation,
    scripts: ScriptEditor,
    /// Seconds the last frame took, which a clip playing moves on by.
    dt: f32,
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
    /// The tiles of the tileset of the tilemap selected, by the tileset's path, kept until another
    /// is selected or the project's files change.
    palette: Option<(String, Rc<Result<Palette, String>>)>,
    /// The folder being chosen in the Open dialog, while it's open.
    opening: Option<String>,
    /// The folders in the folder the Open dialog shows, and in the examples folder the welcome
    /// screen offers, as [`subfolders`] finds them, read again when the folder changes.
    listed_folders: [Option<(PathBuf, Folders)>; 2],
    closing: Option<Closing>,
    themed: bool,
    /// Asks for frames when there's something new from another thread: the game played, the
    /// language server, or the project's folder.
    waker: Waker,
    /// What tells the editor files in the project's folder have changed.
    watcher: Option<Watcher>,
}

impl Editor {
    /// An editor of the game in `folder`, or of nothing yet, to open a game from the start. It
    /// redraws on demand, waking itself through `ctx` when there's something new to show.
    pub fn new(ctx: &Context, folder: Option<PathBuf>) -> Self {
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
            animation: Animation::default(),
            scripts: ScriptEditor::new(ctx.waker()),
            dt: 0.0,
            bottom_open: true,
            bottom_tab: BottomTab::Output,
            last_selected: None,
            play: None,
            runner: std::env::current_exe().unwrap_or_else(|_| "wingine-editor".into()),
            play_env: Vec::new(),
            cache: AssetCache::default(),
            palette: None,
            opening: None,
            listed_folders: [None, None],
            closing: None,
            themed: false,
            waker: ctx.waker(),
            watcher: None,
        };
        if let Some(folder) = folder
            && let Err(err) = editor.open_project(folder)
        {
            editor.output.push(Level::Error, err);
        }
        editor
    }

    /// Opens the game in `folder`, as the Open dialog does: asking first what to do about unsaved
    /// changes to the game open now, if it has any.
    pub fn open_folder(&mut self, folder: PathBuf) {
        if self.has_unsaved_changes() {
            self.closing = Some(Closing::Project(folder));
        } else if let Err(err) = self.open_project(folder) {
            self.output.push(Level::Error, err);
        }
    }

    /// Opens the game in `folder`, closing the one open now, if any, unsaved changes and all.
    pub fn open_project(&mut self, folder: impl Into<PathBuf>) -> Result<(), String> {
        let project = Project::open(folder)?;
        self.stop();
        self.documents.clear();
        self.current = 0;
        self.cache = AssetCache::default();
        self.palette = None;
        self.scripts.set_project(Some(project.folder.clone()));
        self.watcher = Some(project::watch(project.folder.clone(), self.waker.clone()));
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

    /// Opens the script at `path` in the project in the Script screen, and shows it.
    pub fn open_script(&mut self, path: &str) -> Result<(), String> {
        self.scripts.open(path)?;
        self.screen = MainScreen::Script;
        Ok(())
    }

    pub fn project(&self) -> Option<&Project> {
        self.project.as_ref()
    }

    pub fn screen(&self) -> MainScreen {
        self.screen
    }

    pub fn set_screen(&mut self, screen: MainScreen) {
        self.screen = screen;
    }

    /// The Script screen, and the scripts open in it.
    pub fn scripts(&self) -> &ScriptEditor {
        &self.scripts
    }

    pub fn scripts_mut(&mut self) -> &mut ScriptEditor {
        &mut self.scripts
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

    /// The Animation panel's playing and snapping.
    pub fn animation(&self) -> &Animation {
        &self.animation
    }

    /// Whether the Animation panel shows, so its clip poses the open scene.
    fn animating(&self) -> bool {
        self.bottom_open && self.bottom_tab == BottomTab::Animation
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

    /// Whether any open scene or script has changes that aren't saved.
    pub fn has_unsaved_changes(&self) -> bool {
        self.documents.iter().any(Document::is_dirty) || self.scripts.is_dirty()
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

    /// Saves the script shown, in the Script screen, or the scene shown.
    pub fn save_shown(&mut self) {
        if self.screen == MainScreen::Script {
            let (level, message) = self.scripts.save_current();
            self.output.push(level, message);
        } else {
            self.save();
        }
    }

    /// Saves every scene and script with unsaved changes.
    pub fn save_all(&mut self) {
        let current = self.current;
        for i in 0..self.documents.len() {
            if self.documents[i].is_dirty() {
                self.current = i;
                self.save();
            }
        }
        self.current = current;
        for (level, message) in self.scripts.save_all() {
            self.output.push(level, message);
        }
    }

    /// Plays the game, saving every scene and script first, as Godot does.
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
        match Play::start(&self.runner, &project.folder, env, self.waker.clone()) {
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
            self.take_last_play_output(&mut play);
            self.output.push(Level::Info, "Stopped the game.");
        }
    }

    fn take_play_output(&mut self, play: &mut Play) {
        for line in play.lines() {
            self.output.push_game_line(line);
        }
    }

    /// Takes what a game that's ended printed, the last of it still being read too.
    fn take_last_play_output(&mut self, play: &mut Play) {
        for line in play.last_lines(LAST_OUTPUT_TIME) {
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

    /// Closes the script at `index`, asking first if it has unsaved changes, unless `force`.
    fn close_script(&mut self, index: usize, force: bool) {
        let dirty = self
            .scripts
            .scripts()
            .get(index)
            .map(|script| script.is_dirty());
        match dirty {
            Some(true) if !force => self.closing = Some(Closing::Script(index)),
            Some(_) => self.scripts.close(index),
            None => {}
        }
    }

    /// The open game's folder, which its scenes' files are in, or none without one.
    fn project_folder(&self) -> PathBuf {
        self.project
            .as_ref()
            .map(|project| project.folder.clone())
            .unwrap_or_default()
    }

    /// Does what the Scene dock asks.
    fn apply(&mut self, action: scene_tree::Action) {
        let listed = self.project.as_ref().map(Project::listed);
        let tileset = listed
            .and_then(|listed| listed.tilesets.first().cloned())
            .unwrap_or_default();
        let model = listed
            .and_then(|listed| listed.models.first().cloned())
            .unwrap_or_default();
        let folder = self.project_folder();
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
                let showing = edit::Showing {
                    sheet: &sheet,
                    tileset: &tileset,
                    model: &model,
                    ..edit::Showing::default()
                };
                let node = edit::new_node(kind, showing);
                edit::add(document.edit(), &parent, node).map(|path| {
                    document.select(Some(path));
                })
            }
            scene_tree::Action::Instance { parent, scene } => {
                add_instance(document, &folder, &parent, &scene, [0.0, 0.0])
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
            self.save_shown();
        }
        if pressed(ui, Modifiers::NONE, Key::F5) {
            self.play();
        }
        if pressed(ui, Modifiers::NONE, Key::F8) {
            self.stop();
        }
        // The rest are the 2D screen's.
        if typing || self.screen == MainScreen::Script {
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
        // Over the timeline, Delete takes out the selected key, not the node.
        if self.animating()
            && self.animation.wants_delete()
            && pressed(ui, Modifiers::NONE, Key::Delete)
            && let Some(document) = self.documents.get_mut(self.current)
        {
            self.animation.delete_selected(document);
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
        middle.selectable_value(&mut self.screen, MainScreen::Script, "📜 Script");
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
                    ui.label(
                        "The Animation panel poses the scene by a clip: the Inspector's ♦ keys \
                         a property.",
                    );
                    ui.label(
                        "Double-click a script to edit it in the Script screen, checked by \
                         luau-lsp if it's installed.",
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
        // What's in the folders may have changed since the dialog last showed them.
        self.listed_folders[OPENING] = None;
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
            let listed = &mut self.listed_folders[OPENING];
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for (name, folder, is_game) in
                        listed_subfolders(listed, Path::new(path.as_str()))
                    {
                        let is_game = *is_game;
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
            self.open_folder(folder);
        } else if cancel {
            self.opening = None;
        }
    }

    /// Asks what to do about unsaved changes, for a tab, the window or the game closing.
    fn closing_dialog(&mut self, ui: &mut egui::Ui, ctx: &mut Context) {
        let Some(closing) = self.closing.clone() else {
            return;
        };
        let names: Vec<String> = match closing {
            Closing::Tab(i) => self
                .documents
                .get(i)
                .map(|doc| doc.name().to_owned())
                .into_iter()
                .collect(),
            Closing::Script(i) => self
                .scripts
                .scripts()
                .get(i)
                .map(|script| script.name().to_owned())
                .into_iter()
                .collect(),
            Closing::Window | Closing::Project(_) => {
                let scenes = self.documents.iter().filter(|doc| doc.is_dirty());
                let scripts = self.scripts.scripts().iter().filter(|s| s.is_dirty());
                scenes
                    .map(|doc| doc.name().to_owned())
                    .chain(scripts.map(|script| script.name().to_owned()))
                    .collect()
            }
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
        // What fails to save stays open, changes and all, and Output says why.
        if save && !self.save_closing(&closing) {
            return;
        }
        match closing {
            Closing::Tab(i) => self.close_tab(i, true),
            Closing::Script(i) => self.close_script(i, true),
            Closing::Window => ctx.quit(),
            Closing::Project(folder) => {
                if let Err(err) = self.open_project(folder) {
                    self.output.push(Level::Error, err);
                }
            }
        }
    }

    /// Saves what `closing` closes, and tells whether all of it saved.
    fn save_closing(&mut self, closing: &Closing) -> bool {
        match *closing {
            Closing::Tab(i) => {
                let current = self.current;
                self.current = i;
                self.save();
                self.current = current;
                !self.documents.get(i).is_some_and(Document::is_dirty)
            }
            Closing::Script(i) => {
                let (level, message) = self.scripts.save(i);
                self.output.push(level, message);
                !self.scripts.scripts().get(i).is_some_and(|s| s.is_dirty())
            }
            Closing::Window | Closing::Project(_) => {
                self.save_all();
                !self.has_unsaved_changes()
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
                ui.label("Open a game's folder to edit its scenes and scripts.");
                if ui.button("Open Folder…").clicked() {
                    self.start_opening();
                }
                let examples =
                    listed_subfolders(&mut self.listed_folders[EXAMPLES], Path::new("examples"));
                let mut games = examples
                    .iter()
                    .filter(|(_, _, is_game)| *is_game)
                    .peekable();
                let mut chosen = None;
                if games.peek().is_some() {
                    ui.add_space(12.0);
                    ui.label("Or one of the examples:");
                    for (name, folder, _) in games {
                        if ui.button(format!("🎮 {name}")).clicked() {
                            chosen = Some(folder.clone());
                        }
                    }
                }
                if let Some(folder) = chosen {
                    self.open_folder(folder);
                }
            });
        });
    }
}

/// Fails unless the scene file at `scene`, read from the game's `folder`, can be placed inside the
/// node at `parent` of `document`: see [`edit::check_placed`]. One that can't be read isn't
/// checked, since building the preview says what's wrong with it.
fn check_instance(
    document: &Document,
    folder: &Path,
    parent: &[usize],
    scene: &str,
) -> Result<(), String> {
    if scene == document.path {
        return Err("a scene can't be placed in itself".into());
    }
    let mut load = |path: &str| SceneFile::read(folder.join(path)).ok().map(Rc::new);
    let Some(file) = load(scene) else {
        return Ok(());
    };
    let in_3d = edit::holds_3d(document.file(), parent);
    edit::check_placed(&file, in_3d, &mut load).map_err(|why| {
        let place = if in_3d {
            "in a 3D view"
        } else {
            "in the 2D scene"
        };
        format!("`{scene}` can't be placed {place}: {why}")
    })
}

/// Places the scene file at `scene` of the game in `folder` in `document`, as the last child of
/// `parent`, at `at` in the parent's coordinates.
fn add_instance(
    document: &mut Document,
    folder: &Path,
    parent: &[usize],
    scene: &str,
    [x, y]: [f32; 2],
) -> Result<(), String> {
    check_instance(document, folder, parent, scene)?;
    let showing = edit::Showing {
        scene,
        ..edit::Showing::default()
    };
    let mut node = edit::new_node("instance", showing);
    node.x = x;
    node.y = y;
    let path = edit::add(document.edit(), parent, node)?;
    document.select(Some(path));
    Ok(())
}

/// Adds `node` to the 3D view at `view` in `document`, at `at` in its world, placed in 3D, and
/// selects it.
fn add_3d(
    document: &mut Document,
    view: &[usize],
    mut node: NodeDef,
    [x, y, z]: [f32; 3],
) -> Result<(), String> {
    node.x = x;
    node.y = y;
    node.placement3 = Some(Placement3 {
        z,
        ..Placement3::default()
    });
    let path = edit::add(document.edit(), view, node)?;
    document.select(Some(path));
    Ok(())
}

/// Places the scene file at `scene` of the game in `folder` in `document` at `at`, a point of the
/// scene, in the selected node, or the tilemap of a selected layer, or the scene itself if nothing
/// is selected.
fn place_instance(
    document: &mut Document,
    folder: &Path,
    scene: &str,
    at: [f32; 2],
) -> Result<(), String> {
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
    add_instance(document, folder, &parent, scene, to_parent.apply(at))
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

/// Which of [`Editor::listed_folders`] the Open dialog's are.
const OPENING: usize = 0;
/// Which of [`Editor::listed_folders`] the examples are.
const EXAMPLES: usize = 1;

/// Folders by name, with their paths and whether each holds a game.
type Folders = Vec<(String, PathBuf, bool)>;

/// The folders in `folder`, as [`subfolders`] finds them, kept in `listed` with the folder they're
/// in until another folder's are asked for.
fn listed_subfolders<'a>(listed: &'a mut Option<(PathBuf, Folders)>, folder: &Path) -> &'a Folders {
    if listed.as_ref().is_none_or(|(of, _)| of != folder) {
        *listed = Some((folder.to_owned(), subfolders(folder)));
    }
    &listed.as_ref().expect("the folders were just listed").1
}

/// The folders in `folder`, by name.
fn subfolders(folder: &Path) -> Folders {
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
    fn update(&mut self, ctx: &mut Context, dt: f32) {
        self.dt = dt;
        if let Some(mut play) = self.play.take() {
            self.take_play_output(&mut play);
            match play.exited() {
                Some(status) => {
                    self.take_last_play_output(&mut play);
                    let message = match status.code() {
                        Some(0) => "The game ended.".to_owned(),
                        Some(code) => format!("The game ended with code {code}."),
                        None => "The game was ended.".to_owned(),
                    };
                    self.output.push(Level::Info, message);
                }
                None => {
                    // A game closes its output as it ends, which wakes the editor, but may not
                    // quite have ended yet.
                    if play.is_closed() {
                        ctx.request_redraw_after(ENDING_CHECK_TIME);
                    }
                    self.play = Some(play);
                }
            }
        }
        if self.watcher.as_ref().is_some_and(Watcher::take_changed)
            && let Some(project) = &mut self.project
        {
            project.rescan();
            self.palette = None;
            // The scenes themselves may have changed, images and files they use, and scripts.
            for document in &mut self.documents {
                let reloaded = document.reload_if_changed(&project.folder);
                let path = &document.path;
                match reloaded {
                    Some(Reloaded::Read) => self.output.push(
                        Level::Info,
                        format!("Read {path} again: it changed on disk."),
                    ),
                    Some(Reloaded::Kept) => self.output.push(
                        Level::Warning,
                        format!(
                            "{path} changed on disk, but has unsaved changes here: saving it \
                             will replace the file."
                        ),
                    ),
                    Some(Reloaded::Failed(err)) => self
                        .output
                        .push(Level::Warning, format!("Couldn't read {path} again: {err}")),
                    None => {}
                }
                document.rebuild();
            }
            self.scripts.rescan();
        }
    }

    fn gui(&mut self, ctx: &mut Context, ui: &mut egui::Ui) {
        if !self.themed {
            theme::apply(ui.ctx());
            self.themed = true;
        }
        self.output.take_logged();
        for (level, message) in self.scripts.take_messages() {
            self.output.push(level, message);
        }
        self.shortcuts(ui);

        egui::Panel::top("top bar").show(ui, |ui| self.top_bar(ui, ctx));
        if self.project.is_none() {
            self.welcome(ui);
            self.open_dialog(ui);
            return;
        }

        let project = self.project.as_ref().expect("a game is open");
        let game_size = project.canvas_size();
        let canvas_size = self.viewport.canvas_size(game_size);
        let folder = project.folder.clone();
        // Only the scene shown is posed, while the Animation panel shows.
        let animating = self.animating();
        for (i, document) in self.documents.iter_mut().enumerate() {
            document.set_posing(animating && i == self.current);
        }
        if let Some(document) = self.documents.get_mut(self.current)
            && let Some(error) = document.refresh(ctx, &mut self.cache, &folder, canvas_size)
        {
            self.output.push(Level::Error, error);
        }
        // Particles move while what's selected holds them, and keep the editor drawing.
        if let Some(document) = self.documents.get_mut(self.current)
            && document.animate_particles(self.dt)
        {
            ctx.request_redraw();
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
                        file_action = self.files.ui(ui, &project.name, project.files());
                    });
                egui::CentralPanel::default().show(ui, |ui| {
                    let Some(document) = self.documents.get(self.current) else {
                        ui.strong("Scene");
                        ui.weak("Double-click a scene in the FileSystem dock to open it.");
                        return;
                    };
                    let listed = self.project.as_ref().expect("a game is open").listed();
                    let addable = Addable {
                        sheets: !document.file().sheets.is_empty(),
                        tilesets: !listed.tilesets.is_empty(),
                        models: !listed.models.is_empty(),
                        scenes: &listed.scenes,
                    };
                    tree_actions = self.tree.ui(ui, document, &addable);
                });
            });
        for action in tree_actions {
            self.apply(action);
        }
        match file_action {
            Some(filesystem::Action::Open(path)) => match self.open_scene(&path) {
                Ok(()) => self.screen = MainScreen::TwoD,
                Err(err) => self.output.push(Level::Error, err),
            },
            Some(filesystem::Action::OpenScript(path)) => {
                if let Err(err) = self.open_script(&path) {
                    self.output.push(Level::Error, err);
                }
            }
            Some(filesystem::Action::Instance(scene)) => {
                let folder = self.project_folder();
                if let Some(document) = self.documents.get_mut(self.current) {
                    let parent = document.selected.clone().unwrap_or_default();
                    let result = add_instance(document, &folder, &parent, &scene, [0.0, 0.0]);
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
                self.palette = None;
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
                let listed = project.listed();
                let Some(document) = self.documents.get_mut(self.current) else {
                    ui.weak("Open a scene to see its nodes' fields.");
                    return;
                };
                let mut services = Services {
                    ctx,
                    cache: &mut self.cache,
                    folder: &project.folder,
                    images: &listed.images,
                    tilesets: &listed.tilesets,
                    scenes: &listed.scenes,
                    models: &listed.models,
                    fonts: &listed.fonts,
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
        match tileset {
            Some(tileset) if self.palette.as_ref().is_none_or(|(of, _)| *of != tileset) => {
                let palette = Palette::load(ctx, &mut self.cache, &folder, &tileset);
                self.palette = Some((tileset, Rc::new(palette)));
            }
            None => self.palette = None,
            Some(_) => {}
        }
        let palette = self.palette.as_ref().map(|(_, palette)| Rc::clone(palette));

        egui::Panel::bottom("bottom tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                for (tab, name) in [
                    (BottomTab::Output, "Output"),
                    (BottomTab::Tiles, "Tiles"),
                    (BottomTab::Animation, "Animation"),
                ] {
                    let shown = self.bottom_open && self.bottom_tab == tab;
                    if ui.selectable_label(shown, name).clicked() {
                        self.bottom_open = !shown;
                        self.bottom_tab = tab;
                    }
                }
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
                        let target = tiles_target(document, selected.as_ref(), palette.as_deref());
                        select_layer = self.tiles.ui(ui, target);
                    }
                    BottomTab::Animation => {
                        let document = self.documents.get_mut(self.current);
                        self.animation.ui(ui, document, self.dt);
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
        let mut script_action = None;
        if self.screen == MainScreen::Script {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| script_action = self.scripts.ui(ui));
        } else {
            egui::CentralPanel::default().show(ui, |ui| {
                self.tabs(ui);
                let document = self.documents.get_mut(self.current);
                self.viewport.toolbar(ui, document, game_size);
                ui.separator();
                match self.documents.get_mut(self.current) {
                    Some(document) => {
                        let palette = palette.as_deref().and_then(|palette| palette.as_ref().ok());
                        viewport_action = self.viewport.ui(
                            ui,
                            ctx,
                            document,
                            canvas_size,
                            &mut self.tiles,
                            palette,
                        );
                    }
                    None => {
                        ui.centered_and_justified(|ui| {
                            ui.weak("Double-click a scene in the FileSystem dock to open it.");
                        });
                    }
                }
            });
        }
        match script_action {
            Some(script_editor::Action::Close(i)) => self.close_script(i, false),
            Some(script_editor::Action::Save) => self.save_shown(),
            Some(script_editor::Action::Rescan) => {
                if let Some(project) = &mut self.project {
                    project.rescan();
                }
            }
            None => {}
        }
        let folder = self.project_folder();
        if let Some(action) = viewport_action
            && let Some(document) = self.documents.get_mut(self.current)
        {
            let result = match action {
                viewport::Action::Instance { scene, at } => {
                    place_instance(document, &folder, &scene, at)
                }
                viewport::Action::Instance3d { scene, view, at } => {
                    let showing = edit::Showing {
                        scene: &scene,
                        ..edit::Showing::default()
                    };
                    check_instance(document, &folder, &view, &scene).and_then(|()| {
                        add_3d(document, &view, edit::new_node("instance", showing), at)
                    })
                }
                viewport::Action::Model { model, view, at } => {
                    let showing = edit::Showing {
                        model: &model,
                        ..edit::Showing::default()
                    };
                    add_3d(document, &view, edit::new_node("model", showing), at)
                }
            };
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
        if self.has_unsaved_changes() {
            self.closing = Some(Closing::Window);
            return false;
        }
        true
    }
}
