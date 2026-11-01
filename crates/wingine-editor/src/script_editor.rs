//! The Script screen, where the game's scripts are edited, as Godot's script editor edits
//! GDScript: the open scripts listed on the left, the one shown in the middle, colored by what
//! its words are (`luau.rs`) with line numbers down its side, and along the bottom its problems,
//! where the cursor is, and how the language server is doing.
//!
//! With luau-lsp installed (`lsp.rs`), scripts are checked against the engine's API as they're
//! typed: problems are underlined, resting the pointer on a name shows its type and docs, typing a
//! name or a `.` offers what could come next, and typing a call shows its parameters. Without it,
//! scripts are edited all the same.

use std::{
    hash::{DefaultHasher, Hash, Hasher},
    io,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

use wingine::{
    Waker,
    gui::egui::{
        self, Color32, FontId, Key, Modifiers, Stroke,
        text::{CCursor, CCursorRange, LayoutJob, TextFormat},
    },
};

use crate::{
    docks::output::Level,
    lsp::{self, Completion, Diagnostic, Event, LanguageServer, RequestId, Severity, Signature},
    luau::{self, Kind, Token},
    script::Script,
    theme,
};

/// The spaces a level of indentation is.
const INDENT: usize = 4;
/// Seconds the pointer rests on a name before what it is shows.
const HOVER_DELAY: f64 = 0.3;
/// The most completions the list shows at once.
const COMPLETIONS_SHOWN: usize = 10;

/// What the screen asks of the editor.
pub enum Action {
    /// Close the script at this index, asking first if it has unsaved changes.
    Close(usize),
    /// Save the script shown.
    Save,
    /// Read the project's folder again: a script was made.
    Rescan,
}

/// How the language server is doing.
enum Server {
    /// Not started: it starts when a script is first opened.
    Idle,
    Running(LanguageServer),
    /// It couldn't start, or it stopped, and why.
    Failed(String),
    /// There's none to run.
    Off,
}

/// The completions offered at the cursor.
struct CompletionList {
    /// The request for them, until it's answered.
    request: Option<RequestId>,
    items: Vec<Completion>,
    /// Where the word being completed starts, in characters.
    start: usize,
    /// Which of those shown is picked.
    selected: usize,
}

/// The call the cursor is in.
struct SignatureHelp {
    request: Option<RequestId>,
    signature: Option<Signature>,
}

/// What the pointer rests on, and what the server says it is.
#[derive(Default)]
struct Hover {
    /// The bytes of the word under the pointer, and since when it's been there.
    target: Option<(Range<usize>, f64)>,
    request: Option<RequestId>,
    contents: Option<String>,
}

pub struct ScriptEditor {
    folder: Option<PathBuf>,
    scripts: Vec<Script>,
    /// The index of the script shown, if there are any.
    current: usize,
    /// The language server to run, or none.
    program: Option<PathBuf>,
    server: Server,
    /// What to tell the Output panel.
    messages: Vec<(Level, String)>,
    completion: Option<CompletionList>,
    signature: Option<SignatureHelp>,
    hover: Hover,
    /// Where the cursor is in the script shown, in characters.
    cursor: usize,
    /// A character of the script shown to move the cursor to and show.
    jump: Option<usize>,
    problems_open: bool,
    /// The tokens of a script's text, by a hash of the text.
    tokens: Option<(u64, Arc<Vec<Token>>)>,
    /// The path in the New Script dialog, while it's open.
    new_script: Option<String>,
    /// How the script shown was laid out last frame, and where.
    shown: Option<(Arc<egui::Galley>, egui::Pos2)>,
    /// Wakes the editor when the language server says something.
    waker: Waker,
}

impl ScriptEditor {
    /// The screen, with no game open. `waker` wakes the editor when the language server says
    /// something.
    pub fn new(waker: Waker) -> Self {
        Self {
            folder: None,
            scripts: Vec::new(),
            current: 0,
            program: Some(lsp::program()),
            server: Server::Idle,
            messages: Vec::new(),
            completion: None,
            signature: None,
            hover: Hover::default(),
            cursor: 0,
            jump: None,
            problems_open: true,
            tokens: None,
            new_script: None,
            shown: None,
            waker,
        }
    }

    /// Edits the scripts of the game in `folder`, closing those open.
    pub fn set_project(&mut self, folder: Option<PathBuf>) {
        self.folder = folder;
        self.scripts.clear();
        self.current = 0;
        self.restart();
    }

    /// Runs `program` as the language server, or none.
    pub fn set_program(&mut self, program: Option<PathBuf>) {
        self.program = program;
        self.restart();
    }

    /// Stops the language server, which starts again for the next script opened.
    fn restart(&mut self) {
        self.server = if self.program.is_some() {
            Server::Idle
        } else {
            Server::Off
        };
        for script in &mut self.scripts {
            script.synced = None;
            script.diagnostics.clear();
        }
        self.close_popups();
    }

    fn close_popups(&mut self) {
        self.completion = None;
        self.signature = None;
        self.hover = Hover::default();
    }

    /// Opens the script at `path` in the project, or shows it if it's open.
    pub fn open(&mut self, path: &str) -> Result<(), String> {
        if let Some(i) = self.scripts.iter().position(|script| script.path == path) {
            self.show(i);
            return Ok(());
        }
        let folder = self.folder.as_ref().ok_or("no game is open")?;
        let script = Script::open(folder, path).map_err(|err| format!("{path}: {err}"))?;
        self.scripts.push(script);
        self.show(self.scripts.len() - 1);
        Ok(())
    }

    fn show(&mut self, index: usize) {
        if index != self.current {
            self.close_popups();
        }
        self.current = index;
    }

    pub fn scripts(&self) -> &[Script] {
        &self.scripts
    }

    /// The script shown.
    pub fn script(&self) -> Option<&Script> {
        self.scripts.get(self.current)
    }

    pub fn is_dirty(&self) -> bool {
        self.scripts.iter().any(Script::is_dirty)
    }

    /// Saves the script at `index`, saying how it went.
    pub fn save(&mut self, index: usize) -> (Level, String) {
        let (Some(folder), Some(script)) = (&self.folder, self.scripts.get_mut(index)) else {
            return (Level::Error, "there's no script to save".into());
        };
        match script.save(folder) {
            Ok(()) => (Level::Info, format!("Saved {}", script.path)),
            Err(err) => (Level::Error, format!("{}: {err}", script.path)),
        }
    }

    pub fn save_current(&mut self) -> (Level, String) {
        self.save(self.current)
    }

    /// Saves every script with unsaved changes, saying how each went.
    pub fn save_all(&mut self) -> Vec<(Level, String)> {
        let dirty: Vec<usize> = (0..self.scripts.len())
            .filter(|&i| self.scripts[i].is_dirty())
            .collect();
        dirty.into_iter().map(|i| self.save(i)).collect()
    }

    /// Closes the script at `index`, unsaved changes and all.
    pub fn close(&mut self, index: usize) {
        if index >= self.scripts.len() {
            return;
        }
        let script = self.scripts.remove(index);
        if let Server::Running(server) = &mut self.server
            && script.synced.is_some()
        {
            server.close(&script.uri);
        }
        if self.current > index || self.current == self.scripts.len() {
            self.current = self.current.saturating_sub(1);
        }
        self.close_popups();
    }

    /// Reads scripts again whose files have changed, unless they have changes of their own.
    pub fn rescan(&mut self) {
        let Some(folder) = &self.folder else { return };
        for script in &mut self.scripts {
            script.reload_if_changed(folder);
        }
    }

    /// What to tell the Output panel since this was last called.
    pub fn take_messages(&mut self) -> Vec<(Level, String)> {
        std::mem::take(&mut self.messages)
    }

    /// Whether the language server is running and answering.
    pub fn is_checking(&self) -> bool {
        matches!(&self.server, Server::Running(server) if server.is_ready())
    }

    /// Where the cursor is in the script shown, in characters.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Moves the cursor to character `index` of the script shown, and shows it.
    pub fn go_to(&mut self, index: usize) {
        self.jump = Some(index);
    }

    /// The middle of character `index` of the script shown, where it was drawn last frame, in
    /// points: for tests, to point at it.
    pub fn screen_pos(&self, index: usize) -> Option<[f32; 2]> {
        let (galley, pos) = self.shown.as_ref()?;
        let left = galley.pos_from_cursor(CCursor::new(index));
        let right = galley.pos_from_cursor(CCursor::new(index + 1));
        let middle = pos.to_vec2() + left.union(right).center().to_vec2();
        Some([middle.x, middle.y])
    }

    /// The labels of the completions offered, in order.
    pub fn completions(&self) -> Vec<&str> {
        let Some((list, script)) = self.completion.as_ref().zip(self.script()) else {
            return Vec::new();
        };
        shown_completions(list, &script.text, self.cursor)
            .into_iter()
            .map(|item| item.label.as_str())
            .collect()
    }

    /// What the server said the name under the pointer is.
    pub fn hover_text(&self) -> Option<&str> {
        self.hover.contents.as_deref()
    }

    /// The call the cursor is in, as the server wrote it.
    pub fn signature_text(&self) -> Option<&str> {
        let signature = self.signature.as_ref()?.signature.as_ref()?;
        Some(&signature.label)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        self.poll();
        self.sync();
        let mut action = None;
        egui::Panel::top("script toolbar").show(ui, |ui| self.toolbar(ui, &mut action));
        egui::Panel::left("script list")
            .resizable(true)
            .default_size(180.0)
            .show(ui, |ui| self.list(ui, &mut action));
        egui::Panel::bottom("script status").show(ui, |ui| self.status(ui));
        let problems = self
            .script()
            .is_some_and(|script| !script.diagnostics.is_empty());
        if self.problems_open && problems {
            egui::Panel::bottom("script problems")
                .resizable(true)
                .default_size(90.0)
                .show(ui, |ui| self.problems(ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.code(ui));
        self.new_script_dialog(ui, &mut action);
        action
    }

    /// Takes in what the language server has said.
    fn poll(&mut self) {
        let events = match &mut self.server {
            Server::Running(server) => server.poll(),
            _ => return,
        };
        for event in events {
            match event {
                Event::Diagnostics { uri, diagnostics } => {
                    if let Some(script) = self
                        .scripts
                        .iter_mut()
                        .find(|script| lsp::same_file(&script.uri, &uri))
                    {
                        script.diagnostics = diagnostics;
                    }
                }
                Event::Hover { id, contents } => {
                    if self.hover.request == Some(id) {
                        self.hover.request = None;
                        self.hover.contents = contents;
                    }
                }
                Event::Completions { id, mut items } => {
                    if let Some(list) = &mut self.completion
                        && list.request == Some(id)
                    {
                        items.sort_by(|a, b| (&a.sort, &a.label).cmp(&(&b.sort, &b.label)));
                        list.request = None;
                        list.items = items;
                        list.selected = 0;
                    }
                }
                Event::Signature { id, signature } => {
                    if let Some(help) = &mut self.signature
                        && help.request == Some(id)
                    {
                        match signature {
                            Some(signature) => {
                                help.request = None;
                                help.signature = Some(signature);
                            }
                            None => self.signature = None,
                        }
                    }
                }
                Event::Stopped(why) => {
                    self.messages
                        .push((Level::Warning, format!("luau-lsp stopped: {why}")));
                    self.server = Server::Failed(why);
                    for script in &mut self.scripts {
                        script.synced = None;
                    }
                }
            }
        }
    }

    /// Starts the language server if it's time, and tells it what's changed in the scripts.
    fn sync(&mut self) {
        if matches!(self.server, Server::Idle) && !self.scripts.is_empty() {
            self.start_server();
        }
        let Server::Running(server) = &mut self.server else {
            return;
        };
        for script in &mut self.scripts {
            send_changes(server, script);
        }
    }

    fn start_server(&mut self) {
        let (Some(folder), Some(program)) = (&self.folder, &self.program) else {
            self.server = Server::Off;
            return;
        };
        match LanguageServer::start(program, folder, self.waker.clone()) {
            Ok(server) => self.server = Server::Running(server),
            Err(err) => {
                let why = if err.kind() == io::ErrorKind::NotFound {
                    format!(
                        "{} isn't installed. Install luau-lsp and put it on the PATH, or set \
                         WINGINE_LUAU_LSP to it, to check scripts as they're typed.",
                        program.display()
                    )
                } else {
                    format!("luau-lsp didn't start: {err}")
                };
                self.messages.push((Level::Warning, why.clone()));
                self.server = Server::Failed(why);
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        let has_script = !self.scripts.is_empty();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New Script…").clicked() {
                    self.new_script = Some("new_script.luau".into());
                }
                if ui
                    .add_enabled(
                        has_script,
                        egui::Button::new("Save").shortcut_text("Ctrl+S"),
                    )
                    .clicked()
                {
                    *action = Some(Action::Save);
                }
                if ui
                    .add_enabled(has_script, egui::Button::new("Close Script"))
                    .clicked()
                {
                    *action = Some(Action::Close(self.current));
                }
            });
            ui.menu_button("Edit", |ui| {
                ui.label("Ctrl+Space offers what could be typed.");
                ui.label("Tab and Shift+Tab indent and unindent lines.");
                ui.label("Ctrl+Z and Ctrl+Y undo and redo typing.");
            });
        });
    }

    fn list(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        ui.strong("Scripts");
        ui.separator();
        let mut show = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for (i, script) in self.scripts.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let title = if script.is_dirty() {
                            format!("{}*", script.name())
                        } else {
                            script.name().to_owned()
                        };
                        let errors = script
                            .diagnostics
                            .iter()
                            .any(|problem| problem.severity == Severity::Error);
                        let text = if errors {
                            egui::RichText::new(title).color(theme::ERROR)
                        } else {
                            egui::RichText::new(title)
                        };
                        if ui
                            .selectable_label(i == self.current, text)
                            .on_hover_text(&script.path)
                            .clicked()
                        {
                            show = Some(i);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("✖")
                                .on_hover_text("Close the script")
                                .clicked()
                            {
                                *action = Some(Action::Close(i));
                            }
                        });
                    });
                }
            });
        if let Some(i) = show {
            self.show(i);
        }
    }

    fn status(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(script) = self.scripts.get(self.current) {
                let count = |severity: fn(Severity) -> bool| {
                    script
                        .diagnostics
                        .iter()
                        .filter(|problem| severity(problem.severity))
                        .count()
                };
                let errors = count(|severity| severity == Severity::Error);
                let warnings = count(|severity| severity != Severity::Error);
                let text = egui::RichText::new(format!("⛔ {errors}  ⚠ {warnings}")).color(
                    if errors > 0 {
                        theme::ERROR
                    } else if warnings > 0 {
                        theme::WARNING
                    } else {
                        theme::CODE_TEXT
                    },
                );
                if ui
                    .selectable_label(self.problems_open, text)
                    .on_hover_text("Show or hide the script's problems")
                    .clicked()
                {
                    self.problems_open = !self.problems_open;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (status, hint) = match &self.server {
                    Server::Running(server) if server.is_ready() => {
                        ("luau-lsp ✔", "Checking scripts as they're typed".to_owned())
                    }
                    Server::Running(_) => ("luau-lsp starting…", String::new()),
                    Server::Idle => ("luau-lsp", "Starts when a script is opened".to_owned()),
                    Server::Off => ("Not checking", String::new()),
                    Server::Failed(why) => ("luau-lsp ✖", why.clone()),
                };
                let failed = matches!(self.server, Server::Failed(_));
                if failed
                    && ui
                        .small_button("⟳")
                        .on_hover_text("Start luau-lsp again")
                        .clicked()
                {
                    self.restart();
                }
                ui.weak(status).on_hover_text(hint);
                if let Some(script) = self.scripts.get(self.current) {
                    let at = lsp::position_of(&script.text, self.cursor);
                    ui.weak(format!("Line {}, Column {}", at.line + 1, at.character + 1));
                }
            });
        });
    }

    fn problems(&mut self, ui: &mut egui::Ui) {
        let Some(script) = self.scripts.get(self.current) else {
            return;
        };
        let mut jump = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for problem in &script.diagnostics {
                    let (icon, color) = match problem.severity {
                        Severity::Error => ("⛔", theme::ERROR),
                        _ => ("⚠", theme::WARNING),
                    };
                    let first_line = problem.message.lines().next().unwrap_or_default();
                    let text = format!(
                        "{icon} {}:{}  {first_line}",
                        problem.start.line + 1,
                        problem.start.character + 1
                    );
                    let label = egui::Label::new(egui::RichText::new(text).color(color))
                        .truncate()
                        .sense(egui::Sense::click());
                    if ui.add(label).on_hover_text(&problem.message).clicked() {
                        let byte = lsp::byte_at(&script.text, problem.start);
                        jump = Some(script.text[..byte].chars().count());
                    }
                }
            });
        if jump.is_some() {
            self.jump = jump;
        }
    }

    fn new_script_dialog(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        let (Some(path), Some(folder)) = (&mut self.new_script, self.folder.clone()) else {
            return;
        };
        let problem = new_script_problem(&folder, path);
        let mut create = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("new script dialog")).show(ui.ctx(), |ui| {
            ui.set_width(400.0);
            ui.heading("New Script");
            ui.label("Its path in the game's folder:");
            let field = ui.add(egui::TextEdit::singleline(path).desired_width(f32::INFINITY));
            if let Some(problem) = &problem {
                ui.colored_label(theme::ERROR, problem);
            }
            let entered = field.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(problem.is_none(), egui::Button::new("Create"))
                    .clicked()
                    || (entered && problem.is_none())
                {
                    create = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if create {
            let path = self.new_script.take().unwrap_or_default();
            let file = folder.join(&path);
            let made = file
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&file, ""));
            match made
                .map_err(|err| err.to_string())
                .and_then(|()| self.open(&path))
            {
                Ok(()) => *action = Some(Action::Rescan),
                Err(err) => self.messages.push((Level::Error, err)),
            }
        } else if cancel {
            self.new_script = None;
        }
    }

    /// The script shown, edited.
    fn code(&mut self, ui: &mut egui::Ui) {
        let full = ui.available_rect_before_wrap();
        ui.painter().rect_filled(full, 0.0, theme::CODE_BACKGROUND);
        let Some(script) = self.scripts.get_mut(self.current) else {
            ui.centered_and_justified(|ui| {
                ui.weak("Double-click a script in the FileSystem dock to edit it.");
            });
            return;
        };
        let Self {
            server,
            completion,
            signature,
            hover,
            jump,
            tokens: token_cache,
            ..
        } = self;
        let id = ui.make_persistent_id(("script", &script.path));
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        let focused = ui.memory(|memory| memory.has_focus(id));
        let popups_open = completion.is_some() || signature.is_some();

        // Keys the script editor takes before the text field sees them.
        let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
        let selection = state.cursor.char_range().map(|range| {
            let (a, b) = (range.primary.index.0, range.secondary.index.0);
            a.min(b)..a.max(b)
        });
        let mut moved_to = None;
        let mut edited = false;
        let jumped_to = jump
            .take()
            .map(|index| index.min(script.text.chars().count()));
        if let Some(index) = jumped_to {
            moved_to = Some(index);
            ui.memory_mut(|memory| memory.request_focus(id));
        } else if focused && let Some(selection) = selection.clone() {
            let pressed = |ui: &mut egui::Ui, modifiers, key| {
                ui.input_mut(|input| input.consume_key(modifiers, key))
            };
            if let Some(list) = completion {
                let shown: Vec<String> = shown_completions(list, &script.text, selection.end)
                    .into_iter()
                    .map(|item| item.insert.clone())
                    .collect();
                let count = shown.len();
                let start = list.start.min(selection.start);
                let word: String = script
                    .text
                    .chars()
                    .skip(start)
                    .take(selection.end - start)
                    .collect();
                let picked = shown.get(list.selected.min(count.max(1) - 1));
                // What's picked may be typed already, so Enter starts a new line.
                let typed_already = picked == Some(&word);
                if count > 0 && pressed(ui, Modifiers::NONE, Key::ArrowDown) {
                    list.selected = (list.selected + 1) % count;
                } else if count > 0 && pressed(ui, Modifiers::NONE, Key::ArrowUp) {
                    list.selected = (list.selected + count - 1) % count;
                } else if count > 0 && pressed(ui, Modifiers::NONE, Key::Escape) {
                    *completion = None;
                } else if let Some(insert) = picked.filter(|_| !typed_already)
                    && (pressed(ui, Modifiers::NONE, Key::Enter)
                        || pressed(ui, Modifiers::NONE, Key::Tab))
                {
                    moved_to = Some(replace(&mut script.text, start..selection.end, insert));
                    *completion = None;
                    edited = true;
                }
            }
            if !edited && pressed(ui, Modifiers::NONE, Key::Escape) {
                *signature = None;
            }
            if pressed(ui, Modifiers::COMMAND, Key::Space) {
                let start = word_start(&script.text, selection.end);
                if let Server::Running(server) = server {
                    let at = lsp::position_of(&script.text, selection.end);
                    *completion = Some(CompletionList {
                        request: server.complete(&script.uri, at, None),
                        items: Vec::new(),
                        start,
                        selected: 0,
                    });
                }
            }
            if !edited && pressed(ui, Modifiers::SHIFT, Key::Tab) {
                moved_to = Some(unindent(&mut script.text, selection.clone()));
                edited = true;
            } else if !edited && pressed(ui, Modifiers::NONE, Key::Tab) {
                moved_to = Some(indent(&mut script.text, selection.clone()));
                edited = true;
            }
        }
        // Enter is left to the text field, which puts the new line after anything typed before
        // it this frame, and the new line is indented after.
        let entered = focused && ui.input(|input| input.key_pressed(Key::Enter));
        if let Some(index) = moved_to {
            state
                .cursor
                .set_char_range(Some(CCursorRange::one(CCursor::new(index))));
            state.store(ui.ctx(), id);
        }

        // The text, colored, with its problems underlined.
        let typed: String = ui.input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect()
        });
        let diagnostics = &script.diagnostics;
        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap: f32| {
            let text = text.as_str();
            let tokens = cached_tokens(token_cache, text);
            let job = layout_job(text, &tokens, &underlines(text, diagnostics), &font);
            ui.fonts_mut(|fonts| fonts.layout_job(job))
        };
        let gutter = gutter_width(
            ui,
            &script.text,
            &egui::TextStyle::Monospace.resolve(ui.style()),
        );
        let current_line = ui.painter().add(egui::Shape::Noop);
        let scroll = egui::ScrollArea::both()
            .id_salt(("script scroll", &script.path))
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.add_space(gutter);
                    let output = egui::TextEdit::multiline(&mut script.text)
                        .id(id)
                        .code_editor()
                        // Escape closes what's open over the script, keeping the cursor in it.
                        .event_filter(egui::EventFilter {
                            tab: true,
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            escape: popups_open,
                        })
                        .frame(egui::Frame::NONE)
                        .margin(egui::Margin::symmetric(4, 2))
                        .desired_width(ui.available_width())
                        .min_size(ui.available_size())
                        .layouter(&mut layouter)
                        .show(ui);
                    // A jump shows where it lands at once. The field may not have focus yet to
                    // know where its cursor is.
                    if let Some(index) = jumped_to {
                        let rect = output.galley.pos_from_cursor(CCursor::new(index));
                        let rect = rect.translate(output.galley_pos.to_vec2());
                        ui.scroll_to_rect_animation(
                            rect,
                            Some(egui::Align::Center),
                            egui::style::ScrollAnimation::none(),
                        );
                    }
                    output
                })
                .inner
            });
        let output = scroll.inner;
        let viewport = scroll.inner_rect;
        let changed = edited || output.response.changed();
        let mut cursor = output
            .cursor_range
            .map_or(self.cursor, |range| range.primary.index.0);
        if entered && output.response.changed() && cursor > 0 {
            // The new line takes the indentation of the line before, and a level more in a block.
            let line = line_before(&script.text, cursor - 1);
            let mut indentation: String = line
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect();
            if opens_block(line) {
                indentation.push_str(&" ".repeat(INDENT));
            }
            cursor = replace(&mut script.text, cursor..cursor, &indentation);
            let mut state = output.state.clone();
            state
                .cursor
                .set_char_range(Some(CCursorRange::one(CCursor::new(cursor))));
            state.store(ui.ctx(), id);
            ui.ctx().request_repaint();
        }
        if changed {
            script.changed();
            hover.contents = None;
            hover.target = None;
        }
        self.cursor = cursor;
        self.shown = Some((Arc::clone(&output.galley), output.galley_pos));
        let Self {
            server,
            completion,
            signature,
            hover,
            tokens: token_cache,
            ..
        } = self;
        let Some(script) = self.scripts.get_mut(self.current) else {
            return;
        };
        let tokens = cached_tokens(token_cache, &script.text);
        let galley_pos = output.galley_pos;
        let cursor_rect = output
            .galley
            .pos_from_cursor(CCursor::new(cursor))
            .translate(galley_pos.to_vec2());

        // The line the cursor is on, and line numbers down the side, over what's scrolled under.
        let painter = ui.painter().with_clip_rect(viewport);
        if output.response.has_focus() {
            let line = egui::Rect::from_x_y_ranges(viewport.x_range(), cursor_rect.y_range());
            ui.painter().set(
                current_line,
                egui::Shape::rect_filled(line, 0.0, theme::CURRENT_LINE),
            );
        }
        let gutter_rect =
            egui::Rect::from_min_size(viewport.min, egui::vec2(gutter, viewport.height()));
        painter.rect_filled(gutter_rect, 0.0, theme::CODE_BACKGROUND);
        let problem_lines = problem_lines(&script.diagnostics);
        let mut line = 0;
        let mut starts_line = true;
        for row in &output.galley.rows {
            let y = galley_pos.y + row.pos.y;
            if starts_line && y + row.rect().height() >= viewport.top() && y <= viewport.bottom() {
                let color = match problem_lines.get(line) {
                    Some(Some(Severity::Error)) => theme::ERROR,
                    Some(Some(_)) => theme::WARNING,
                    _ if row
                        .rect()
                        .y_range()
                        .contains(cursor_rect.center().y - galley_pos.y) =>
                    {
                        theme::CODE_TEXT
                    }
                    _ => theme::LINE_NUMBER,
                };
                painter.text(
                    egui::pos2(gutter_rect.right() - 8.0, y),
                    egui::Align2::RIGHT_TOP,
                    line + 1,
                    font.clone(),
                    color,
                );
            }
            if row.ends_with_newline {
                line += 1;
                starts_line = true;
            } else {
                starts_line = false;
            }
        }

        let has_focus = output.response.has_focus();
        let Server::Running(server) = server else {
            // Hovering a problem still shows it.
            show_hover(
                ui,
                &output,
                hover,
                &script.text,
                &script.diagnostics,
                &tokens,
                None,
            );
            return;
        };

        // Completions and calls, as they're typed, asked about the text as it is now.
        if changed && has_focus {
            send_changes(server, script);
            let at = lsp::position_of(&script.text, cursor);
            let byte = byte_of(&script.text, cursor);
            // Typing in comments, strings and numbers isn't code.
            let in_words =
                luau::token_containing(&tokens, byte.saturating_sub(1)).is_some_and(|token| {
                    matches!(token.kind, Kind::Comment | Kind::String | Kind::Number)
                });
            match typed.chars().last() {
                Some(typed @ ('.' | ':')) if !in_words => {
                    *completion = Some(CompletionList {
                        request: server.complete(&script.uri, at, Some(typed)),
                        items: Vec::new(),
                        start: cursor,
                        selected: 0,
                    });
                }
                Some(c) if is_word(c) && !in_words => {
                    let start = word_start(&script.text, cursor);
                    // Asked again for another word, or if nothing was offered for this one.
                    let stale = completion.as_ref().is_none_or(|list| {
                        list.start != start || (list.request.is_none() && list.items.is_empty())
                    });
                    if stale {
                        *completion = Some(CompletionList {
                            request: server.complete(&script.uri, at, None),
                            items: Vec::new(),
                            start,
                            selected: 0,
                        });
                    }
                }
                Some(_) => *completion = None,
                None => {}
            }
            if matches!(typed.chars().last(), Some('(' | ',')) || signature.is_some() {
                let request = server.signature(&script.uri, at);
                match signature {
                    Some(help) => help.request = request,
                    None => {
                        *signature = Some(SignatureHelp {
                            request,
                            signature: None,
                        });
                    }
                }
            }
        }
        // The cursor leaving the word being completed ends completing it.
        if let Some(list) = completion
            && (cursor < list.start
                || !script
                    .text
                    .chars()
                    .skip(list.start)
                    .take(cursor - list.start)
                    .all(is_word)
                || !has_focus)
        {
            *completion = None;
        }
        if !has_focus {
            *signature = None;
        }

        show_hover(
            ui,
            &output,
            hover,
            &script.text,
            &script.diagnostics,
            &tokens,
            Some((server, &script.uri)),
        );
        let popup_id = id.with("popups");
        if let Some(help) = signature
            && let Some(shown) = &help.signature
        {
            show_signature(ui, popup_id.with("signature"), cursor_rect, shown, &font);
        }
        if let Some(list) = completion {
            let shown = shown_completions(list, &script.text, cursor);
            let selected = list.selected.min(shown.len().saturating_sub(1));
            let picked = if shown.is_empty() {
                None
            } else {
                show_completions(
                    ui,
                    popup_id.with("completions"),
                    cursor_rect,
                    &shown,
                    selected,
                    &font,
                )
            };
            let insert = picked.map(|i| shown[i].insert.clone());
            list.selected = selected;
            if let Some(insert) = insert {
                let index = replace(&mut script.text, list.start..cursor, &insert);
                script.changed();
                *completion = None;
                self.jump = Some(index);
            }
        }
    }
}

/// Tells `server` what's changed in `script` since it last heard, opening it if it hasn't.
fn send_changes(server: &mut LanguageServer, script: &mut Script) {
    match script.synced {
        None => server.open(&script.uri, script.version, &script.text),
        Some(version) if version != script.version => {
            server.change(&script.uri, script.version, &script.text);
        }
        Some(_) => return,
    }
    script.synced = Some(script.version);
}

/// What's wrong with a path for a new script, if anything.
fn new_script_problem(folder: &Path, path: &str) -> Option<String> {
    if !path.ends_with(".luau") {
        Some("Scripts' names end in .luau.".into())
    } else if path.starts_with('/') || path.split('/').any(|part| part.is_empty() || part == "..") {
        Some("Give a path inside the game's folder, such as enemies/bat.luau.".into())
    } else if folder.join(path).exists() {
        Some("There's a file there already.".into())
    } else {
        None
    }
}

fn cached_tokens(cache: &mut Option<(u64, Arc<Vec<Token>>)>, text: &str) -> Arc<Vec<Token>> {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    let hash = hasher.finish();
    if let Some((cached, tokens)) = cache
        && *cached == hash
    {
        return Arc::clone(tokens);
    }
    let tokens = Arc::new(luau::tokens(text));
    *cache = Some((hash, Arc::clone(&tokens)));
    tokens
}

/// The bytes of `text` each of `diagnostics` underlines, at least a character, in its color.
fn underlines(text: &str, diagnostics: &[Diagnostic]) -> Vec<(Range<usize>, Color32)> {
    diagnostics
        .iter()
        .map(|problem| {
            let mut start = lsp::byte_at(text, problem.start);
            let mut end = lsp::byte_at(text, problem.end).max(start);
            if start == end {
                match text[end..].chars().next() {
                    Some(c) if c != '\n' => end += c.len_utf8(),
                    _ => {
                        start -= text[..start].chars().next_back().map_or(0, char::len_utf8);
                    }
                }
            }
            let color = match problem.severity {
                Severity::Error => theme::ERROR,
                _ => theme::WARNING,
            };
            (start..end, color)
        })
        .collect()
}

/// How a script's text is shown: each token in its color, and each problem underlined.
fn layout_job(
    text: &str,
    tokens: &[Token],
    underlines: &[(Range<usize>, Color32)],
    font: &FontId,
) -> LayoutJob {
    let mut cuts = vec![0, text.len()];
    for token in tokens {
        cuts.extend([token.range.start, token.range.end]);
    }
    for (range, _) in underlines {
        cuts.extend([range.start, range.end]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    // Runs of text alike are one section: the run being gathered.
    let mut run: Option<(Range<usize>, TextFormat)> = None;
    let mut token = 0;
    for pair in cuts.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        while tokens.get(token).is_some_and(|t| t.range.end <= start) {
            token += 1;
        }
        let color = tokens
            .get(token)
            .filter(|t| t.range.start <= start)
            .map_or(theme::CODE_TEXT, |t| theme::code_color(t.kind));
        let underline = underlines
            .iter()
            .find(|(range, _)| range.start <= start && end <= range.end)
            .map_or(Stroke::NONE, |(_, color)| Stroke::new(1.5, *color));
        let format = TextFormat {
            font_id: font.clone(),
            color,
            underline,
            ..Default::default()
        };
        match &mut run {
            Some((range, alike)) if *alike == format => range.end = end,
            _ => {
                if let Some((range, format)) = run.take() {
                    job.append(&text[range], 0.0, format);
                }
                run = Some((start..end, format));
            }
        }
    }
    if let Some((range, format)) = run {
        job.append(&text[range], 0.0, format);
    }
    job
}

/// The worst problem on each line, by line.
fn problem_lines(diagnostics: &[Diagnostic]) -> Vec<Option<Severity>> {
    let mut lines = Vec::new();
    for problem in diagnostics {
        let line = problem.start.line as usize;
        if lines.len() <= line {
            lines.resize(line + 1, None);
        }
        let worst = &mut lines[line];
        *worst = Some(worst.map_or(problem.severity, |worst: Severity| {
            worst.min(problem.severity)
        }));
    }
    lines
}

fn gutter_width(ui: &egui::Ui, text: &str, font: &FontId) -> f32 {
    let lines = text.lines().count().max(1);
    let digits = lines.to_string().len().max(3);
    let digit = ui.fonts_mut(|fonts| fonts.glyph_width(font, '0'));
    digit * digits as f32 + 16.0
}

/// Shows what the server says the name under the pointer is, and the problems there, once the
/// pointer has rested on it, asking the server if it hasn't yet.
fn show_hover(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    hover: &mut Hover,
    text: &str,
    diagnostics: &[Diagnostic],
    tokens: &[Token],
    server: Option<(&mut LanguageServer, &str)>,
) {
    let now = ui.input(|input| input.time);
    let pointer = ui
        .input(|input| input.pointer.hover_pos())
        .filter(|_| output.response.hovered());
    let byte = pointer.and_then(|pointer| {
        let local = pointer - output.galley_pos;
        let row = output
            .galley
            .rows
            .iter()
            .find(|row| row.rect().y_range().contains(local.y))?;
        if local.x > row.rect().right() {
            return None;
        }
        let cursor = output.galley.cursor_from_pos(local);
        let index = cursor.index.0;
        // The character the pointer is over, not the gap after it.
        let start = output.galley.pos_from_cursor(cursor);
        let index = if local.x < start.left() {
            index.saturating_sub(1)
        } else {
            index
        };
        Some(byte_of(text, index))
    });
    let word = byte.map(|byte| {
        luau::token_containing(tokens, byte)
            .map(|token| token.range.clone())
            // Off any token, a problem there still shows.
            .unwrap_or(byte..byte)
    });
    let Some(word) = word else {
        *hover = Hover::default();
        return;
    };
    if hover.target.as_ref().map(|(target, _)| target) != Some(&word) {
        *hover = Hover {
            target: Some((word.clone(), now)),
            ..Default::default()
        };
    }
    let since = hover.target.as_ref().map_or(now, |(_, since)| *since);
    if now - since < HOVER_DELAY {
        ui.ctx()
            .request_repaint_after_secs((HOVER_DELAY - (now - since)) as f32);
        return;
    }
    let named = luau::token_containing(tokens, word.start).is_some_and(|token| {
        token.range == word
            && matches!(
                token.kind,
                Kind::Name | Kind::Global | Kind::Function | Kind::Keyword
            )
    });
    if named
        && hover.request.is_none()
        && hover.contents.is_none()
        && let Some((server, uri)) = server
    {
        let index = text[..word.start].chars().count();
        hover.request = server.hover(uri, lsp::position_of(text, index));
    }
    let problems: Vec<&Diagnostic> = underlines(text, diagnostics)
        .into_iter()
        .zip(diagnostics)
        .filter(|((range, _), _)| {
            range.start < word.end.max(word.start + 1) && word.start < range.end
        })
        .map(|(_, problem)| problem)
        .collect();
    if problems.is_empty() && hover.contents.is_none() {
        return;
    }
    let first = text[..word.start].chars().count();
    let last = text[..word.end].chars().count();
    let rect = output
        .galley
        .pos_from_cursor(CCursor::new(first))
        .union(output.galley.pos_from_cursor(CCursor::new(last)))
        .translate(output.galley_pos.to_vec2());
    egui::Tooltip::always_open(
        ui.ctx().clone(),
        ui.layer_id(),
        output.response.id.with("hover"),
        rect,
    )
    .width(520.0)
    .show(|ui| {
        for problem in &problems {
            let color = match problem.severity {
                Severity::Error => theme::ERROR,
                _ => theme::WARNING,
            };
            ui.colored_label(color, &problem.message);
        }
        if let Some(contents) = &hover.contents {
            if !problems.is_empty() {
                ui.separator();
            }
            markdown(ui, contents);
        }
    });
}

/// The call the cursor is in, over the cursor's line.
fn show_signature(
    ui: &egui::Ui,
    id: egui::Id,
    cursor: egui::Rect,
    signature: &Signature,
    font: &FontId,
) {
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fade_in(false)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(cursor.left_top() - egui::vec2(0.0, 2.0))
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(600.0);
                let mut job = LayoutJob::default();
                let mut add = |text: &str, color: Color32, underline: bool| {
                    job.append(
                        text,
                        0.0,
                        TextFormat {
                            font_id: font.clone(),
                            color,
                            underline: if underline {
                                Stroke::new(1.0, color)
                            } else {
                                Stroke::NONE
                            },
                            ..Default::default()
                        },
                    );
                };
                let label = &signature.label;
                match &signature.parameter {
                    Some(parameter) if label.get(parameter.clone()).is_some() => {
                        add(&label[..parameter.start], theme::CODE_TEXT, false);
                        add(&label[parameter.clone()], theme::ACCENT, true);
                        add(&label[parameter.end..], theme::CODE_TEXT, false);
                    }
                    _ => add(label, theme::CODE_TEXT, false),
                }
                ui.label(job);
                if !signature.documentation.is_empty() {
                    ui.separator();
                    markdown(ui, &signature.documentation);
                }
            });
        });
}

/// The completions offered under the cursor, the one picked highlighted, returning one clicked.
fn show_completions(
    ui: &egui::Ui,
    id: egui::Id,
    cursor: egui::Rect,
    shown: &[&Completion],
    selected: usize,
    font: &FontId,
) -> Option<usize> {
    let mut clicked = None;
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fade_in(false)
        .pivot(egui::Align2::LEFT_TOP)
        .fixed_pos(cursor.left_bottom() + egui::vec2(0.0, 2.0))
        .show(ui.ctx(), |ui| {
            ui.horizontal_top(|ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(360.0);
                    egui::ScrollArea::vertical()
                        .max_height(ui.spacing().interact_size.y * COMPLETIONS_SHOWN as f32)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.with_layout(
                                egui::Layout::top_down_justified(egui::Align::LEFT),
                                |ui| {
                                    for (i, item) in shown.iter().enumerate() {
                                        let mut job = LayoutJob::default();
                                        job.append(
                                            &item.label,
                                            0.0,
                                            TextFormat::simple(font.clone(), theme::CODE_TEXT),
                                        );
                                        job.append(
                                            &item.detail,
                                            12.0,
                                            TextFormat::simple(font.clone(), theme::LINE_NUMBER),
                                        );
                                        let response = ui.add(
                                            egui::Button::selectable(i == selected, job)
                                                .truncate()
                                                .min_size(egui::vec2(ui.available_width(), 0.0)),
                                        );
                                        if i == selected {
                                            response.scroll_to_me(None);
                                        }
                                        if response.clicked() {
                                            clicked = Some(i);
                                        }
                                    }
                                },
                            );
                        });
                });
                if let Some(item) = shown.get(selected)
                    && !item.documentation.is_empty()
                {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(360.0);
                        markdown(ui, &item.documentation);
                    });
                }
            });
        });
    clicked
}

/// Shows Markdown from the server: code blocks as colored code, rules as separators, and the rest
/// as text, with `code` in it set in the code's font.
fn markdown(ui: &mut egui::Ui, markdown: &str) {
    let code_font = egui::TextStyle::Monospace.resolve(ui.style());
    let body_font = egui::TextStyle::Body.resolve(ui.style());
    let mut code: Option<String> = None;
    let mut paragraph = String::new();
    let flush = |ui: &mut egui::Ui, paragraph: &mut String| {
        if paragraph.trim().is_empty() {
            paragraph.clear();
            return;
        }
        let mut job = LayoutJob::default();
        for (i, part) in paragraph.trim().split('`').enumerate() {
            let font = if i % 2 == 1 { &code_font } else { &body_font };
            job.append(
                part,
                0.0,
                TextFormat::simple(font.clone(), theme::CODE_TEXT),
            );
        }
        job.wrap.max_width = ui.available_width();
        ui.label(job);
        paragraph.clear();
    };
    for line in markdown.lines() {
        if let Some(block) = &mut code {
            if line.trim_start().starts_with("```") {
                let text = block.trim_end_matches('\n');
                let tokens = luau::tokens(text);
                ui.label(layout_job(text, &tokens, &[], &code_font));
                code = None;
            } else {
                block.push_str(line);
                block.push('\n');
            }
        } else if line.trim_start().starts_with("```") {
            flush(ui, &mut paragraph);
            code = Some(String::new());
        } else if line.trim().len() >= 3 && line.trim().chars().all(|c| c == '-') {
            flush(ui, &mut paragraph);
            ui.separator();
        } else if line.trim().is_empty() {
            flush(ui, &mut paragraph);
        } else {
            paragraph.push_str(line);
            paragraph.push('\n');
        }
    }
    flush(ui, &mut paragraph);
}

/// The completions of `list` to show with the cursor at `cursor`: those that start with what's
/// been typed of the word, in any case, then those that hold it.
fn shown_completions<'a>(
    list: &'a CompletionList,
    text: &str,
    cursor: usize,
) -> Vec<&'a Completion> {
    let typed: String = text
        .chars()
        .skip(list.start)
        .take(cursor.saturating_sub(list.start))
        .collect::<String>()
        .to_lowercase();
    let mut starting = Vec::new();
    let mut holding = Vec::new();
    for item in &list.items {
        let label = item.label.to_lowercase();
        if label.starts_with(&typed) {
            starting.push(item);
        } else if label.contains(&typed) {
            holding.push(item);
        }
    }
    // Nothing to offer for a word typed in full.
    if starting.len() == 1 && holding.is_empty() && starting[0].label == typed {
        return Vec::new();
    }
    starting.extend(holding);
    starting
}

fn is_word(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Where the word that ends at character `index` of `text` starts.
fn word_start(text: &str, index: usize) -> usize {
    let before: Vec<char> = text.chars().take(index).collect();
    let word = before.iter().rev().take_while(|c| is_word(**c)).count();
    index - word
}

/// The byte of `text` at character `index`.
fn byte_of(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

/// Replaces the characters `range` of `text` with `with`, returning the character after it.
fn replace(text: &mut String, range: Range<usize>, with: &str) -> usize {
    let start = byte_of(text, range.start);
    let end = byte_of(text, range.end);
    text.replace_range(start..end, with);
    range.start + with.chars().count()
}

/// The line `text` has at character `index`, up to it.
fn line_before(text: &str, index: usize) -> &str {
    let byte = byte_of(text, index);
    let start = text[..byte].rfind('\n').map_or(0, |newline| newline + 1);
    &text[start..byte]
}

/// Whether a line opens a block, which the line after it is indented in.
fn opens_block(line: &str) -> bool {
    let line = line.split("--").next().unwrap_or_default().trim_end();
    let last_word = line
        .rsplit(|c: char| !is_word(c))
        .next()
        .unwrap_or_default();
    matches!(last_word, "then" | "do" | "else" | "repeat")
        || line.ends_with('{')
        || line.ends_with('(')
        || (line.ends_with(')') && line.contains("function"))
}

/// The lines `selection` touches, as characters of `text`: the start of the first, and the start
/// of each after it.
fn line_starts(text: &str, selection: &Range<usize>) -> Vec<usize> {
    let first = selection.start - line_before(text, selection.start).chars().count();
    let mut starts = vec![first];
    for (i, c) in text.chars().enumerate().skip(first) {
        if i >= selection.end.max(first + 1) {
            break;
        }
        if c == '\n' && i + 1 < selection.end {
            starts.push(i + 1);
        }
    }
    starts
}

/// Tab: the next indentation stop, or with lines selected, a level more of indentation for each,
/// returning where the cursor goes.
fn indent(text: &mut String, selection: Range<usize>) -> usize {
    let starts = line_starts(text, &selection);
    if starts.len() == 1 {
        let column = line_before(text, selection.start).chars().count();
        let spaces = INDENT - column % INDENT;
        return replace(text, selection, &" ".repeat(spaces));
    }
    for &start in starts.iter().rev() {
        replace(text, start..start, &" ".repeat(INDENT));
    }
    selection.end + INDENT * starts.len()
}

/// Shift+Tab: a level less of indentation for each line selected, returning where the cursor
/// goes.
fn unindent(text: &mut String, selection: Range<usize>) -> usize {
    let mut cursor = selection.end;
    for &start in line_starts(text, &selection).iter().rev() {
        let spaces = text
            .chars()
            .skip(start)
            .take(INDENT)
            .take_while(|c| *c == ' ')
            .count();
        replace(text, start..start + spaces, "");
        if start < cursor {
            cursor -= spaces.min(cursor - start);
        }
    }
    cursor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_indents_after_lines_that_open_blocks() {
        for line in [
            "    if ready then",
            "for i = 1, 3 do",
            "local function f(x)",
            "local t = {",
            "spawn(function()",
            "else",
            "if x then -- a comment",
        ] {
            assert!(opens_block(line), "{line}");
        }
        for line in ["local x = f(1)", "end", "return then_", "-- if x then"] {
            assert!(!opens_block(line), "{line}");
        }
    }

    #[test]
    fn tab_indents_lines_and_shift_tab_unindents_them() {
        let mut text = String::from("a\nb\nc");
        // Within a line, to the next stop.
        assert_eq!(indent(&mut text, 1..1), 4);
        assert_eq!(text, "a   \nb\nc");
        // Across lines, each line.
        let mut text = String::from("a\nb\nc");
        let cursor = indent(&mut text, 0..3);
        assert_eq!(text, "    a\n    b\nc");
        assert_eq!(cursor, 11);
        let cursor = unindent(&mut text, 2..cursor);
        assert_eq!(text, "a\nb\nc");
        assert_eq!(cursor, 3);
        // A line with less than a level of indentation loses what it has.
        let mut text = String::from("  x");
        assert_eq!(unindent(&mut text, 3..3), 1);
        assert_eq!(text, "x");
    }

    #[test]
    fn completions_are_filtered_by_what_is_typed() {
        let item = |label: &str| Completion {
            label: label.into(),
            detail: String::new(),
            documentation: String::new(),
            insert: label.into(),
            sort: label.into(),
        };
        let list = CompletionList {
            request: None,
            items: vec![
                item("draw_sprite"),
                item("clear"),
                item("draw_text"),
                item("redraw"),
            ],
            start: 9,
            selected: 0,
        };
        let labels = |text: &str| -> Vec<String> {
            shown_completions(&list, text, text.chars().count())
                .into_iter()
                .map(|item| item.label.clone())
                .collect()
        };
        assert_eq!(
            labels("graphics."),
            ["draw_sprite", "clear", "draw_text", "redraw"]
        );
        assert_eq!(
            labels("graphics.Dr"),
            ["draw_sprite", "draw_text", "redraw"]
        );
        assert_eq!(labels("graphics.clear"), Vec::<String>::new());
        assert_eq!(word_start("local abc", 9), 6);
    }

    #[test]
    fn layout_colors_tokens_and_underlines_problems() {
        let text = "local x = nope";
        let tokens = luau::tokens(text);
        let font = FontId::monospace(12.0);
        let job = layout_job(text, &tokens, &[(10..14, theme::ERROR)], &font);
        assert_eq!(job.text, text);
        let section = |byte: usize| {
            job.sections
                .iter()
                .find(|section| {
                    section.byte_range.start.0 <= byte && byte < section.byte_range.end.0
                })
                .unwrap()
        };
        assert_eq!(section(0).format.color, theme::code_color(Kind::Keyword));
        assert_eq!(section(11).format.underline.color, theme::ERROR);
        assert_eq!(section(6).format.underline, Stroke::NONE);
    }
}
