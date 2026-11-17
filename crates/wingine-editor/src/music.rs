//! The Music screen: the open songs as tabs, and the one shown with its transport along the top,
//! its instruments on the left, its arrangement of sections and tracks in the middle, and a piano
//! roll of the pattern being edited along the bottom.
//!
//! Playing a song plays it through the editor's own audio. Changing it as it plays plays the
//! changed song from where it was, once the change is made: when nothing's being dragged or typed.
//! Muting and soloing tracks is for listening, and isn't saved.

use std::{
    collections::BTreeSet,
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

use wingine::{
    Context,
    audio::Sound,
    gui::egui::{self, Key},
    music::{
        Filter, FilterKind, Instrument, MAX_BARS, Marker, Note, Performance, Pitch, SongSettings,
        TEMPO_RANGE, Vibrato, Wave,
    },
};

use crate::{docks::output::Level, document::Reloaded, song_document::SongDocument, songs, theme};

/// Height of a pitch's row in the piano roll, in points.
const ROW: f32 = 14.0;
/// Width of the keys along the piano roll's left.
const KEYS: f32 = 44.0;
/// How near a note's right end, in points, a drag lengthens it rather than moves it.
const EDGE: f32 = 6.0;
/// Frames a second notes are auditioned at.
const AUDITION_RATE: u32 = 44_100;
/// Seconds an auditioned note is held.
const AUDITION_LENGTH: f32 = 0.35;
/// Seconds a song takes to fade as it stops, or gives way to itself changed.
const FADE: f32 = 0.03;
/// Frames a second songs bake at.
const BAKE_RATE: u32 = 48_000;

/// Beats a grid step can be, with their names.
const SNAPS: [(f64, &str); 7] = [
    (1.0, "1 beat"),
    (0.5, "1/2 beat"),
    (0.25, "1/4 beat"),
    (0.125, "1/8 beat"),
    (1.0 / 3.0, "1/3 beat"),
    (1.0 / 6.0, "1/6 beat"),
    (0.0625, "1/16 beat"),
];

/// What the screen asks of the editor.
pub enum Action {
    /// Close the song at this index, asking first if it has unsaved changes.
    Close(usize),
    /// Save the song shown.
    Save,
    /// Read the project's folder again: a file was made.
    Rescan,
}

/// A song playing.
struct Playing {
    performance: Performance,
    /// The path of the song it's of.
    path: String,
    /// The revision of the song it's playing.
    revision: u64,
}

/// A note being dragged in the piano roll.
#[derive(Clone, Copy, Debug)]
struct Drag {
    /// The note's beat and pitch as the drag started.
    note: (f64, Pitch),
    length: f64,
    /// Whether it's lengthening the note, rather than moving it.
    resize: bool,
    /// How far into the note it was grabbed, in beats.
    grab: f64,
    /// Where it would go now: its beat and pitch, and its length.
    to: (f64, Pitch, f64),
}

/// Where the piano roll was laid out last frame, for tests to aim at notes.
#[derive(Clone, Copy, Debug)]
struct RollLayout {
    /// Where beat 0 of the highest pitch's row starts.
    origin: egui::Pos2,
    zoom: f32,
}

pub struct MusicEditor {
    folder: Option<PathBuf>,
    songs: Vec<SongDocument>,
    /// The index of the song shown, if there are any.
    current: usize,
    /// What to tell the Output panel.
    messages: Vec<(Level, String)>,
    /// The path in the New Song dialog, while it's open.
    new_song: Option<String>,
    playing: Option<Playing>,
    /// Tracks muted and soloed for listening, by name.
    muted: BTreeSet<String>,
    soloed: BTreeSet<String>,
    /// The piano roll's grid step and zoom, in beats and points a beat.
    snap: f64,
    zoom: f32,
    drag: Option<Drag>,
    /// The pattern the piano roll last scrolled to show, so it scrolls once for each.
    scrolled_to: Option<String>,
    roll: Option<RollLayout>,
    /// The octave the keyboard under an instrument plays.
    octave: u8,
    /// Why the song shown can't play, if it can't, as of the last frame.
    problem: Option<String>,
}

impl Default for MusicEditor {
    fn default() -> Self {
        Self {
            folder: None,
            songs: Vec::new(),
            current: 0,
            messages: Vec::new(),
            new_song: None,
            playing: None,
            muted: BTreeSet::new(),
            soloed: BTreeSet::new(),
            snap: 0.25,
            zoom: 64.0,
            drag: None,
            scrolled_to: None,
            roll: None,
            octave: 4,
            problem: None,
        }
    }
}

impl MusicEditor {
    /// Edits the songs of the game in `folder`, closing those open. A song playing stops at the
    /// next [`MusicEditor::keep_playing`].
    pub fn set_project(&mut self, folder: Option<PathBuf>) {
        self.folder = folder;
        self.songs.clear();
        self.current = 0;
    }

    /// Opens the song file at `path` in the project, or shows it if it's open.
    pub fn open(&mut self, path: &str) -> Result<(), String> {
        if let Some(i) = self.songs.iter().position(|song| song.path == path) {
            self.current = i;
            return Ok(());
        }
        let folder = self.folder.as_ref().ok_or("no game is open")?;
        let song = SongDocument::open(folder, path).map_err(|err| err.to_string())?;
        self.songs.push(song);
        self.current = self.songs.len() - 1;
        Ok(())
    }

    pub fn songs(&self) -> &[SongDocument] {
        &self.songs
    }

    /// The song shown.
    pub fn song(&self) -> Option<&SongDocument> {
        self.songs.get(self.current)
    }

    pub fn song_mut(&mut self) -> Option<&mut SongDocument> {
        self.songs.get_mut(self.current)
    }

    /// Whether a song open here is playing.
    pub fn is_playing(&self) -> bool {
        self.playing.is_some()
    }

    /// The performance playing, if a song is.
    pub fn performance(&self) -> Option<Performance> {
        self.playing.as_ref().map(|playing| playing.performance)
    }

    pub fn is_dirty(&self) -> bool {
        self.songs.iter().any(SongDocument::is_dirty)
    }

    /// What to tell the Output panel since this was last asked.
    pub fn take_messages(&mut self) -> Vec<(Level, String)> {
        std::mem::take(&mut self.messages)
    }

    /// Where `beat` of `pitch`'s row was in the piano roll in the window, in points, as it was
    /// last laid out: for tests, to click on notes.
    pub fn roll_point(&self, beat: f64, pitch: Pitch) -> Option<[f32; 2]> {
        let roll = self.roll?;
        let row = f32::from(Pitch::HIGHEST.number() - pitch.number());
        Some([
            roll.origin.x + beat as f32 * roll.zoom,
            roll.origin.y + (row + 0.5) * ROW,
        ])
    }

    /// Saves the song at `index`, saying how it went.
    pub fn save(&mut self, index: usize) -> (Level, String) {
        let (Some(folder), Some(song)) = (&self.folder, self.songs.get_mut(index)) else {
            return (Level::Warning, "There's no song to save.".into());
        };
        match song.save(folder) {
            Ok(()) => (Level::Info, format!("Saved {}", song.path)),
            Err(err) => (Level::Error, err.to_string()),
        }
    }

    /// Saves the song shown.
    pub fn save_current(&mut self) -> (Level, String) {
        self.save(self.current)
    }

    /// Saves every song with unsaved changes, saying how each went.
    pub fn save_all(&mut self) -> Vec<(Level, String)> {
        (0..self.songs.len())
            .filter(|&i| self.songs[i].is_dirty())
            .collect::<Vec<_>>()
            .into_iter()
            .map(|i| self.save(i))
            .collect()
    }

    /// Closes the song at `index`, changes and all, stopping it if it's playing.
    pub fn close(&mut self, ctx: &mut Context, index: usize) {
        if index >= self.songs.len() {
            return;
        }
        if self
            .playing
            .as_ref()
            .is_some_and(|playing| playing.path == self.songs[index].path)
        {
            self.stop(ctx);
        }
        self.songs.remove(index);
        if self.current > index || self.current == self.songs.len() {
            self.current = self.current.saturating_sub(1);
        }
    }

    /// Ends the change being made to each song, as a frame with nothing dragged or typed does.
    pub fn end_changes(&mut self) {
        for song in &mut self.songs {
            song.end_change();
        }
    }

    pub fn undo(&mut self) {
        if let Some(song) = self.song_mut() {
            song.undo();
        }
    }

    pub fn redo(&mut self) {
        if let Some(song) = self.song_mut() {
            song.redo();
        }
    }

    /// Reads again the songs whose files changed on disk, saying what became of them.
    pub fn reload_changed(&mut self) {
        let Some(folder) = self.folder.clone() else {
            return;
        };
        for song in &mut self.songs {
            let path = song.path.clone();
            match song.reload_if_changed(&folder) {
                Some(Reloaded::Read) => self.messages.push((
                    Level::Info,
                    format!("Read {path} again: it changed on disk."),
                )),
                Some(Reloaded::Kept) => self.messages.push((
                    Level::Warning,
                    format!(
                        "{path} changed on disk, but has unsaved changes here: saving it will \
                         replace the file."
                    ),
                )),
                Some(Reloaded::Failed(err)) => self
                    .messages
                    .push((Level::Warning, format!("Couldn't read {path} again: {err}"))),
                None => {}
            }
        }
    }

    /// Plays the song shown from its start, or from the section `section`.
    pub fn play(&mut self, ctx: &mut Context, section: Option<String>) {
        self.stop(ctx);
        let Some(document) = self.songs.get_mut(self.current) else {
            return;
        };
        let song = match document.song() {
            Ok(song) => song,
            Err(err) => {
                self.messages
                    .push((Level::Error, format!("{} can't play: {err}", document.path)));
                return;
            }
        };
        let settings = SongSettings {
            section,
            ..SongSettings::default()
        };
        match ctx.audio().play_song(&song, settings) {
            Ok(performance) => {
                self.playing = Some(Playing {
                    performance,
                    path: document.path.clone(),
                    revision: document.revision(),
                });
                self.apply_mutes(ctx);
            }
            Err(err) => self.messages.push((Level::Error, err.to_string())),
        }
    }

    /// Stops the song playing, if one is.
    pub fn stop(&mut self, ctx: &mut Context) {
        if let Some(playing) = self.playing.take()
            && let Some(song) = ctx.audio().song(playing.performance)
        {
            song.stop(FADE);
        }
    }

    /// Notices the song playing has ended, or plays the song changed in its place, from where it
    /// was, once the change is made, or stops it if it's closed. Called each update.
    pub fn keep_playing(&mut self, ctx: &mut Context) {
        let Some((performance, path, played)) = self
            .playing
            .as_ref()
            .map(|playing| (playing.performance, playing.path.clone(), playing.revision))
        else {
            return;
        };
        let Some(live) = ctx.audio().song(performance) else {
            self.playing = None;
            return;
        };
        let position = live.position();
        let section = live
            .song()
            .section_name(position.section)
            .map(str::to_owned);
        let Some(document) = self.songs.iter_mut().find(|song| song.path == path) else {
            self.stop(ctx);
            return;
        };
        if document.is_changing() {
            return;
        }
        // A song that can't play leaves the last that could playing.
        let Ok(song) = document.song() else {
            return;
        };
        if document.revision() == played {
            return;
        }
        let revision = document.revision();
        let section = section.filter(|name| song.section_index(name).is_some());
        let beat = if section.is_some() {
            position.beat
        } else {
            0.0
        };
        let settings = SongSettings {
            section,
            beat,
            ..SongSettings::default()
        };
        if let Some(old) = ctx.audio().song(performance) {
            old.stop(FADE);
        }
        match ctx.audio().play_song(&song, settings) {
            Ok(performance) => {
                self.playing = Some(Playing {
                    performance,
                    path: document.path.clone(),
                    revision,
                });
                self.apply_mutes(ctx);
            }
            Err(err) => {
                self.playing = None;
                self.messages.push((Level::Error, err.to_string()));
            }
        }
    }

    /// Fades each track of the song playing to its volume, or silence if it's muted, or another
    /// is soloed.
    fn apply_mutes(&self, ctx: &mut Context) {
        let Some(playing) = &self.playing else {
            return;
        };
        let Some(document) = self.songs.iter().find(|song| song.path == playing.path) else {
            return;
        };
        let Some(live) = ctx.audio().song(playing.performance) else {
            return;
        };
        for track in &document.file().tracks {
            let silent = self.muted.contains(&track.name)
                || (!self.soloed.is_empty() && !self.soloed.contains(&track.name));
            let volume = if silent { 0.0 } else { track.volume };
            let _ = live.set_track_volume(&track.name, volume, FADE);
        }
    }

    /// Bakes the song shown to a WAV file beside it, saying how it went.
    pub fn bake(&mut self) -> Option<Action> {
        let folder = self.folder.clone()?;
        let document = self.songs.get_mut(self.current)?;
        let stem = document
            .path
            .strip_suffix(".song.luau")
            .unwrap_or(&document.path);
        let wav = format!("{stem}.wav");
        let baked = document
            .song()
            .map_err(|err| format!("{} can't play: {err}", document.path))
            .and_then(|song| {
                song.bake(folder.join(&wav), BAKE_RATE, None)
                    .map_err(|err| err.to_string())
            });
        match baked {
            Ok(rendered) => {
                let looping = if rendered.loops { ", looping" } else { "" };
                self.messages.push((
                    Level::Info,
                    format!("Baked {wav}: {:.2} seconds{looping}.", rendered.duration()),
                ));
                Some(Action::Rescan)
            }
            Err(err) => {
                self.messages.push((Level::Error, err));
                None
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, ctx: &mut Context) -> Option<Action> {
        let mut action = None;
        egui::Panel::top("music toolbar").show(ui, |ui| {
            self.menu(ui, &mut action);
            self.tabs(ui, &mut action);
        });
        if self.songs.is_empty() {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.weak(
                        "Double-click a song in the FileSystem dock to open it, or make one with \
                         File ▸ New Song.",
                    );
                });
            });
        } else {
            self.problem = self.songs[self.current].song().err();
            egui::Panel::top("music transport").show(ui, |ui| self.transport(ui, ctx));
            egui::Panel::left("music instruments")
                .resizable(true)
                .default_size(270.0)
                .show(ui, |ui| self.instruments(ui, ctx));
            egui::Panel::bottom("piano roll")
                .resizable(true)
                .default_size(330.0)
                .show(ui, |ui| self.piano_roll(ui, ctx));
            egui::CentralPanel::default().show(ui, |ui| self.arrangement(ui, ctx));
        }
        self.new_song_dialog(ui, &mut action);
        if self.playing.is_some() {
            ui.ctx().request_repaint();
        }
        action
    }

    fn menu(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        let has_song = !self.songs.is_empty();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New Song…").clicked() {
                    self.new_song = Some("music/new.song.luau".into());
                }
                if ui
                    .add_enabled(has_song, egui::Button::new("Save").shortcut_text("Ctrl+S"))
                    .clicked()
                {
                    *action = Some(Action::Save);
                }
                if ui
                    .add_enabled(has_song, egui::Button::new("Bake to WAV"))
                    .on_hover_text("Render the song once through to a WAV file beside it")
                    .clicked()
                    && let Some(baked) = self.bake()
                {
                    *action = Some(baked);
                }
                if ui
                    .add_enabled(has_song, egui::Button::new("Close Song"))
                    .clicked()
                {
                    *action = Some(Action::Close(self.current));
                }
            });
            ui.menu_button("Edit", |ui| {
                let (can_undo, can_redo) = self
                    .song()
                    .map_or((false, false), |song| (song.can_undo(), song.can_redo()));
                if ui
                    .add_enabled(can_undo, egui::Button::new("Undo").shortcut_text("Ctrl+Z"))
                    .clicked()
                {
                    self.undo();
                }
                if ui
                    .add_enabled(can_redo, egui::Button::new("Redo").shortcut_text("Ctrl+Y"))
                    .clicked()
                {
                    self.redo();
                }
            });
        });
    }

    /// The open songs' tabs.
    fn tabs(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        ui.horizontal(|ui| {
            for (i, song) in self.songs.iter().enumerate() {
                let title = if song.is_dirty() {
                    format!("{}*", song.name())
                } else {
                    song.name().to_owned()
                };
                if ui
                    .selectable_label(i == self.current, title)
                    .on_hover_text(&song.path)
                    .clicked()
                {
                    self.current = i;
                    self.drag = None;
                }
                if ui
                    .small_button("✖")
                    .on_hover_text("Close the song")
                    .clicked()
                {
                    *action = Some(Action::Close(i));
                }
                ui.separator();
            }
        });
    }

    /// Play and Stop, the song's tempo, meter and start, and where it's playing.
    fn transport(&mut self, ui: &mut egui::Ui, ctx: &mut Context) {
        let section = self.songs[self.current].selection.section.clone();
        ui.horizontal(|ui| {
            if labelled_button(ui, "▶ Song", "Play Song", "Play the song from its start") {
                self.play(ctx, None);
            }
            if labelled_button(
                ui,
                "▶ Section",
                "Play Section",
                "Play from the selected section",
            ) {
                self.play(ctx, section);
            }
            if labelled_button(ui, "⏹ Stop", "Stop Song", "Stop the song") {
                self.stop(ctx);
            }
            ui.separator();
            let document = &mut self.songs[self.current];
            let file = document.file();
            let [least, most] = TEMPO_RANGE;
            if let Some(tempo) = number(ui, "Tempo", file.tempo, least..=most, 0.5, " bpm") {
                document.edit().tempo = tempo;
            }
            let beats = document.file().meter[0];
            if let Some(beats) = number(ui, "Beats a Bar", beats, 1..=32, 0.05, "") {
                songs::set_beats_per_bar(document.edit(), beats);
            }
            let note = document.file().meter[1];
            let notes = [1, 2, 4, 8, 16, 32].map(|note| (note, note.to_string()));
            if let Some(note) = {
                ui.label("/");
                choice(ui, "Beat Note", &note, &notes)
            } {
                document.edit().meter[1] = note;
            }
            let sections = songs::section_order(document.file());
            let start = document.file().start.clone();
            let options: Vec<(String, String)> = sections
                .iter()
                .map(|name| (name.clone(), name.clone()))
                .collect();
            if let Some(start) = {
                ui.label("Start");
                choice(ui, "Start", &start, &options)
            } {
                document.edit().start = start;
            }
            ui.separator();
            let shown = &self.songs[self.current].path;
            let position = self
                .playing
                .as_ref()
                .filter(|playing| playing.path == *shown)
                .and_then(|playing| {
                    let live = ctx.audio().song(playing.performance)?;
                    let position = live.position();
                    let name = live.song().section_name(position.section)?.to_owned();
                    let (bar, beat) = position.bar(live.song().meter()[0]);
                    Some(format!("{name} · bar {bar} · beat {beat:.1}"))
                });
            match position {
                Some(position) => ui.monospace(position),
                None => ui.weak("Stopped"),
            };
            if let Some(problem) = &self.problem {
                ui.colored_label(theme::ERROR, format!("⚠ {problem}"));
            }
        });
    }

    /// The song's instruments, the selected one's settings, and a keyboard to hear it.
    fn instruments(&mut self, ui: &mut egui::Ui, ctx: &mut Context) {
        let document = &mut self.songs[self.current];
        ui.horizontal(|ui| {
            ui.strong("Instruments");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let selected = document.selection.instrument.clone();
                if ui
                    .add_enabled(selected.is_some(), egui::Button::new("🗑"))
                    .on_hover_text("Delete the instrument")
                    .widget_info_labelled("Delete Instrument")
                    .clicked()
                    && let Some(name) = selected
                {
                    if let Err(err) = songs::remove_instrument(document.edit(), &name) {
                        self.messages.push((Level::Error, err));
                    }
                    document.end_change();
                }
                if ui
                    .button("+")
                    .on_hover_text("Add an instrument")
                    .widget_info_labelled("Add Instrument")
                    .clicked()
                {
                    let name =
                        songs::add_instrument(document.edit(), "instrument", Instrument::default());
                    document.selection.instrument = Some(name);
                    document.end_change();
                }
            });
        });
        let mut select = None;
        egui::ScrollArea::vertical()
            .id_salt("instrument list")
            .max_height(120.0)
            .show(ui, |ui| {
                for name in document.file().instruments.keys() {
                    let selected = document.selection.instrument.as_ref() == Some(name);
                    if ui.selectable_label(selected, name).clicked() {
                        select = Some(name.clone());
                    }
                }
            });
        if select.is_some() {
            document.selection.instrument = select;
        }
        ui.separator();
        let Some(name) = document.selection.instrument.clone() else {
            ui.weak("Add an instrument for tracks to play.");
            return;
        };
        let Some(instrument) = document.file().instruments.get(&name).copied() else {
            return;
        };
        egui::ScrollArea::vertical()
            .id_salt("instrument settings")
            .show(ui, |ui| {
                if let Some(to) =
                    name_field(ui, ("instrument name", &name), &name, "Instrument Name")
                {
                    match songs::rename_instrument(document.edit(), &name, &to) {
                        Ok(()) => document.selection.instrument = Some(to),
                        Err(err) => self.messages.push((Level::Error, err)),
                    }
                    document.end_change();
                }
                if let Some(changed) = instrument_settings(ui, instrument) {
                    document.edit().instruments.insert(name.clone(), changed);
                }
                envelope(ui, &instrument);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Play");
                    if let Some(octave) = number(ui, "Octave", self.octave, 0..=8, 0.05, "") {
                        self.octave = octave;
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for semitone in 0..12_u8 {
                        let Some(pitch) = Pitch::new((self.octave + 1) * 12 + semitone) else {
                            continue;
                        };
                        let name = pitch.to_string();
                        let black = name.contains('#');
                        let key = egui::Button::new(
                            egui::RichText::new(name.trim_end_matches(char::is_numeric)).small(),
                        )
                        .fill(if black {
                            theme::BACKGROUND
                        } else {
                            theme::WIDGET
                        });
                        if ui
                            .add(key)
                            .widget_info_labelled(&format!("Play {name}"))
                            .clicked()
                        {
                            audition(ctx, &instrument, pitch, &mut self.messages);
                        }
                    }
                });
            });
    }

    /// The song's sections as columns and tracks as rows, with the selected section's settings
    /// over them.
    fn arrangement(&mut self, ui: &mut egui::Ui, ctx: &mut Context) {
        let mut mutes_changed = false;
        let playing_section = self.playing_section(ctx);
        let document = &mut self.songs[self.current];
        ui.horizontal(|ui| {
            ui.strong("Arrangement");
            if ui.button("+ Track").clicked() {
                let instrument = document
                    .selection
                    .instrument
                    .clone()
                    .or_else(|| document.file().instruments.keys().next().cloned());
                match instrument {
                    Some(instrument) => {
                        if let Err(err) = songs::add_track(document.edit(), "track", &instrument) {
                            self.messages.push((Level::Error, err));
                        }
                    }
                    None => self.messages.push((
                        Level::Error,
                        "Add an instrument for the track to play first.".into(),
                    )),
                }
                document.end_change();
            }
            if ui.button("+ Section").clicked() {
                let name = songs::add_section(document.edit(), "section", 4);
                document.selection.section = Some(name);
                document.end_change();
            }
        });
        if let Some(section) = document.selection.section.clone() {
            self.section_settings(ui, &section);
        }
        let document = &mut self.songs[self.current];
        ui.separator();
        let order = songs::section_order(document.file());
        let patterns: Vec<(Option<String>, String)> = std::iter::once((None, "—".to_owned()))
            .chain(
                document
                    .file()
                    .patterns
                    .keys()
                    .map(|name| (Some(name.clone()), name.clone())),
            )
            .collect();
        egui::ScrollArea::both()
            .id_salt("arrangement")
            .auto_shrink(false)
            .show(ui, |ui| {
                egui::Grid::new("arrangement grid")
                    .striped(true)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.weak("Track");
                        for name in &order {
                            let bars = document.file().sections[name].bars;
                            let selected = document.selection.section.as_ref() == Some(name);
                            let mut text = egui::RichText::new(format!("{name} · {bars}"));
                            if playing_section.as_ref() == Some(name) {
                                text = text.color(theme::ACCENT);
                            }
                            if ui
                                .selectable_label(selected, text)
                                .widget_info_labelled(&format!("Section {name}"))
                                .clicked()
                            {
                                document.selection.section = Some(name.clone());
                            }
                        }
                        ui.end_row();
                        let tracks: Vec<String> = document
                            .file()
                            .tracks
                            .iter()
                            .map(|t| t.name.clone())
                            .collect();
                        for track in &tracks {
                            ui.horizontal(|ui| {
                                mutes_changed |= Self::track_header(
                                    ui,
                                    document,
                                    &mut self.muted,
                                    &mut self.soloed,
                                    &mut self.messages,
                                    track,
                                );
                            });
                            for section in &order {
                                let current =
                                    document.file().sections[section].tracks.get(track).cloned();
                                let label = format!("{track} in {section}");
                                if let Some(pattern) = choice(ui, &label, &current, &patterns) {
                                    songs::set_cell(
                                        document.edit(),
                                        section,
                                        track,
                                        pattern.as_deref(),
                                    );
                                    if pattern.is_some() {
                                        document.selection.pattern = pattern;
                                    }
                                    document.end_change();
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
        if mutes_changed {
            self.apply_mutes(ctx);
        }
    }

    /// A track's name, instrument, volume and pan, and buttons to mute, solo, move and delete
    /// it, returning whether it was muted or soloed.
    fn track_header(
        ui: &mut egui::Ui,
        document: &mut SongDocument,
        muted: &mut BTreeSet<String>,
        soloed: &mut BTreeSet<String>,
        messages: &mut Vec<(Level, String)>,
        track: &str,
    ) -> bool {
        let mut mutes_changed = false;
        let Some(def) = document
            .file()
            .tracks
            .iter()
            .find(|t| t.name == track)
            .cloned()
        else {
            return false;
        };
        if ui
            .small_button("⏶")
            .widget_info_labelled(&format!("Move {track} Up"))
            .clicked()
        {
            songs::move_track(document.edit(), track, -1);
            document.end_change();
        }
        if ui
            .small_button("⏷")
            .widget_info_labelled(&format!("Move {track} Down"))
            .clicked()
        {
            songs::move_track(document.edit(), track, 1);
            document.end_change();
        }
        if let Some(to) = name_field(ui, ("track name", track), track, &format!("Track {track}")) {
            if let Err(err) = songs::rename_track(document.edit(), track, &to) {
                messages.push((Level::Error, err));
            } else {
                rename_in(muted, track, &to);
                rename_in(soloed, track, &to);
            }
            document.end_change();
        }
        let instruments: Vec<(String, String)> = document
            .file()
            .instruments
            .keys()
            .map(|name| (name.clone(), name.clone()))
            .collect();
        if let Some(instrument) = choice(
            ui,
            &format!("{track} Instrument"),
            &def.instrument,
            &instruments,
        ) && let Some(def) = document.edit().tracks.iter_mut().find(|t| t.name == track)
        {
            def.instrument = instrument;
        }
        if let Some(volume) = number_as(
            ui,
            "Vol",
            &format!("{track} Volume"),
            def.volume,
            0.0..=4.0,
            0.01,
            "",
        ) && let Some(def) = document.edit().tracks.iter_mut().find(|t| t.name == track)
        {
            def.volume = volume;
        }
        if let Some(pan) = number_as(
            ui,
            "Pan",
            &format!("{track} Pan"),
            def.pan,
            -1.0..=1.0,
            0.01,
            "",
        ) && let Some(def) = document.edit().tracks.iter_mut().find(|t| t.name == track)
        {
            def.pan = pan;
        }
        for (set, icon, what) in [(&mut *muted, "M", "Mute"), (&mut *soloed, "S", "Solo")] {
            let on = set.contains(track);
            if ui
                .selectable_label(on, icon)
                .on_hover_text(format!("{what} the track while listening"))
                .widget_info_labelled(&format!("{what} {track}"))
                .clicked()
            {
                if on {
                    set.remove(track);
                } else {
                    set.insert(track.to_owned());
                }
                mutes_changed = true;
            }
        }
        if ui
            .small_button("🗑")
            .on_hover_text("Delete the track")
            .widget_info_labelled(&format!("Delete {track}"))
            .clicked()
        {
            songs::remove_track(document.edit(), track);
            document.end_change();
            muted.remove(track);
            soloed.remove(track);
            mutes_changed = true;
        }
        mutes_changed
    }

    /// The selected section's name, length, what follows it, and its markers.
    fn section_settings(&mut self, ui: &mut egui::Ui, section: &str) {
        let document = &mut self.songs[self.current];
        let Some(def) = document.file().sections.get(section).cloned() else {
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.label("Section");
            if let Some(to) = name_field(ui, ("section name", section), section, "Section Name") {
                match songs::rename_section(document.edit(), section, &to) {
                    Ok(()) => document.selection.section = Some(to),
                    Err(err) => self.messages.push((Level::Error, err)),
                }
                document.end_change();
            }
            if let Some(bars) = number(ui, "Section Bars", def.bars, 1..=MAX_BARS, 0.05, "") {
                songs::set_section_bars(document.edit(), section, bars);
            }
            let options: Vec<(Option<String>, String)> = std::iter::once((None, "(end)".into()))
                .chain(
                    songs::section_order(document.file())
                        .into_iter()
                        .map(|name| (Some(name.clone()), name)),
                )
                .collect();
            if let Some(next) = {
                ui.label("Next");
                choice(ui, "Next", &def.next, &options)
            } && let Some(def) = document.edit().sections.get_mut(section)
            {
                def.next = next;
            }
            if ui.button("+ Marker").clicked()
                && let Some(def) = document.edit().sections.get_mut(section)
            {
                let name = songs::unique_name("marker", |name| {
                    def.markers.iter().any(|marker| marker.name == name)
                });
                def.markers.insert(0, Marker { beat: 0.0, name });
                document.end_change();
            }
            if ui.button("Delete Section").clicked() {
                if let Err(err) = songs::remove_section(document.edit(), section) {
                    self.messages.push((Level::Error, err));
                }
                document.end_change();
            }
        });
        let beats = f64::from(def.bars * document.file().beats_per_bar());
        let mut remove = None;
        for (i, marker) in def.markers.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label("Marker");
                if let Some(to) = name_field(
                    ui,
                    ("marker name", section, i),
                    &marker.name,
                    &format!("Marker {}", marker.name),
                ) && let Some(def) = document.edit().sections.get_mut(section)
                {
                    def.markers[i].name = to;
                    document.end_change();
                }
                let most = (beats - 1e-3).max(0.0);
                if let Some(beat) = number(
                    ui,
                    &format!("{} Beat", marker.name),
                    marker.beat,
                    0.0..=most,
                    0.05,
                    "",
                ) && let Some(def) = document.edit().sections.get_mut(section)
                {
                    def.markers[i].beat = songs::tidy(beat);
                    def.markers.sort_by(|a, b| {
                        a.beat.total_cmp(&b.beat).then_with(|| a.name.cmp(&b.name))
                    });
                }
                if ui
                    .small_button("✖")
                    .widget_info_labelled(&format!("Delete Marker {}", marker.name))
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove
            && let Some(def) = document.edit().sections.get_mut(section)
        {
            def.markers.remove(i);
            document.end_change();
        }
    }

    /// The section playing in the song shown, if it's playing.
    fn playing_section(&self, ctx: &mut Context) -> Option<String> {
        let playing = self.playing.as_ref()?;
        if self.songs.get(self.current)?.path != playing.path {
            return None;
        }
        let live = ctx.audio().song(playing.performance)?;
        let position = live.position();
        live.song()
            .section_name(position.section)
            .map(str::to_owned)
    }

    /// Where the song shown is playing in the pattern `pattern`, in beats from its start, if a
    /// track of the section playing plays it.
    fn playhead(&self, ctx: &mut Context, pattern: &str) -> Option<f64> {
        let playing = self.playing.as_ref()?;
        let document = self.songs.get(self.current)?;
        if document.path != playing.path {
            return None;
        }
        let live = ctx.audio().song(playing.performance)?;
        let position = live.position();
        let section = live.song().section_name(position.section)?;
        let file = document.file();
        let def = file.sections.get(section)?;
        if !def.tracks.values().any(|name| name == pattern) {
            return None;
        }
        let beats = songs::pattern_beats(file, file.patterns.get(pattern)?);
        Some(position.beat.rem_euclid(beats))
    }

    /// The selected pattern, its settings, and its notes on a grid of pitches by beats.
    fn piano_roll(&mut self, ui: &mut egui::Ui, ctx: &mut Context) {
        self.roll = None;
        let document = &mut self.songs[self.current];
        ui.horizontal_wrapped(|ui| {
            ui.strong("Pattern");
            let patterns: Vec<(Option<String>, String)> = document
                .file()
                .patterns
                .keys()
                .map(|name| (Some(name.clone()), name.clone()))
                .collect();
            let selected = document.selection.pattern.clone();
            if let Some(pattern) = choice(ui, "Pattern", &selected, &patterns) {
                document.selection.pattern = pattern;
                document.selection.note = None;
            }
            if ui.button("+ Pattern").clicked() {
                let name = songs::add_pattern(document.edit(), "pattern", 1);
                document.selection.pattern = Some(name);
                document.end_change();
            }
            let Some(pattern) = selected.filter(|name| document.file().patterns.contains_key(name))
            else {
                return;
            };
            if ui.button("Duplicate").clicked() {
                document.selection.pattern = songs::duplicate_pattern(document.edit(), &pattern);
                document.end_change();
            }
            if let Some(to) = name_field(ui, ("pattern name", &pattern), &pattern, "Pattern Name") {
                match songs::rename_pattern(document.edit(), &pattern, &to) {
                    Ok(()) => document.selection.pattern = Some(to),
                    Err(err) => self.messages.push((Level::Error, err)),
                }
                document.end_change();
            }
            let bars = document.file().patterns[&pattern].bars;
            if let Some(bars) = number(ui, "Pattern Bars", bars, 1..=MAX_BARS, 0.05, "") {
                songs::set_pattern_bars(document.edit(), &pattern, bars);
            }
            if ui.button("Delete Pattern").clicked() {
                songs::remove_pattern(document.edit(), &pattern);
                document.end_change();
            }
            ui.separator();
            let snaps = SNAPS.map(|(beats, name)| (beats.to_bits(), name.to_owned()));
            if let Some(snap) = {
                ui.label("Snap");
                choice(ui, "Snap", &self.snap.to_bits(), &snaps)
            } {
                self.snap = f64::from_bits(snap);
            }
            ui.label("Zoom");
            ui.add(egui::Slider::new(&mut self.zoom, 16.0..=256.0).show_value(false))
                .widget_info_labelled("Zoom");
        });
        let document = &self.songs[self.current];
        let Some(pattern) = document
            .selection
            .pattern
            .clone()
            .filter(|name| document.file().patterns.contains_key(name))
        else {
            ui.weak("Add a pattern for tracks to play.");
            return;
        };
        let playhead = self.playhead(ctx, &pattern);
        let instrument = self.instrument_of(&pattern);
        let rows = f32::from(Pitch::HIGHEST.number() - Pitch::LOWEST.number() + 1);
        let mut scroll = egui::ScrollArea::both()
            .id_salt("piano roll")
            .auto_shrink(false);
        if self.scrolled_to.as_ref() != Some(&pattern) {
            // Around the pattern's notes, or middle C.
            let document = &self.songs[self.current];
            let notes = &document.file().patterns[&pattern].notes;
            let top = notes
                .iter()
                .map(|note| note.pitch)
                .max()
                .unwrap_or(Pitch::from_name("C5").expect("C5 is a pitch"));
            let row = f32::from(Pitch::HIGHEST.number().saturating_sub(top.number() + 6));
            scroll = scroll
                .vertical_scroll_offset(row * ROW)
                .horizontal_scroll_offset(0.0);
            self.scrolled_to = Some(pattern.clone());
        }
        scroll.show(ui, |ui| {
            self.roll_grid(ui, ctx, &pattern, rows, playhead, instrument);
        });
    }

    /// The instrument a track playing `pattern` plays, to hear its notes: the first track's
    /// in the song's sections, or the selected instrument.
    fn instrument_of(&self, pattern: &str) -> Option<Instrument> {
        let file = self.songs.get(self.current)?.file();
        let track = songs::section_order(file).iter().find_map(|section| {
            file.sections[section]
                .tracks
                .iter()
                .find(|(_, name)| *name == pattern)
                .map(|(track, _)| track.clone())
        });
        let name = track
            .and_then(|track| file.tracks.iter().find(|t| t.name == track))
            .map(|track| track.instrument.clone())
            .or_else(|| self.songs[self.current].selection.instrument.clone())?;
        file.instruments.get(&name).copied()
    }

    fn roll_grid(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &mut Context,
        pattern: &str,
        rows: f32,
        playhead: Option<f64>,
        instrument: Option<Instrument>,
    ) {
        let document = &mut self.songs[self.current];
        let file = document.file();
        let def = file.patterns[pattern].clone();
        let beats = songs::pattern_beats(file, &def);
        let per_bar = f64::from(file.beats_per_bar());
        let zoom = self.zoom;
        let size = egui::vec2(KEYS + beats as f32 * zoom + 1.0, rows * ROW);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Canvas, true, "Piano Roll"));
        let origin = egui::pos2(rect.left() + KEYS, rect.top());
        self.roll = Some(RollLayout { origin, zoom });
        let x_of = |beat: f64| origin.x + beat as f32 * zoom;
        let y_of =
            |pitch: Pitch| origin.y + f32::from(Pitch::HIGHEST.number() - pitch.number()) * ROW;
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::BACKGROUND);
        // Rows: darker for black keys, a line under each C.
        let weak = ui.visuals().weak_text_color();
        for number in Pitch::LOWEST.number()..=Pitch::HIGHEST.number() {
            let pitch = Pitch::new(number).expect("within the keyboard");
            let name = pitch.to_string();
            let black = name.contains('#');
            let top = y_of(pitch);
            let row = egui::Rect::from_min_max(
                egui::pos2(origin.x, top),
                egui::pos2(rect.right(), top + ROW),
            );
            if black {
                painter.rect_filled(row, 0.0, egui::Color32::from_black_alpha(60));
            }
            let key = egui::Rect::from_min_max(
                egui::pos2(rect.left(), top),
                egui::pos2(origin.x, top + ROW),
            );
            painter.rect_filled(
                key.shrink2(egui::vec2(0.0, 0.5)),
                0.0,
                if black {
                    theme::BACKGROUND
                } else {
                    theme::WIDGET
                },
            );
            if number % 12 == 0 {
                painter.line_segment(
                    [
                        egui::pos2(rect.left(), top + ROW),
                        egui::pos2(rect.right(), top + ROW),
                    ],
                    egui::Stroke::new(1.0, weak.gamma_multiply(0.5)),
                );
            }
            painter.text(
                key.left_center() + egui::vec2(4.0, 0.0),
                egui::Align2::LEFT_CENTER,
                &name,
                egui::FontId::proportional(9.0),
                if number % 12 == 0 {
                    ui.visuals().text_color()
                } else {
                    weak
                },
            );
        }
        // Columns: each step faintly, each beat, and each bar strongly.
        let steps = (beats / self.snap).round() as usize;
        for step in 0..=steps {
            let beat = songs::tidy(step as f64 * self.snap);
            let x = x_of(beat);
            let (alpha, width) = if (beat / per_bar).fract().abs() < 1e-6 {
                (110, 1.5)
            } else if beat.fract().abs() < 1e-6 {
                (50, 1.0)
            } else {
                (18, 1.0)
            };
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(width, egui::Color32::from_white_alpha(alpha)),
            );
        }
        // The notes, the one dragged where it would go.
        let selected = document.selection.note;
        let rect_of = |beat: f64, pitch: Pitch, length: f64| {
            egui::Rect::from_min_max(
                egui::pos2(x_of(beat), y_of(pitch) + 1.0),
                egui::pos2(
                    x_of(beat + length).max(x_of(beat) + 3.0),
                    y_of(pitch) + ROW - 1.0,
                ),
            )
        };
        for note in &def.notes {
            let dragged = self.drag.is_some_and(|drag| {
                (drag.note.0 - note.beat).abs() < songs::SAME_BEAT && drag.note.1 == note.pitch
            });
            let shown = rect_of(note.beat, note.pitch, note.length);
            let fill = theme::ACCENT.gamma_multiply(0.35 + 0.65 * note.velocity);
            painter.rect_filled(
                shown,
                2.0,
                if dragged {
                    fill.gamma_multiply(0.4)
                } else {
                    fill
                },
            );
            let is_selected = selected.is_some_and(|(beat, pitch)| {
                (beat - note.beat).abs() < songs::SAME_BEAT && pitch == note.pitch
            });
            if is_selected {
                painter.rect_stroke(
                    shown,
                    2.0,
                    egui::Stroke::new(2.0, theme::KEY),
                    egui::StrokeKind::Inside,
                );
            }
        }
        if let Some(drag) = self.drag {
            let (beat, pitch, length) = drag.to;
            painter.rect_stroke(
                rect_of(beat, pitch, length),
                2.0,
                egui::Stroke::new(2.0, theme::KEY),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(beat) = playhead {
            let x = x_of(beat);
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(2.0, theme::ACCENT),
            );
        }

        // What's under the pointer: a beat and pitch, and the note there, if any.
        let at = |pos: egui::Pos2| -> Option<(f64, Pitch)> {
            if pos.x < origin.x {
                return None;
            }
            let beat = f64::from((pos.x - origin.x) / zoom);
            let row = ((pos.y - origin.y) / ROW).floor();
            let number = f32::from(Pitch::HIGHEST.number()) - row;
            let pitch = Pitch::new(number.clamp(0.0, 127.0) as u8)?;
            (beat < beats).then_some((beat, pitch))
        };
        let note_at = |pos: egui::Pos2| -> Option<(Note, bool)> {
            def.notes
                .iter()
                .rev()
                .find(|note| rect_of(note.beat, note.pitch, note.length).contains(pos))
                .map(|note| {
                    let right = rect_of(note.beat, note.pitch, note.length).right();
                    (*note, pos.x > right - EDGE)
                })
        };
        let pointer = response.interact_pointer_pos().or(response.hover_pos());
        let step = self.snap;
        let mut change: Option<RollChange> = None;
        // Where the drag started, which the pointer has moved on from as it's noticed.
        let pressed = ui
            .input(|input| input.pointer.press_origin())
            .or(response.interact_pointer_pos());
        if response.drag_started()
            && let Some(pos) = pressed
            && let Some((note, edge)) = note_at(pos)
        {
            let grab = at(pos).map_or(0.0, |(beat, _)| beat - note.beat);
            self.drag = Some(Drag {
                note: (note.beat, note.pitch),
                length: note.length,
                resize: edge,
                grab,
                to: (note.beat, note.pitch, note.length),
            });
            document.selection.note = Some((note.beat, note.pitch));
        }
        if let Some(drag) = &mut self.drag
            && let Some(pos) = pointer
            && let Some((beat, pitch)) = at(pos.clamp(rect.min, rect.max - egui::vec2(1.0, 1.0)))
        {
            drag.to = if drag.resize {
                let length = songs::snap_nearest(beat - drag.note.0, step).max(step);
                (drag.note.0, drag.note.1, length)
            } else {
                let beat = songs::snap_nearest(beat - drag.grab, step).clamp(0.0, beats - step);
                (songs::tidy(beat), pitch, drag.length)
            };
        }
        if response.drag_stopped()
            && let Some(drag) = self.drag.take()
        {
            change = Some(RollChange::Dragged(drag));
        } else if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            change = Some(match note_at(pos) {
                Some((note, _)) => RollChange::Select(note),
                None => match at(pos) {
                    Some((beat, pitch)) => RollChange::Add(songs::snap(beat, step), pitch),
                    None => RollChange::None,
                },
            });
        } else if response.secondary_clicked()
            && let Some(pos) = response.interact_pointer_pos()
            && let Some((note, _)) = note_at(pos)
        {
            change = Some(RollChange::Remove(note.beat, note.pitch));
        } else if response.hovered()
            && let Some((beat, pitch)) = selected
            && ui.input_mut(|input| {
                input.consume_key(egui::Modifiers::NONE, Key::Delete)
                    || input.consume_key(egui::Modifiers::NONE, Key::Backspace)
            })
        {
            change = Some(RollChange::Remove(beat, pitch));
        }
        let Some(change) = change else {
            return;
        };
        let pattern = pattern.to_owned();
        let mut heard = None;
        let result = match change {
            RollChange::None => Ok(()),
            RollChange::Select(note) => {
                document.selection.note = Some((note.beat, note.pitch));
                heard = Some(note.pitch);
                Ok(())
            }
            RollChange::Add(beat, pitch) => {
                let note = Note {
                    beat,
                    pitch,
                    length: step,
                    velocity: 1.0,
                };
                let def = document.edit().patterns.get_mut(&pattern).expect("shown");
                let added = songs::add_note(def, beats, note);
                if added.is_ok() {
                    document.selection.note = Some((beat, pitch));
                    heard = Some(pitch);
                }
                added
            }
            RollChange::Remove(beat, pitch) => {
                let def = document.edit().patterns.get_mut(&pattern).expect("shown");
                songs::remove_note(def, beat, pitch);
                document.selection.note = None;
                Ok(())
            }
            RollChange::Dragged(drag) => {
                let (beat, pitch, length) = drag.to;
                let def = document.edit().patterns.get_mut(&pattern).expect("shown");
                if drag.resize {
                    songs::resize_note(def, drag.note, length, step);
                    Ok(())
                } else if (beat - drag.note.0).abs() < songs::SAME_BEAT && pitch == drag.note.1 {
                    Ok(())
                } else {
                    let moved = songs::move_note(def, beats, drag.note, (beat, pitch));
                    if moved.is_ok() {
                        document.selection.note = Some((beat, pitch));
                        if pitch != drag.note.1 {
                            heard = Some(pitch);
                        }
                    }
                    moved
                }
            }
        };
        document.end_change();
        if let Err(err) = result {
            self.messages.push((Level::Warning, err));
        }
        if let (Some(pitch), Some(instrument)) = (heard, instrument) {
            audition(ctx, &instrument, pitch, &mut self.messages);
        }
    }

    fn new_song_dialog(&mut self, ui: &mut egui::Ui, action: &mut Option<Action>) {
        let (Some(path), Some(folder)) = (&mut self.new_song, self.folder.clone()) else {
            return;
        };
        let problem = new_song_problem(&folder, path);
        let mut create = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("new song dialog")).show(ui.ctx(), |ui| {
            ui.set_width(400.0);
            ui.heading("New Song");
            ui.label("Its path in the game's folder:");
            let field = ui.add(egui::TextEdit::singleline(path).desired_width(f32::INFINITY));
            field.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::TextInput, true, "Song Path")
            });
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
            let path = self.new_song.take().unwrap_or_default();
            let file = folder.join(&path);
            let made = file
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .map_err(|err| err.to_string())
                .and_then(|()| {
                    songs::new_song()
                        .write(&file)
                        .map_err(|err| err.to_string())
                })
                .and_then(|()| self.open(&path));
            match made {
                Ok(()) => *action = Some(Action::Rescan),
                Err(err) => self.messages.push((Level::Error, err)),
            }
        } else if cancel {
            self.new_song = None;
        }
    }
}

/// What a click or drag in the piano roll does.
enum RollChange {
    None,
    Select(Note),
    Add(f64, Pitch),
    Remove(f64, Pitch),
    Dragged(Drag),
}

/// What's wrong with `path` for a new song in the game in `folder`, if anything.
fn new_song_problem(folder: &Path, path: &str) -> Option<String> {
    if !path.ends_with(".song.luau") {
        Some("Songs' names end in .song.luau.".into())
    } else if path.starts_with('/') || path.split('/').any(|part| part.is_empty() || part == "..") {
        Some("Give a path inside the game's folder, such as music/theme.song.luau.".into())
    } else if folder.join(path).exists() {
        Some("There's a file there already.".into())
    } else {
        None
    }
}

/// Plays a note of `pitch` on `instrument`, as a key of the keyboard does.
fn audition(
    ctx: &mut Context,
    instrument: &Instrument,
    pitch: Pitch,
    messages: &mut Vec<(Level, String)>,
) {
    if let Some(problem) = instrument.problem() {
        messages.push((
            Level::Warning,
            format!("The instrument can't play: {problem}"),
        ));
        return;
    }
    let samples = instrument.render_note(pitch, AUDITION_LENGTH, 1.0, AUDITION_RATE);
    match Sound::from_samples(&samples, AUDITION_RATE, 2) {
        Ok(sound) => {
            ctx.audio().play(&sound);
        }
        Err(err) => messages.push((Level::Warning, err.to_string())),
    }
}

/// `set` with `from` named `to` in its place, if it held it.
fn rename_in(set: &mut BTreeSet<String>, from: &str, to: &str) {
    if set.remove(from) {
        set.insert(to.to_owned());
    }
}

/// Fields for each of `instrument`'s settings, returning it changed, if it was.
fn instrument_settings(ui: &mut egui::Ui, mut instrument: Instrument) -> Option<Instrument> {
    let before = instrument;
    let waves = Wave::ALL.map(|wave| (wave, wave.name().to_owned()));
    if let Some(wave) = {
        ui.label("Wave");
        choice(ui, "Wave", &instrument.wave, &waves)
    } {
        instrument.wave = wave;
    }
    egui::Grid::new("instrument settings grid")
        .num_columns(2)
        .show(ui, |ui| {
            let row = |ui: &mut egui::Ui,
                       name: &str,
                       value: &mut f32,
                       range: RangeInclusive<f32>,
                       speed: f64,
                       suffix: &str| {
                if let Some(changed) = number(ui, name, *value, range, speed, suffix) {
                    *value = changed;
                }
                ui.end_row();
            };
            if instrument.wave == Wave::Square {
                row(ui, "Duty", &mut instrument.duty, 0.01..=0.99, 0.005, "");
            }
            row(
                ui,
                "Attack",
                &mut instrument.attack,
                0.0..=60.0,
                0.005,
                " s",
            );
            row(ui, "Decay", &mut instrument.decay, 0.0..=60.0, 0.005, " s");
            row(ui, "Sustain", &mut instrument.sustain, 0.0..=1.0, 0.005, "");
            row(
                ui,
                "Release",
                &mut instrument.release,
                0.0..=60.0,
                0.005,
                " s",
            );
            row(ui, "Volume", &mut instrument.volume, 0.0..=4.0, 0.005, "");
            row(ui, "Pan", &mut instrument.pan, -1.0..=1.0, 0.01, "");
            row(ui, "Sweep", &mut instrument.sweep, -96.0..=96.0, 0.1, " st");
            row(
                ui,
                "Sweep Time",
                &mut instrument.sweep_time,
                0.0..=60.0,
                0.005,
                " s",
            );
        });
    let mut vibrato = instrument.vibrato.is_some();
    if ui.checkbox(&mut vibrato, "Vibrato").changed() {
        instrument.vibrato = vibrato.then(Vibrato::default);
    }
    if let Some(mut wobble) = instrument.vibrato {
        egui::Grid::new("vibrato grid")
            .num_columns(2)
            .show(ui, |ui| {
                for (name, value, range, suffix) in [
                    ("Vibrato Rate", &mut wobble.rate, 0.0..=30.0, " Hz"),
                    ("Vibrato Depth", &mut wobble.depth, -12.0..=12.0, " st"),
                    ("Vibrato Delay", &mut wobble.delay, 0.0..=60.0, " s"),
                ] {
                    if let Some(changed) = number(ui, name, *value, range, 0.01, suffix) {
                        *value = changed;
                    }
                    ui.end_row();
                }
            });
        instrument.vibrato = Some(wobble);
    }
    let mut filtered = instrument.filter.is_some();
    if ui.checkbox(&mut filtered, "Filter").changed() {
        instrument.filter = filtered.then(Filter::default);
    }
    if let Some(mut filter) = instrument.filter {
        let kinds = FilterKind::ALL.map(|kind| (kind, kind.name().to_owned()));
        if let Some(kind) = {
            ui.label("Kind");
            choice(ui, "Filter Kind", &filter.kind, &kinds)
        } {
            filter.kind = kind;
        }
        egui::Grid::new("filter grid")
            .num_columns(2)
            .show(ui, |ui| {
                for (name, value, range, speed, suffix) in [
                    ("Cutoff", &mut filter.cutoff, 10.0..=20_000.0, 5.0, " Hz"),
                    ("Resonance", &mut filter.resonance, 0.0..=1.0, 0.005, ""),
                    ("Filter Sweep", &mut filter.sweep, -8.0..=8.0, 0.01, " oct"),
                    (
                        "Filter Sweep Time",
                        &mut filter.sweep_time,
                        0.0..=60.0,
                        0.005,
                        " s",
                    ),
                ] {
                    if let Some(changed) = number(ui, name, *value, range, speed, suffix) {
                        *value = changed;
                    }
                    ui.end_row();
                }
            });
        instrument.filter = Some(filter);
    }
    (instrument != before).then_some(instrument)
}

/// The instrument's envelope: a note held a while, then let go.
fn envelope(ui: &mut egui::Ui, instrument: &Instrument) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().min(240.0), 56.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, theme::BACKGROUND);
    let attack = instrument.attack.max(wingine::music::MIN_RAMP);
    let decay = instrument.decay;
    let release = instrument.release.max(wingine::music::MIN_RAMP);
    let hold = ((attack + decay + release) * 0.5).max(0.1);
    let total = attack + decay + hold + release;
    let falling = |progress: f32| {
        let left = 1.0 - progress.clamp(0.0, 1.0);
        left * left
    };
    let level = |time: f32| {
        let sustain = instrument.sustain;
        let held = if time < attack {
            time / attack
        } else if time - attack < decay {
            sustain + (1.0 - sustain) * falling((time - attack) / decay)
        } else {
            sustain
        };
        let let_go = attack + decay + hold;
        if time < let_go {
            held
        } else {
            let at = if let_go < attack {
                let_go / attack
            } else {
                sustain
            };
            at * falling((time - let_go) / release)
        }
    };
    let inner = rect.shrink(4.0);
    let points: Vec<egui::Pos2> = (0..=96)
        .map(|i| {
            let time = total * i as f32 / 96.0;
            egui::pos2(
                inner.left() + inner.width() * i as f32 / 96.0,
                inner.bottom() - inner.height() * level(time),
            )
        })
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.5, theme::ACCENT),
    ));
}

/// A button showing `icon`, named `label` for screen readers and tests, returning whether it was
/// clicked.
fn labelled_button(ui: &mut egui::Ui, icon: &str, label: &str, hover: &str) -> bool {
    ui.button(icon)
        .on_hover_text(hover)
        .widget_info_labelled(label)
        .clicked()
}

/// A number field after the label `name`, which names it for screen readers and tests too,
/// returning the value changed, if it was. A value outside `range` from a file stays as it is
/// until it's changed.
fn number<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    name: &str,
    value: T,
    range: RangeInclusive<T>,
    speed: f64,
    suffix: &str,
) -> Option<T> {
    number_as(ui, name, name, value, range, speed, suffix)
}

/// A number field after the label `shown`, named `name` for screen readers and tests, as
/// [`number`] has one.
fn number_as<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    shown: &str,
    name: &str,
    value: T,
    range: RangeInclusive<T>,
    speed: f64,
    suffix: &str,
) -> Option<T> {
    ui.label(shown);
    let mut changed = value;
    let mut field = egui::DragValue::new(&mut changed)
        .range(range)
        .clamp_existing_to_range(false)
        .speed(speed)
        .max_decimals(4)
        .suffix(suffix);
    if T::INTEGRAL {
        field = field.fixed_decimals(0);
    }
    let response = ui.add(field);
    response.widget_info(|| {
        let mut info = egui::WidgetInfo::drag_value(response.enabled(), changed.to_f64());
        info.label = Some(name.to_owned());
        info
    });
    (changed != value).then_some(changed)
}

/// A drop-down after the label `name`, which names it too, of `options`' names, returning the
/// option chosen, if it changed.
fn choice<T: Clone + PartialEq>(
    ui: &mut egui::Ui,
    name: &str,
    value: &T,
    options: &[(T, String)],
) -> Option<T> {
    let shown = options
        .iter()
        .find(|(option, _)| option == value)
        .map_or("?", |(_, text)| text.as_str());
    let mut chosen = None;
    let response = egui::ComboBox::from_id_salt(name)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for (option, text) in options {
                if ui.selectable_label(option == value, text).clicked() && option != value {
                    chosen = Some(option.clone());
                }
            }
        })
        .response;
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, name));
    chosen
}

/// A field showing `name`, which gives the name typed once it's entered or loses focus, if it
/// changed. `id` keeps what's typed between frames.
fn name_field(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    label: &str,
) -> Option<String> {
    let id = ui.make_persistent_id(id);
    let mut text = ui
        .data_mut(|data| data.get_temp::<String>(id))
        .unwrap_or_else(|| name.to_owned());
    let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(110.0));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, label));
    if response.has_focus() {
        ui.data_mut(|data| data.insert_temp(id, text));
        return None;
    }
    ui.data_mut(|data| data.remove::<String>(id));
    let text = text.trim().to_owned();
    (response.lost_focus() && !text.is_empty() && text != name).then_some(text)
}

/// Labelling a widget for screen readers and tests, as a button.
trait Labelled {
    fn widget_info_labelled(self, label: &str) -> Self;
}

impl Labelled for egui::Response {
    fn widget_info_labelled(self, label: &str) -> Self {
        let role = egui::Role::Button;
        self.widget_info(|| egui::WidgetInfo::labeled(role, true, label));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_songs_go_inside_the_game() {
        let folder = Path::new("nowhere");
        assert!(new_song_problem(folder, "theme.luau").is_some());
        assert!(new_song_problem(folder, "../theme.song.luau").is_some());
        assert!(new_song_problem(folder, "/theme.song.luau").is_some());
        assert_eq!(new_song_problem(folder, "music/theme.song.luau"), None);
    }

    #[test]
    fn renaming_keeps_what_was_muted() {
        let mut set = BTreeSet::from(["a".to_owned()]);
        rename_in(&mut set, "a", "b");
        rename_in(&mut set, "c", "d");
        assert_eq!(set, BTreeSet::from(["b".to_owned()]));
    }
}
