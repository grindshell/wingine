//! Embedding a game's folder in a module, from its crate's build script.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// Embeds a game's folder in the crate whose build script calls this, for [`game!`](crate::game)
/// to give as a [`Game`](crate::Game): the folder `WINGINE_GAME` names if it's set, or else
/// `default`, either relative to the crate's manifest. Every file in it is embedded, but those in
/// folders whose names start with `.`, such as tests' `.test-output`.
///
/// # Panics
///
/// If it isn't called from a build script, or the folder has no `main.luau` or can't be read.
pub fn embed_game(default: impl AsRef<Path>) {
    println!("cargo:rerun-if-env-changed=WINGINE_GAME");
    let manifest = env::var_os("CARGO_MANIFEST_DIR").expect("embed_game runs in a build script");
    let game = env::var_os("WINGINE_GAME")
        .map(PathBuf::from)
        .unwrap_or_else(|| default.as_ref().to_owned());
    let game = std::path::absolute(Path::new(&manifest).join(game)).unwrap();
    assert!(
        game.join("main.luau").is_file(),
        "{} isn't a game's folder: it has no main.luau",
        game.display()
    );
    // Cargo looks through a folder for changes, so files added and removed count too.
    println!("cargo:rerun-if-changed={}", game.display());

    let mut files = Vec::new();
    collect(&game, &game, &mut files);
    files.sort();
    // `absolute` leaves `..` in a path on Unix, so the name comes from where the folder is.
    let name = fs::canonicalize(&game).unwrap();
    let name = name.file_name().unwrap().to_string_lossy();
    // A constant, so the files are `'static` wherever `game!` is used.
    let mut source = String::from("{\n    const GAME: ::wingine_web::Game = ::wingine_web::Game {\n");
    source += &format!("        name: {name:?},\n        files: &[\n");
    for (path, file) in &files {
        let file = file.display().to_string();
        source += &format!("            ({path:?}, include_bytes!({file:?})),\n");
    }
    source += "        ],\n    };\n    GAME\n}\n";
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("wingine_game.rs"), source).unwrap();
}

/// Adds every file in `folder` to `files`, as its path in `root` with `/` between names, and
/// where it is, but those in folders whose names start with `.`.
fn collect(root: &Path, folder: &Path, files: &mut Vec<(String, PathBuf)>) {
    for entry in fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if !path.file_name().unwrap().to_string_lossy().starts_with('.') {
                collect(root, &path, files);
            }
        } else {
            let relative = path.strip_prefix(root).unwrap();
            let names: Vec<_> = relative
                .components()
                .map(|name| name.as_os_str().to_string_lossy().into_owned())
                .collect();
            files.push((names.join("/"), path));
        }
    }
}
