//! Nodes that instances place: what each is in the scene file it's from, with the overrides of any
//! instances on the way to it made, so the Inspector shows it as the scene would before the
//! instance's own overrides, and tells which fields those change.

use std::collections::BTreeMap;

use wingine::scene::{Assets, KindDef, NodeDef, SheetDef, Value};

/// How deeply instances may place scenes that place others, which a scene placing itself would
/// otherwise make endless.
const MAX_DEPTH: usize = 32;

/// A node an instance places, as the scene it's in makes it.
pub struct Placed {
    pub node: NodeDef,
    /// The sheets of the file it's from, which its `sheet` names one of.
    pub sheets: BTreeMap<String, SheetDef>,
    /// The columns and rows of the tilemap it's a layer of, if it's a layer.
    pub tilemap: Option<[u32; 2]>,
}

impl Placed {
    /// The node with an instance's overrides of it.
    pub fn overridden(&self, change: &BTreeMap<String, Value>) -> Result<NodeDef, String> {
        self.node.overridden(change, &self.sheets, self.tilemap)
    }
}

/// The node at `names`, a path of names from the top of the scene file `scene`, through any
/// instances on the way, with their overrides of it made.
pub fn placed(assets: &mut dyn Assets, scene: &str, names: &str) -> Result<Placed, String> {
    placed_in(assets, scene, names, 0)
}

fn placed_in(
    assets: &mut dyn Assets,
    scene: &str,
    names: &str,
    depth: usize,
) -> Result<Placed, String> {
    if depth > MAX_DEPTH {
        return Err(format!("`{scene}` places itself, through its instances"));
    }
    let file = assets.scene(scene).map_err(|err| err.to_string())?;
    let missing = || format!("`{scene}` has no node `{names}`");
    let parts: Vec<&str> = names.split('/').collect();
    let mut nodes = &file.children;
    let mut tilemap = None;
    for (i, name) in parts.iter().enumerate() {
        let node = nodes
            .iter()
            .find(|node| node.name == *name)
            .ok_or_else(missing)?;
        let Some(next) = parts.get(i + 1) else {
            return Ok(Placed {
                node: node.clone(),
                sheets: file.sheets.clone(),
                tilemap,
            });
        };
        // The nodes inside an instance are the scene's it places, then its own.
        if let KindDef::Instance(instance) = &node.kind
            && !node.children.iter().any(|child| child.name == *next)
        {
            let rest = parts[i + 1..].join("/");
            let mut inner = placed_in(assets, &instance.scene, &rest, depth + 1)?;
            if let Some(change) = instance.overrides.get(&rest) {
                inner.node = inner.overridden(change)?;
            }
            return Ok(inner);
        }
        tilemap = match &node.kind {
            KindDef::Tilemap(map) => Some([map.columns, map.rows]),
            _ => None,
        };
        nodes = &node.children;
    }
    Err(missing())
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use wingine::{
        Result,
        graphics::{Color, Texture},
        scene::{SceneFile, TilesetFile},
    };

    use super::*;

    /// Scene files held in memory.
    struct Files(BTreeMap<&'static str, &'static str>);

    impl Assets for Files {
        fn texture(&mut self, path: &str) -> Result<Texture> {
            unreachable!("no images are read: {path}")
        }

        fn scene(&mut self, path: &str) -> Result<Rc<SceneFile>> {
            SceneFile::parse(self.0[path], path).map(Rc::new)
        }

        fn tileset(&mut self, path: &str) -> Result<Rc<TilesetFile>> {
            unreachable!("no tilesets are read: {path}")
        }
    }

    fn files() -> Files {
        Files(
            [
                (
                    "hat.scene.luau",
                    r##"return { format = 1, children = {
                        { name = "brim", kind = "rect", width = 8, height = 2, color = "#202020" },
                    } }"##,
                ),
                (
                    "elder.scene.luau",
                    r##"return { format = 1, children = {
                        { name = "body", kind = "node", children = {
                            { name = "hat", kind = "instance", scene = "hat.scene.luau",
                              overrides = { brim = { color = "#a04040" } },
                              children = { { name = "feather", kind = "node", x = 3 } } },
                        } },
                    } }"##,
                ),
            ]
            .into(),
        )
    }

    #[test]
    fn nodes_inside_instances_have_their_overrides() {
        let mut files = files();
        let body = placed(&mut files, "elder.scene.luau", "body").unwrap();
        assert!(matches!(body.node.kind, KindDef::Node));

        // The brim, through the hat, which recolors it.
        let brim = placed(&mut files, "elder.scene.luau", "body/hat/brim").unwrap();
        let KindDef::Rect(rect) = &brim.node.kind else {
            panic!("the brim is a rect");
        };
        assert_eq!(rect.color, Color::rgba8(0xa0, 0x40, 0x40, 0xff));

        // The hat's own child.
        let feather = placed(&mut files, "elder.scene.luau", "body/hat/feather").unwrap();
        assert_eq!(feather.node.x, 3.0);

        let err = placed(&mut files, "elder.scene.luau", "body/cane")
            .err()
            .unwrap();
        assert_eq!(err, "`elder.scene.luau` has no node `body/cane`");
    }
}
