//! The FileSystem dock: the files in the game's folder. Double-clicking a scene opens it.

use wingine::gui::egui;

use crate::{project::Entry, theme};

/// What the dock asks of the editor.
pub enum Action {
    /// Open the scene file at this path.
    Open(String),
    /// Place the scene file at this path in the open scene.
    Instance(String),
    /// Read the folder again.
    Refresh,
}

#[derive(Default)]
pub struct FileSystem {
    filter: String,
    /// The file last clicked.
    selected: Option<String>,
}

impl FileSystem {
    pub fn ui(&mut self, ui: &mut egui::Ui, name: &str, files: &[Entry]) -> Option<Action> {
        let mut action = None;
        ui.horizontal(|ui| {
            ui.strong("FileSystem");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("⟳")
                    .on_hover_text("Read the folder again")
                    .clicked()
                {
                    action = Some(Action::Refresh);
                }
            });
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .hint_text("Filter files")
                .desired_width(f32::INFINITY),
        );
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                egui::CollapsingHeader::new(format!("📁 {name}"))
                    .id_salt("project folder")
                    .default_open(true)
                    .show(ui, |ui| {
                        let filter = self.filter.to_lowercase();
                        for entry in files {
                            self.entry(ui, entry, &filter, &mut action);
                        }
                    });
            });
        action
    }

    fn entry(
        &mut self,
        ui: &mut egui::Ui,
        entry: &Entry,
        filter: &str,
        action: &mut Option<Action>,
    ) {
        if !matches(entry, filter) {
            return;
        }
        if let Some(children) = &entry.children {
            egui::CollapsingHeader::new(format!("📁 {}", entry.name))
                .id_salt(&entry.path)
                // While filtering, show what matched.
                .open((!filter.is_empty()).then_some(true))
                .show(ui, |ui| {
                    for child in children {
                        self.entry(ui, child, filter, action);
                    }
                });
            return;
        }
        let is_scene = entry.name.ends_with(".scene.luau");
        let selected = self.selected.as_deref() == Some(entry.path.as_str());
        let response = ui
            .horizontal(|ui| {
                ui.label(theme::file_icon(&entry.name));
                ui.selectable_label(selected, &entry.name)
            })
            .inner;
        if response.clicked() {
            self.selected = Some(entry.path.clone());
        }
        if is_scene && response.double_clicked() {
            *action = Some(Action::Open(entry.path.clone()));
        }
        if is_scene {
            response.context_menu(|ui| {
                if ui.button("Open").clicked() {
                    *action = Some(Action::Open(entry.path.clone()));
                }
                if ui.button("Instance in the open scene").clicked() {
                    *action = Some(Action::Instance(entry.path.clone()));
                }
            });
        }
    }
}

/// Whether `entry`, or something inside it, has `filter` in its name.
fn matches(entry: &Entry, filter: &str) -> bool {
    filter.is_empty()
        || entry.name.to_lowercase().contains(filter)
        || entry
            .children
            .as_ref()
            .is_some_and(|children| children.iter().any(|child| matches(child, filter)))
}
