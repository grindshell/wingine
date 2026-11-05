//! The Scene dock: the open scene's tree of nodes. Clicking selects, dragging moves a node into
//! another or between two, and its menus add, rename, duplicate and delete nodes. An instance
//! opens to show the nodes of the scene it places, in italics, to select and override; scenes
//! dragged in from the FileSystem dock are placed in the node they're dropped on.

use wingine::{
    gui::egui,
    scene::{KindDef, NodeDef, NodeId, Scene},
};

use crate::{
    docks::filesystem::ScenePath,
    document::Document,
    edit::{self, KINDS, KINDS_3D, NodePath},
    theme,
};

/// What the dock asks of the editor.
pub enum Action {
    Select(NodePath),
    /// Select the node at `names`, a path of names from the top of the scene the instance at
    /// `instance` places.
    SelectInner {
        instance: NodePath,
        names: String,
    },
    /// Add a node of this kind, one of [`KINDS`] or [`KINDS_3D`], as the last child of `parent`.
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
    /// Whether the game has model files.
    pub models: bool,
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
    /// The selection as the dock last showed it: a node of the file, and one inside it if it's an
    /// instance.
    shown: (Option<NodePath>, Option<String>),
    /// Whether the selection changed this frame, so the rows on the way to it open and it scrolls
    /// into view.
    reveal: bool,
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
        let selection = (selected.clone(), document.inner().map(str::to_owned));
        self.reveal = selection != self.shown;
        self.shown = selection;
        ui.horizontal(|ui| {
            ui.strong("Scene");
            ui.menu_button("➕", |ui| {
                let in_3d = edit::holds_3d(document.file(), &target);
                add_menu(ui, &target, in_3d, addable, &mut actions);
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
                    add_menu(ui, &[], false, addable, &mut actions);
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
            drop_scene(ui, &response, &[], &mut actions);
            let filter = self.filter.to_lowercase();
            ui.indent("nodes", |ui| {
                let mut path = Vec::new();
                self.nodes(
                    ui,
                    document,
                    &document.file().children,
                    &mut path,
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
        document: &Document,
        nodes: &[NodeDef],
        path: &mut NodePath,
        filter: &str,
        addable: &Addable,
        actions: &mut Vec<Action>,
    ) {
        // A node inside an instance shows as selected rather than the instance.
        let none = None;
        let selected = match document.inner() {
            Some(_) => &none,
            None => &document.selected,
        };
        for (i, node) in nodes.iter().enumerate() {
            if !matches(node, filter) {
                continue;
            }
            path.push(i);
            let id = ui.make_persistent_id(("node", path.as_slice()));
            // The nodes of the scene an instance places, which aren't in the file.
            let placed = match &node.kind {
                KindDef::Instance(_) => document
                    .preview
                    .as_ref()
                    .zip(document.live_node(path))
                    .filter(|(scene, id)| scene.node(*id).children().len() > node.children.len()),
                _ => None,
            };
            if node.children.is_empty() && placed.is_none() {
                ui.horizontal(|ui| {
                    // Lined up with the names of nodes that can open.
                    ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x);
                    self.row(ui, node, path, selected, addable, actions);
                });
            } else {
                // Instances start closed: what they place is usually left as it is.
                let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    id,
                    placed.is_none(),
                );
                // Open on the way to the selection.
                if self.reveal
                    && let Some(selected) = &document.selected
                    && selected.starts_with(path)
                    && (selected.len() > path.len() || document.inner().is_some())
                {
                    state.set_open(true);
                }
                state
                    .show_header(ui, |ui| {
                        self.row(ui, node, path, selected, addable, actions);
                    })
                    .body(|ui| {
                        if let Some((scene, live)) = placed {
                            let own: Vec<&str> = node
                                .children
                                .iter()
                                .map(|child| child.name.as_str())
                                .collect();
                            let inner = document.inner();
                            let children = scene.node(live).children();
                            let selected = document
                                .selected
                                .as_ref()
                                .filter(|selected| *selected == path)
                                .and(inner);
                            let reveal = self.reveal;
                            for &child in children
                                .iter()
                                .filter(|&&child| !own.contains(&scene.node(child).name()))
                            {
                                inner_nodes(ui, scene, child, path, "", selected, reveal, actions);
                            }
                        }
                        self.nodes(ui, document, &node.children, path, filter, addable, actions);
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
        if is_selected && self.reveal {
            dragged.scroll_to_me(None);
        }
        if dragged.double_clicked() {
            self.renaming = Some((path.clone(), node.name.clone()));
            self.focus_rename = true;
        }
        dragged.context_menu(|ui| {
            let in_3d = edit::holds_3d_node(node);
            ui.menu_button("Add Child Node", |ui| {
                add_menu(ui, path, in_3d, addable, actions);
            });
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
        drop_scene(ui, &dragged, path, actions);
    }
}

/// A scene dragged from the FileSystem dock over `response`, a node's row, to place in the node
/// at `parent`.
fn drop_scene(
    ui: &egui::Ui,
    response: &egui::Response,
    parent: &[usize],
    actions: &mut Vec<Action>,
) {
    if response.dnd_hover_payload::<ScenePath>().is_some() {
        let stroke = egui::Stroke::new(2.0, theme::ACCENT);
        ui.painter()
            .rect_stroke(response.rect, 2.0, stroke, egui::StrokeKind::Inside);
    }
    if let Some(scene) = response.dnd_release_payload::<ScenePath>() {
        actions.push(Action::Instance {
            parent: parent.to_vec(),
            scene: scene.0.clone(),
        });
    }
}

/// The row of `id`, a node an instance at `instance` placed, and the rows of the nodes inside it.
/// `names` is the path of names to its parent, from the top of the scene the instance places, and
/// `selected` the node selected inside the instance, if any, which `reveal` opens the rows on the
/// way to and scrolls to.
#[expect(clippy::too_many_arguments)]
fn inner_nodes(
    ui: &mut egui::Ui,
    scene: &Scene,
    id: NodeId,
    instance: &NodePath,
    names: &str,
    selected: Option<&str>,
    reveal: bool,
    actions: &mut Vec<Action>,
) {
    let node = scene.node(id);
    let names = if names.is_empty() {
        node.name().to_owned()
    } else {
        format!("{names}/{}", node.name())
    };
    let row = |ui: &mut egui::Ui, actions: &mut Vec<Action>| {
        ui.label(theme::kind_icon(node.kind().name()))
            .on_hover_text(format!("{}, placed by the instance", node.kind().name()));
        let text = egui::RichText::new(node.name()).italics();
        let text = if node.is_visible() { text } else { text.weak() };
        let is_selected = selected == Some(names.as_str());
        let response = ui.selectable_label(is_selected, text);
        if response.clicked() {
            actions.push(Action::SelectInner {
                instance: instance.clone(),
                names: names.clone(),
            });
        }
        if is_selected && reveal {
            response.scroll_to_me(None);
        }
    };
    if node.children().is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x);
            row(ui, actions);
        });
        return;
    }
    let state_id = ui.make_persistent_id(("inner", instance.as_slice(), names.as_str()));
    let mut state =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), state_id, false);
    if reveal && selected.is_some_and(|selected| selected.starts_with(&format!("{names}/"))) {
        state.set_open(true);
    }
    state.show_header(ui, |ui| row(ui, actions)).body(|ui| {
        for &child in node.children() {
            inner_nodes(
                ui, scene, child, instance, &names, selected, reveal, actions,
            );
        }
    });
}

/// The kinds of node to add to the node at `parent`: those of the 2D scene, or in a 3D view or a
/// node in one, if `in_3d`, those placed in 3D. Instances have a menu of their own.
fn add_menu(
    ui: &mut egui::Ui,
    parent: &[usize],
    in_3d: bool,
    addable: &Addable,
    actions: &mut Vec<Action>,
) {
    let kinds: &[&'static str] = if in_3d { &KINDS_3D } else { &KINDS };
    for &kind in kinds.iter().filter(|&&kind| kind != "instance") {
        let enabled = match kind {
            "sprite" | "billboard" => addable.sheets,
            "tilemap" => addable.tilesets,
            "model" => addable.models,
            _ => true,
        };
        let text = format!("{} {kind}", theme::kind_icon(kind));
        let button = ui.add_enabled(enabled, egui::Button::new(text));
        let button = match kind {
            "sprite" | "billboard" => {
                button.on_disabled_hover_text("Add a sheet to the scene first")
            }
            "tilemap" => button.on_disabled_hover_text("The game has no tileset files"),
            "model" => button.on_disabled_hover_text("The game has no model files, .glb or .gltf"),
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
