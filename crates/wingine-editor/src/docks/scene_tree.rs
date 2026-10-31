//! The Scene dock: the open scene's tree of nodes. Clicking selects, dragging moves a node into
//! another or between two, and its menus add, rename, duplicate and delete nodes.

use wingine::{gui::egui, scene::NodeDef};

use crate::{
    document::Document,
    edit::{self, KINDS, NodePath},
    theme,
};

/// What the dock asks of the editor.
pub enum Action {
    Select(NodePath),
    /// Add a node of this kind, one of [`KINDS`], as the last child of `parent`.
    Add {
        parent: NodePath,
        kind: &'static str,
    },
    /// Place the scene file at `scene` as the last child of `parent`.
    Instance {
        parent: NodePath,
        scene: String,
    },
    Rename(NodePath, String),
    Duplicate(NodePath),
    Delete(NodePath),
    /// Move the node at `from` into `parent`, at `index` among its children as they are now.
    Move {
        from: NodePath,
        parent: NodePath,
        index: usize,
    },
}

/// What can be added to the scene: kinds that need something to show need some of it.
pub struct Addable<'a> {
    pub sheets: bool,
    pub tilesets: bool,
    /// The scene files that can be placed in this one.
    pub scenes: &'a [String],
}

#[derive(Default)]
pub struct SceneTree {
    filter: String,
    /// The node being renamed, and its name so far.
    renaming: Option<(NodePath, String)>,
    /// Whether the field for renaming needs the keyboard, the frame it appears.
    focus_rename: bool,
}

/// Where a node dragged over another lands.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Drop {
    Before,
    Into,
    After,
}

impl SceneTree {
    /// Starts renaming the node at `path` in place.
    pub fn rename(&mut self, document: &Document, path: NodePath) {
        if let Some(node) = edit::node(document.file(), &path) {
            self.renaming = Some((path, node.name.clone()));
            self.focus_rename = true;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, document: &Document, addable: &Addable) -> Vec<Action> {
        let mut actions = Vec::new();
        let selected = document.selected.clone();
        let target = selected.clone().unwrap_or_default();
        ui.horizontal(|ui| {
            ui.strong("Scene");
            ui.menu_button("➕", |ui| {
                add_menu(ui, &target, addable, &mut actions);
            })
            .response
            .on_hover_text("Add a node to the selected one");
            ui.add_enabled_ui(!addable.scenes.is_empty(), |ui| {
                ui.menu_button("⛓", |ui| {
                    instance_menu(ui, &target, addable, &mut actions);
                })
                .response
                .on_hover_text("Place a scene in the selected node");
            });
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .hint_text("Filter nodes")
                .desired_width(f32::INFINITY),
        );
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            let scene_name = document.name().trim_end_matches(".scene.luau");
            let root_selected = selected.as_deref() == Some(&[][..]);
            let response = ui
                .horizontal(|ui| {
                    ui.label(theme::kind_icon("scene"));
                    ui.selectable_label(root_selected, scene_name)
                })
                .inner;
            if response.clicked() {
                actions.push(Action::Select(Vec::new()));
            }
            response.context_menu(|ui| {
                ui.menu_button("Add Child Node", |ui| {
                    add_menu(ui, &[], addable, &mut actions);
                });
                ui.add_enabled_ui(!addable.scenes.is_empty(), |ui| {
                    ui.menu_button("Instance Child Scene", |ui| {
                        instance_menu(ui, &[], addable, &mut actions);
                    });
                });
            });
            if let Some(from) = response.dnd_release_payload::<NodePath>() {
                let index = document.file().children.len();
                actions.push(Action::Move {
                    from: (*from).clone(),
                    parent: Vec::new(),
                    index,
                });
            }
            let filter = self.filter.to_lowercase();
            ui.indent("nodes", |ui| {
                let mut path = Vec::new();
                self.nodes(
                    ui,
                    &document.file().children,
                    &mut path,
                    &selected,
                    &filter,
                    addable,
                    &mut actions,
                );
            });
        });
        actions
    }

    #[expect(clippy::too_many_arguments)]
    fn nodes(
        &mut self,
        ui: &mut egui::Ui,
        nodes: &[NodeDef],
        path: &mut NodePath,
        selected: &Option<NodePath>,
        filter: &str,
        addable: &Addable,
        actions: &mut Vec<Action>,
    ) {
        for (i, node) in nodes.iter().enumerate() {
            if !matches(node, filter) {
                continue;
            }
            path.push(i);
            let id = ui.make_persistent_id(("node", path.as_slice()));
            if node.children.is_empty() {
                ui.horizontal(|ui| {
                    // Lined up with the names of nodes that can open.
                    ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x);
                    self.row(ui, node, path, selected, addable, actions);
                });
            } else {
                egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    true,
                )
                .show_header(ui, |ui| {
                    self.row(ui, node, path, selected, addable, actions);
                })
                .body(|ui| {
                    self.nodes(ui, &node.children, path, selected, filter, addable, actions);
                });
            }
            path.pop();
        }
    }

    /// A node's row: its icon and name, which can be dragged, and its menu.
    fn row(
        &mut self,
        ui: &mut egui::Ui,
        node: &NodeDef,
        path: &NodePath,
        selected: &Option<NodePath>,
        addable: &Addable,
        actions: &mut Vec<Action>,
    ) {
        ui.label(theme::kind_icon(node.kind.name()))
            .on_hover_text(node.kind.name());
        if let Some((renaming, name)) = &mut self.renaming
            && renaming == path
        {
            let response = ui.add(egui::TextEdit::singleline(name).desired_width(120.0));
            if self.focus_rename {
                response.request_focus();
                self.focus_rename = false;
            } else if response.lost_focus() {
                if !ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    actions.push(Action::Rename(path.clone(), name.clone()));
                }
                self.renaming = None;
            }
            return;
        }

        let is_selected = selected.as_ref() == Some(path);
        let text = if node.visible {
            egui::RichText::new(&node.name)
        } else {
            egui::RichText::new(&node.name).weak()
        };
        // One widget for clicks and drags: egui gives a drag-only widget over a label the clicks.
        let dragged = ui
            .selectable_label(is_selected, text)
            .interact(egui::Sense::click_and_drag());
        dragged.dnd_set_drag_payload(path.clone());
        if dragged.dragged()
            && let Some(pointer) = ui.ctx().pointer_interact_pos()
        {
            egui::Area::new(ui.make_persistent_id("dragged node"))
                .order(egui::Order::Tooltip)
                .fixed_pos(pointer + egui::vec2(12.0, 4.0))
                .interactable(false)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(&node.name));
                });
        }
        if dragged.clicked() {
            actions.push(Action::Select(path.clone()));
        }
        if dragged.double_clicked() {
            self.renaming = Some((path.clone(), node.name.clone()));
            self.focus_rename = true;
        }
        dragged.context_menu(|ui| {
            ui.menu_button("Add Child Node", |ui| add_menu(ui, path, addable, actions));
            ui.add_enabled_ui(!addable.scenes.is_empty(), |ui| {
                ui.menu_button("Instance Child Scene", |ui| {
                    instance_menu(ui, path, addable, actions);
                });
            });
            ui.separator();
            if ui.button("Rename").clicked() {
                self.renaming = Some((path.clone(), node.name.clone()));
                self.focus_rename = true;
            }
            if ui.button("Duplicate").clicked() {
                actions.push(Action::Duplicate(path.clone()));
            }
            let (&index, parent) = path.split_last().expect("nodes have paths");
            if ui.button("Move Up").clicked() && index > 0 {
                actions.push(Action::Move {
                    from: path.clone(),
                    parent: parent.to_vec(),
                    index: index - 1,
                });
            }
            if ui.button("Move Down").clicked() {
                actions.push(Action::Move {
                    from: path.clone(),
                    parent: parent.to_vec(),
                    index: index + 2,
                });
            }
            ui.separator();
            if ui.button("Delete").clicked() {
                actions.push(Action::Delete(path.clone()));
            }
        });

        // Dropping another node here: on the top quarter, before it; the bottom quarter, after
        // it; between, into it.
        let rect = dragged.rect;
        let at = |pointer: egui::Pos2| {
            let t = (pointer.y - rect.top()) / rect.height().max(1.0);
            if t < 0.25 {
                Drop::Before
            } else if t > 0.75 {
                Drop::After
            } else {
                Drop::Into
            }
        };
        let pointer = ui.ctx().pointer_interact_pos();
        if let (Some(from), Some(pointer)) = (dragged.dnd_hover_payload::<NodePath>(), pointer)
            && !path.starts_with(&from)
        {
            let stroke = egui::Stroke::new(2.0, theme::ACCENT);
            let painter = ui.painter();
            match at(pointer) {
                Drop::Before => {
                    painter.hline(rect.x_range(), rect.top(), stroke);
                }
                Drop::After => {
                    painter.hline(rect.x_range(), rect.bottom(), stroke);
                }
                Drop::Into => {
                    painter.rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
                }
            }
        }
        if let (Some(from), Some(pointer)) = (dragged.dnd_release_payload::<NodePath>(), pointer) {
            let (&index, parent) = path.split_last().expect("nodes have paths");
            let (parent, index) = match at(pointer) {
                Drop::Before => (parent.to_vec(), index),
                Drop::After => (parent.to_vec(), index + 1),
                Drop::Into => (path.clone(), usize::MAX),
            };
            actions.push(Action::Move {
                from: (*from).clone(),
                parent,
                index,
            });
        }
    }
}

/// The kinds of node to add to the node at `parent`. Instances have a menu of their own.
fn add_menu(ui: &mut egui::Ui, parent: &[usize], addable: &Addable, actions: &mut Vec<Action>) {
    for kind in KINDS.into_iter().filter(|&kind| kind != "instance") {
        let enabled = match kind {
            "sprite" => addable.sheets,
            "tilemap" => addable.tilesets,
            _ => true,
        };
        let text = format!("{} {kind}", theme::kind_icon(kind));
        let button = ui.add_enabled(enabled, egui::Button::new(text));
        let button = match kind {
            "sprite" => button.on_disabled_hover_text("Add a sheet to the scene first"),
            "tilemap" => button.on_disabled_hover_text("The game has no tileset files"),
            _ => button,
        };
        if button.clicked() {
            actions.push(Action::Add {
                parent: parent.to_vec(),
                kind,
            });
        }
    }
}

/// The scene files to place in the node at `parent`.
fn instance_menu(
    ui: &mut egui::Ui,
    parent: &[usize],
    addable: &Addable,
    actions: &mut Vec<Action>,
) {
    for scene in addable.scenes {
        if ui.button(scene).clicked() {
            actions.push(Action::Instance {
                parent: parent.to_vec(),
                scene: scene.clone(),
            });
        }
    }
}

/// Whether `node`, or a node inside it, has `filter` in its name.
fn matches(node: &NodeDef, filter: &str) -> bool {
    filter.is_empty()
        || node.name.to_lowercase().contains(filter)
        || node.children.iter().any(|child| matches(child, filter))
}
