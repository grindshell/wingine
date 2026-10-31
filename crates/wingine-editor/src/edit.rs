//! Changes to a scene file's tree of nodes: adding, removing, duplicating, renaming and moving
//! them. Nodes are addressed by [`NodePath`]s, so these work on the file alone, without a scene
//! loaded to play.

use wingine::{
    graphics::{Align, Color},
    scene::{
        AreaDef, InstanceDef, KindDef, NodeDef, RectDef, SceneFile, SpriteDef, TextDef,
        TileLayerDef, TilemapDef,
    },
};

/// Where a node is in a scene file: the index of each node on the way down from the top, so
/// `[2, 0]` is the first child of the file's third node. The empty path is the scene itself, the
/// root that holds the file's sheets and props.
pub type NodePath = Vec<usize>;

/// The kinds of node that can be added, as scene files name them.
pub const KINDS: [&str; 7] = [
    "node", "sprite", "rect", "text", "area", "tilemap", "instance",
];

/// The node at `path`, if there's one. The empty path has none.
pub fn node<'a>(file: &'a SceneFile, path: &[usize]) -> Option<&'a NodeDef> {
    let (&first, rest) = path.split_first()?;
    rest.iter()
        .try_fold(file.children.get(first)?, |node, &i| node.children.get(i))
}

pub fn node_mut<'a>(file: &'a mut SceneFile, path: &[usize]) -> Option<&'a mut NodeDef> {
    let (&first, rest) = path.split_first()?;
    rest.iter()
        .try_fold(file.children.get_mut(first)?, |node, &i| {
            node.children.get_mut(i)
        })
}

/// The children of the node at `path`, or the nodes at the top of the scene for the empty path.
pub fn children<'a>(file: &'a SceneFile, path: &[usize]) -> Option<&'a Vec<NodeDef>> {
    if path.is_empty() {
        return Some(&file.children);
    }
    node(file, path).map(|node| &node.children)
}

pub fn children_mut<'a>(file: &'a mut SceneFile, path: &[usize]) -> Option<&'a mut Vec<NodeDef>> {
    if path.is_empty() {
        return Some(&mut file.children);
    }
    node_mut(file, path).map(|node| &mut node.children)
}

/// The names on the way down to the node at `path`, separated by `/`, as scenes find nodes.
pub fn name_path(file: &SceneFile, path: &[usize]) -> Option<String> {
    let mut names = Vec::with_capacity(path.len());
    let mut nodes = &file.children;
    for &i in path {
        let node = nodes.get(i)?;
        names.push(node.name.as_str());
        nodes = &node.children;
    }
    Some(names.join("/"))
}

/// The path of the deepest node of the file on the way to `names`, a path of names such as a
/// scene gives its nodes: the node itself, or for a node an instance placed, the instance.
pub fn path_of(file: &SceneFile, names: &str) -> NodePath {
    let mut path = Vec::new();
    let mut nodes = &file.children;
    for name in names.split('/').filter(|name| !name.is_empty()) {
        let Some(i) = nodes.iter().position(|node| node.name == name) else {
            break;
        };
        path.push(i);
        nodes = &nodes[i].children;
    }
    path
}

/// Whether `name` can name a node among `siblings`, apart from the one at `except`: it isn't
/// empty, has no `/`, and no other sibling has it.
pub fn check_name(siblings: &[NodeDef], name: &str, except: Option<usize>) -> Result<(), String> {
    if name.is_empty() {
        return Err("a node needs a name".into());
    }
    if name.contains('/') {
        return Err(format!(
            "`{name}` has a `/`, which separates the names in paths"
        ));
    }
    let taken = siblings
        .iter()
        .enumerate()
        .any(|(i, sibling)| Some(i) != except && sibling.name == name);
    if taken {
        return Err(format!("another node here is named `{name}`"));
    }
    Ok(())
}

/// `name`, or if a sibling has it, `name` with the first number from 2 up that makes it unique.
/// A name that ends in a number counts on from it, so copies of `tree2` are `tree3` and up.
pub fn unique_name(siblings: &[NodeDef], name: &str) -> String {
    let taken = |name: &str| siblings.iter().any(|sibling| sibling.name == name);
    if !taken(name) {
        return name.to_owned();
    }
    let stem = name.trim_end_matches(|c: char| c.is_ascii_digit());
    let start = name[stem.len()..].parse::<u32>().map_or(2, |n| n + 1);
    (start..)
        .map(|n| format!("{stem}{n}"))
        .find(|name| !taken(name))
        .expect("some number is free")
}

/// Adds `node` as the last child of the node at `parent`, renamed if a sibling has its name, and
/// returns its path.
pub fn add(file: &mut SceneFile, parent: &[usize], mut node: NodeDef) -> Result<NodePath, String> {
    if matches!(node.kind, KindDef::TileLayer(_))
        && !matches!(
            self::node(file, parent).map(|p| &p.kind),
            Some(KindDef::Tilemap(_))
        )
    {
        return Err("a tile layer goes in a tilemap".into());
    }
    let siblings = children_mut(file, parent).ok_or("there's no such node")?;
    node.name = unique_name(siblings, &node.name);
    siblings.push(node);
    let mut path = parent.to_vec();
    path.push(siblings.len() - 1);
    Ok(path)
}

/// Removes the node at `path` and the nodes inside it, and returns it.
pub fn remove(file: &mut SceneFile, path: &[usize]) -> Option<NodeDef> {
    let (&index, parent) = path.split_last()?;
    let siblings = children_mut(file, parent)?;
    (index < siblings.len()).then(|| siblings.remove(index))
}

/// Puts a copy of the node at `path` right after it, named apart from its siblings, and returns
/// the copy's path.
pub fn duplicate(file: &mut SceneFile, path: &[usize]) -> Option<NodePath> {
    let (&index, parent) = path.split_last()?;
    let siblings = children_mut(file, parent)?;
    let mut copy = siblings.get(index)?.clone();
    copy.name = unique_name(siblings, &copy.name);
    siblings.insert(index + 1, copy);
    let mut copy_path = path.to_vec();
    *copy_path.last_mut().expect("the path isn't empty") += 1;
    Some(copy_path)
}

/// Renames the node at `path`, and the tracks of clips that pose it or nodes inside it.
pub fn rename(file: &mut SceneFile, path: &[usize], name: &str) -> Result<(), String> {
    let (&index, parent) = path
        .split_last()
        .ok_or("the scene has no name of its own")?;
    let old = name_path(file, path).ok_or("there's no such node")?;
    let siblings = children_mut(file, parent).ok_or("there's no such node")?;
    check_name(siblings, name, Some(index))?;
    siblings[index].name = name.to_owned();
    let new = name_path(file, path).expect("the node is still there");
    retarget_clips(file, &old, &new);
    Ok(())
}

/// Moves the node at `from` to be the child of the node at `parent` at `index` among its
/// children, counted as they are before the move, renamed if a new sibling has its name. Clips'
/// tracks follow it. Returns its new path.
pub fn move_node(
    file: &mut SceneFile,
    from: &[usize],
    parent: &[usize],
    index: usize,
) -> Result<NodePath, String> {
    if from.is_empty() {
        return Err("the scene itself can't move".into());
    }
    if parent.starts_with(from) {
        return Err("a node can't go inside itself".into());
    }
    let old = name_path(file, from).ok_or("there's no such node")?;
    let moved = node(file, from).ok_or("there's no such node")?;
    if matches!(moved.kind, KindDef::TileLayer(_))
        && !matches!(
            node(file, parent).map(|p| &p.kind),
            Some(KindDef::Tilemap(_))
        )
    {
        return Err("a tile layer goes in a tilemap".into());
    }
    let (&from_index, from_parent) = from.split_last().expect("the path isn't empty");
    let siblings = children(file, parent).ok_or("there's no such node")?;
    let index = index.min(siblings.len());

    // Where the parent and the index are once the node is out of the way.
    let mut parent = parent.to_vec();
    if parent.len() > from_parent.len()
        && parent.starts_with(from_parent)
        && parent[from_parent.len()] > from_index
    {
        parent[from_parent.len()] -= 1;
    }
    let same_parent = from_parent == parent.as_slice();
    let index = if same_parent && index > from_index {
        index - 1
    } else {
        index
    };

    let mut node = remove(file, from).expect("the node is there");
    let siblings = children_mut(file, &parent).expect("the parent is still there");
    if !same_parent {
        node.name = unique_name(siblings, &node.name);
    }
    siblings.insert(index, node);
    parent.push(index);
    let new = name_path(file, &parent).expect("the node is there");
    retarget_clips(file, &old, &new);
    Ok(parent)
}

/// Points the tracks of the file's clips at a node, or a node inside it, that moved from `old` to
/// `new`, both paths of names.
fn retarget_clips(file: &mut SceneFile, old: &str, new: &str) {
    for clip in file.clips.values_mut() {
        for track in &mut clip.tracks {
            if track.node == old {
                track.node = new.to_owned();
            } else if let Some(rest) = track.node.strip_prefix(old)
                && rest.starts_with('/')
            {
                track.node = format!("{new}{rest}");
            }
        }
    }
}

/// A new node of `kind`, one of [`KINDS`], with what it shows set to something visible: a sprite
/// from `sheet`, a tilemap of `tileset` with a layer, an instance of `scene`.
pub fn new_node(kind: &str, sheet: &str, tileset: &str, scene: &str) -> NodeDef {
    let kind_def = match kind {
        "sprite" => KindDef::Sprite(SpriteDef {
            sheet: sheet.to_owned(),
            cell: [0, 0],
            size: None,
            stretch: [0.0, 0.0],
            pivot: [0.0, 0.0],
            tint: Color::WHITE,
        }),
        "rect" => KindDef::Rect(RectDef {
            size: [16.0, 16.0],
            stretch: [0.0, 0.0],
            pivot: [0.0, 0.0],
            color: Color::WHITE,
        }),
        "text" => KindDef::Text(TextDef {
            text: "Text".into(),
            font_scale: 1.0,
            wrap: None,
            align: Align::Left,
            pivot: [0.0, 0.0],
            color: Color::WHITE,
        }),
        "area" => KindDef::Area(AreaDef {
            size: [16.0, 16.0],
            stretch: [0.0, 0.0],
            pivot: [0.0, 0.0],
        }),
        "tilemap" => KindDef::Tilemap(TilemapDef {
            tileset: tileset.to_owned(),
            columns: 8,
            rows: 8,
        }),
        "instance" => KindDef::Instance(InstanceDef {
            scene: scene.to_owned(),
            overrides: Default::default(),
        }),
        _ => KindDef::Node,
    };
    let mut node = NodeDef::new(kind, kind_def);
    if let KindDef::Tilemap(map) = &node.kind {
        node.children
            .push(tile_layer("ground", map.columns, map.rows));
    }
    if kind == "instance" {
        // Named for the scene it places, as `tree` for `tree.scene.luau`.
        let file_name = scene.rsplit('/').next().unwrap_or(scene);
        let name = file_name.split('.').next().unwrap_or(file_name);
        if !name.is_empty() {
            node.name = name.to_owned();
        }
    }
    node
}

/// An empty tile layer of a map `columns` by `rows` in size.
pub fn tile_layer(name: &str, columns: u32, rows: u32) -> NodeDef {
    NodeDef::new(
        name,
        KindDef::TileLayer(TileLayerDef {
            legend: Default::default(),
            tiles: vec![" ".repeat(columns as usize); rows as usize],
        }),
    )
}

/// Resizes the layers of the tilemap at `path` to `columns` by `rows`, keeping the cells that
/// are still on the map and leaving new ones empty.
pub fn resize_tilemap(file: &mut SceneFile, path: &[usize], columns: u32, rows: u32) {
    let Some(node) = node_mut(file, path) else {
        return;
    };
    let KindDef::Tilemap(map) = &mut node.kind else {
        return;
    };
    map.columns = columns;
    map.rows = rows;
    for child in &mut node.children {
        if let KindDef::TileLayer(layer) = &mut child.kind {
            layer.tiles.resize(rows as usize, String::new());
            for row in &mut layer.tiles {
                let mut cells: Vec<char> = row.chars().collect();
                cells.resize(columns as usize, ' ');
                *row = cells.into_iter().collect();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `a` holding `b` and `c`, then `d`, and a clip posing `a/b` and `d`.
    fn file() -> SceneFile {
        SceneFile::parse(
            r#"return {
                format = 1,
                children = {
                    {
                        name = "a", kind = "node",
                        children = {
                            { name = "b", kind = "node" },
                            { name = "c", kind = "node" },
                        },
                    },
                    { name = "d", kind = "node" },
                },
                clips = {
                    wave = {
                        duration = 1,
                        tracks = {
                            { node = "a/b", property = "x", keys = { { 0, 0 } } },
                            { node = "d", property = "x", keys = { { 0, 0 } } },
                        },
                    },
                },
            }"#,
            "test.scene.luau",
        )
        .unwrap()
    }

    fn names(file: &SceneFile, parent: &[usize]) -> Vec<String> {
        children(file, parent)
            .unwrap()
            .iter()
            .map(|node| node.name.clone())
            .collect()
    }

    fn tracks(file: &SceneFile) -> Vec<&str> {
        file.clips["wave"]
            .tracks
            .iter()
            .map(|track| track.node.as_str())
            .collect()
    }

    #[test]
    fn paths_lead_to_nodes() {
        let file = file();
        assert_eq!(node(&file, &[0, 1]).unwrap().name, "c");
        assert!(node(&file, &[]).is_none());
        assert!(node(&file, &[0, 2]).is_none());
        assert_eq!(name_path(&file, &[0, 1]).unwrap(), "a/c");
        assert_eq!(path_of(&file, "a/c"), [0, 1]);
        // Inside an instance, nodes come from another file: the deepest found is the answer.
        assert_eq!(path_of(&file, "d/body/hat"), [1]);
    }

    #[test]
    fn names_stay_unique_among_siblings() {
        let mut file = file();
        let siblings = &file.children;
        assert_eq!(unique_name(siblings, "e"), "e");
        assert_eq!(unique_name(siblings, "a"), "a2");
        assert!(check_name(siblings, "a", None).is_err());
        assert!(check_name(siblings, "a", Some(0)).is_ok());
        assert!(check_name(siblings, "x/y", None).is_err());
        assert!(check_name(siblings, "", None).is_err());

        let copy = duplicate(&mut file, &[1]).unwrap();
        assert_eq!(copy, [2]);
        let again = duplicate(&mut file, &[2]).unwrap();
        assert_eq!(names(&file, &[]), ["a", "d", "d2", "d3"]);
        assert_eq!(again, [3]);
    }

    #[test]
    fn adding_and_removing() {
        let mut file = file();
        let added = add(&mut file, &[0], NodeDef::new("b", KindDef::Node)).unwrap();
        assert_eq!(added, [0, 2]);
        assert_eq!(names(&file, &[0]), ["b", "c", "b2"]);
        assert!(add(&mut file, &[], tile_layer("ground", 2, 2)).is_err());

        let removed = remove(&mut file, &[0, 0]).unwrap();
        assert_eq!(removed.name, "b");
        assert_eq!(names(&file, &[0]), ["c", "b2"]);
        assert!(remove(&mut file, &[5]).is_none());
    }

    #[test]
    fn renaming_retargets_clips() {
        let mut file = file();
        rename(&mut file, &[0], "arm").unwrap();
        assert_eq!(tracks(&file), ["arm/b", "d"]);
        assert!(rename(&mut file, &[1], "arm").is_err());
        rename(&mut file, &[1], "d").unwrap();
    }

    #[test]
    fn moving_reparents_and_reorders() {
        let mut file = file();
        // `d` into `a`, between `b` and `c`.
        let moved = move_node(&mut file, &[1], &[0], 1).unwrap();
        assert_eq!(moved, [0, 1]);
        assert_eq!(names(&file, &[0]), ["b", "d", "c"]);
        assert_eq!(tracks(&file), ["a/b", "a/d"]);

        // `b` to the top, after `a`.
        let moved = move_node(&mut file, &[0, 0], &[], 1).unwrap();
        assert_eq!(moved, [1]);
        assert_eq!(names(&file, &[]), ["a", "b"]);
        assert_eq!(tracks(&file), ["b", "a/d"]);

        // Down among its own siblings: before `c` is index 2 counted before the move.
        let mut file = self::file();
        let moved = move_node(&mut file, &[0, 0], &[0], 2).unwrap();
        assert_eq!(moved, [0, 1]);
        assert_eq!(names(&file, &[0]), ["c", "b"]);

        // Out of a node above the parent it goes into: `c` into `d`, after `a` loses it.
        let moved = move_node(&mut file, &[0, 0], &[1], 0).unwrap();
        assert_eq!(moved, [1, 0]);

        assert!(move_node(&mut file, &[0], &[0, 0], 0).is_err());
    }

    #[test]
    fn moving_renames_clashes_in_the_new_parent() {
        let mut file = file();
        let copy = duplicate(&mut file, &[0, 0]).unwrap();
        let moved = move_node(&mut file, &copy, &[], 0).unwrap();
        assert_eq!(moved, [0]);
        assert_eq!(names(&file, &[]), ["b2", "a", "d"]);
        let moved = move_node(&mut file, &[1, 0], &[], 0).unwrap();
        assert_eq!(node(&file, &moved).unwrap().name, "b");
    }

    #[test]
    fn new_nodes_show_something() {
        let sprite = new_node("sprite", "tiles", "", "");
        assert!(matches!(&sprite.kind, KindDef::Sprite(def) if def.sheet == "tiles"));
        let map = new_node("tilemap", "", "world.tileset.luau", "");
        assert_eq!(map.children.len(), 1);
        let KindDef::TileLayer(layer) = &map.children[0].kind else {
            panic!("a tilemap starts with a layer");
        };
        assert_eq!(layer.tiles, vec!["        "; 8]);
        let tree = new_node("instance", "", "", "props/tree.scene.luau");
        assert_eq!(tree.name, "tree");
    }

    #[test]
    fn resizing_a_tilemap_keeps_its_cells() {
        let mut file = SceneFile::default();
        let mut map = new_node("tilemap", "", "world.tileset.luau", "");
        let KindDef::TileLayer(layer) = &mut map.children[0].kind else {
            unreachable!()
        };
        layer.tiles = vec!["ab".into(), "cd".into()];
        file.children.push(map);
        resize_tilemap(&mut file, &[0], 3, 1);
        let KindDef::TileLayer(layer) = &node(&file, &[0, 0]).unwrap().kind else {
            unreachable!()
        };
        assert_eq!(layer.tiles, ["ab "]);
    }
}
