//! The `ui` table, for menus over scenes' nodes.

use std::{collections::HashSet, rc::Rc};

use mlua::{Lua, Table, UserDataRef};

use super::Lent;
use crate::{
    lua::SceneNode,
    math::Rect,
    menu::{Menu, MenuEvent, MenuItem},
    scene::NodeId,
};

pub(super) fn module(lua: &Lua, lent: &Lent) -> mlua::Result<Table> {
    let ui = lua.create_table()?;
    ui.set("menu", {
        let lent = lent.clone();
        lua.create_function(
            move |lua, (node, options): (UserDataRef<SceneNode>, Option<Table>)| {
                let mut menu = ScriptMenu {
                    node: node.clone(),
                    tag: None,
                    menu: Menu::new(),
                    selected: None,
                    disabled: HashSet::new(),
                    lent: lent.clone(),
                };
                if let Some(options) = options {
                    menu.configure(lua, options)?;
                }
                // So the item under a resting cursor isn't selected until it moves. In `draw`,
                // the menu can't tell, so its first update takes the cursor as having moved.
                lent.context(|ctx| menu.menu.track_cursor(ctx.input())).ok();
                Ok(menu)
            },
        )?
    })?;
    ui.set_readonly(true);
    Ok(ui)
}

/// A [`Menu`] over a node's items, as scripts hold it: its children, or the nodes inside it with a
/// tag, those shown, where they're drawn.
struct ScriptMenu {
    node: SceneNode,
    tag: Option<String>,
    menu: Menu,
    /// The item selected when the items were last found, which stays selected while it's among
    /// them, as items come and go.
    selected: Option<NodeId>,
    /// The items that can't be picked.
    disabled: HashSet<NodeId>,
    lent: Lent,
}

impl ScriptMenu {
    fn configure(&mut self, lua: &Lua, options: Table) -> mlua::Result<()> {
        for pair in options.pairs::<String, mlua::Value>() {
            let (key, value) = pair?;
            let actions = &mut self.menu.actions;
            let action = match key.as_str() {
                "tag" => {
                    self.tag = Some(lua.unpack(value)?);
                    continue;
                }
                "wrap" => {
                    self.menu.wrap = lua.unpack(value)?;
                    continue;
                }
                "mouse" => {
                    self.menu.mouse = lua.unpack(value)?;
                    continue;
                }
                "up" => &mut actions.up,
                "down" => &mut actions.down,
                "left" => &mut actions.left,
                "right" => &mut actions.right,
                "confirm" => &mut actions.confirm,
                "cancel" => &mut actions.cancel,
                _ => {
                    return Err(mlua::Error::runtime(format!(
                        "a menu's options are `tag`, `wrap`, `mouse`, and the actions `up`, \
                         `down`, `left`, `right`, `confirm` and `cancel`, not `{key}`"
                    )));
                }
            };
            *action = lua.unpack(value)?;
        }
        Ok(())
    }

    /// The menu's items as they are now, and where each is, with the selection kept on the item it
    /// was on, if that's among them, and on one that can be picked.
    fn refresh(&mut self) -> mlua::Result<(Vec<NodeId>, Vec<MenuItem>)> {
        let (ids, items) = self.node.read(|scene, root| {
            let ids: Vec<NodeId> = match &self.tag {
                Some(tag) => scene.tagged(root, tag),
                None => scene.node(root).children().to_vec(),
            };
            let ids: Vec<NodeId> = ids
                .into_iter()
                .filter(|&id| id != root && scene.is_shown(id))
                .collect();
            let items: Vec<MenuItem> = ids
                .iter()
                .map(|&id| MenuItem {
                    // An item that covers nothing is where its origin is.
                    rect: scene.extent(id).unwrap_or_else(|| {
                        let [x, y] = scene.transform(id).translation;
                        Rect::new(x, y, 0.0, 0.0)
                    }),
                    enabled: !self.disabled.contains(&id),
                })
                .collect();
            (ids, items)
        })?;
        // Gone, it leaves the selection where it was among them, on the item that took its place.
        if let Some(index) = self
            .selected
            .and_then(|selected| ids.iter().position(|&id| id == selected))
        {
            self.menu.select(Some(index));
        }
        self.menu.refresh(&items);
        self.selected = self.menu.selected().map(|index| ids[index]);
        Ok((ids, items))
    }

    /// The item numbered `number`, counting from 1, among `ids`.
    fn item(ids: &[NodeId], number: i64) -> mlua::Result<NodeId> {
        usize::try_from(number)
            .ok()
            .and_then(|number| number.checked_sub(1))
            .and_then(|index| ids.get(index).copied())
            .ok_or_else(|| {
                mlua::Error::runtime(format!(
                    "the menu has no item {number}: it has {} shown",
                    ids.len()
                ))
            })
    }
}

impl mlua::UserData for ScriptMenu {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("update", |_, menu, ()| {
            let (ids, items) = menu.refresh()?;
            let lent = menu.lent.clone();
            let event = lent.context(|ctx| menu.menu.update(&items, ctx.input()))?;
            menu.selected = menu.menu.selected().map(|index| ids[index]);
            Ok(match event {
                Some(MenuEvent::Moved(index)) => (Some("moved"), Some(index + 1)),
                Some(MenuEvent::Picked(index)) => (Some("picked"), Some(index + 1)),
                Some(MenuEvent::Cancelled) => (Some("cancelled"), None),
                None => (None, None),
            })
        });
        methods.add_method_mut("selected", |_, menu, ()| {
            menu.refresh()?;
            Ok(menu.menu.selected().map(|index| index + 1))
        });
        methods.add_method_mut("select", |_, menu, number: i64| {
            let (ids, _) = menu.refresh()?;
            menu.selected = Some(ScriptMenu::item(&ids, number)?);
            menu.refresh()?;
            Ok(())
        });
        methods.add_method_mut("items", |_, menu, ()| {
            let (ids, _) = menu.refresh()?;
            let scene = menu.node.scene();
            Ok(ids
                .into_iter()
                .map(|id| SceneNode::new(Rc::clone(scene), id))
                .collect::<Vec<_>>())
        });
        methods.add_method_mut("set_enabled", |_, menu, (number, enabled): (i64, bool)| {
            let (ids, _) = menu.refresh()?;
            let id = ScriptMenu::item(&ids, number)?;
            if enabled {
                menu.disabled.remove(&id);
            } else {
                menu.disabled.insert(id);
            }
            Ok(())
        });
        methods.add_method_mut("is_enabled", |_, menu, number: i64| {
            let (ids, _) = menu.refresh()?;
            Ok(!menu.disabled.contains(&ScriptMenu::item(&ids, number)?))
        });
    }
}
