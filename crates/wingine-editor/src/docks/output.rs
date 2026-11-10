//! The Output panel: what the editor logs, and what a game played from it prints.

use std::{
    collections::VecDeque,
    sync::{Mutex, PoisonError},
};

use wingine::gui::egui;

/// How many lines the panel keeps.
const MAX_LINES: usize = 5000;

/// Lines logged since the panel last took them, from any thread.
static LOGGED: Mutex<Vec<Line>> = Mutex::new(Vec::new());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub level: Level,
    pub text: String,
}

/// The lines shown, oldest first.
#[derive(Default)]
pub struct Output {
    lines: VecDeque<Line>,
}

impl Output {
    pub fn push(&mut self, level: Level, text: impl Into<String>) {
        if self.lines.len() == MAX_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(Line {
            level,
            text: text.into(),
        });
    }

    /// A line a game printed: errors and warnings as the `wingine` executable logs them.
    pub fn push_game_line(&mut self, text: String) {
        let level = if text.starts_with("[ERROR") || text.starts_with("error:") {
            Level::Error
        } else if text.starts_with("[WARN") {
            Level::Warning
        } else {
            Level::Info
        };
        self.push(level, text);
    }

    /// Takes in what's been logged since.
    pub fn take_logged(&mut self) {
        let logged = std::mem::take(&mut *LOGGED.lock().unwrap_or_else(PoisonError::into_inner));
        for line in logged {
            self.push(line.level, line.text);
        }
    }

    pub fn lines(&self) -> &VecDeque<Line> {
        &self.lines
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Clear").clicked() {
                self.lines.clear();
            }
            ui.weak(format!("{} lines", self.lines.len()));
        });
        ui.separator();
        let visuals = ui.visuals().clone();
        egui::ScrollArea::both()
            .auto_shrink(false)
            .stick_to_bottom(true)
            .show_rows(
                ui,
                ui.text_style_height(&egui::TextStyle::Monospace),
                self.lines.len(),
                |ui, rows| {
                    for line in self.lines.range(rows) {
                        let color = match line.level {
                            Level::Info => visuals.text_color(),
                            Level::Warning => visuals.warn_fg_color,
                            Level::Error => visuals.error_fg_color,
                        };
                        ui.label(egui::RichText::new(&line.text).monospace().color(color));
                    }
                },
            );
    }
}

/// Logs to standard error, and for the editor, to the Output panel: the editor's own messages,
/// the engine's warnings, and errors from anywhere.
pub struct Logger {
    /// Whether lines go to the Output panel too, as the editor's do. A game Play runs only prints
    /// them, for the editor to read, since nothing in its process takes them from the panel.
    pub to_panel: bool,
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        let target = metadata.target();
        let level = if target.starts_with("wingine_editor") || target == "lua" {
            log::Level::Info
        } else if target.starts_with("wingine") {
            log::Level::Warn
        } else {
            log::Level::Error
        };
        metadata.level() <= level
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let text = record.args().to_string();
        eprintln!("[{} {}] {text}", record.level(), record.target());
        if !self.to_panel {
            return;
        }
        let level = match record.level() {
            log::Level::Error => Level::Error,
            log::Level::Warn => Level::Warning,
            _ => Level::Info,
        };
        LOGGED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Line { level, text });
    }

    fn flush(&self) {}
}

#[cfg(test)]
mod tests {
    use log::Log;

    use super::*;

    fn logged(logger: &Logger, text: &str) -> bool {
        logger.log(
            &log::Record::builder()
                .args(format_args!("{text}"))
                .level(log::Level::Warn)
                .target("wingine_editor::tests")
                .build(),
        );
        let lines = LOGGED.lock().unwrap_or_else(PoisonError::into_inner);
        lines.iter().any(|line| line.text == text)
    }

    #[test]
    fn only_the_editor_keeps_lines_for_the_panel() {
        assert!(!logged(&Logger { to_panel: false }, "a game's warning"));
        assert!(logged(&Logger { to_panel: true }, "the editor's warning"));
    }
}
