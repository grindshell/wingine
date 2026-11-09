//! Exporting a Luau game to ship to players, as `wingine export path/to/game out/` does: a folder
//! for the platform it runs on, which players open by double-clicking its executable.
//!
//! ```text
//! out/
//! ├── pong.exe     the wingine that exported it, named by the game's identity (no .exe on Linux)
//! ├── game/        the game's folder, without hidden files and folders, such as .luaurc and
//! │                .test-output, or tests (*.test.luau)
//! └── licenses/    the engine's licenses, and the crates' it's built of, which ship with it
//! ```
//!
//! The executable, started without a game's folder, runs the folder called `game` beside it (see
//! [`shipped_game`](super::shipped_game)), and shows any error loading it in its window, as players
//! have no console to read it in. On Windows, the copy opens no console window behind the game:
//! its header says it's a program of Windows' GUI subsystem, not the console's
//! ([`gui_subsystem`]), as `editbin /SUBSYSTEM:WINDOWS` would make it. And if the game has an
//! `icon`, the copy shows it in Explorer and in shortcuts to it: Windows' own resource functions
//! write it into the copy's resources, at the sizes Windows shows icons at, each a PNG.
//!
//! Export a release build of wingine (`cargo run --release -- export ...`, or the release
//! archives'), since debug builds run games far slower, and export on each platform the game
//! ships for, with its native libraries built for each.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{SHIPPED_FOLDER, config, native::library_path};
#[cfg(any(windows, test))]
use crate::WindowIcon;
use crate::{Error, Result};

/// The engine's licenses, which games built on it ship.
const LICENSE_MIT: &str = include_str!("../../LICENSE-MIT");
const LICENSE_APACHE: &str = include_str!("../../LICENSE-APACHE");
/// The licenses of the crates built into wingine, beside it in the release archives.
const NOTICES: &str = "THIRD-PARTY-NOTICES.txt";
/// The end of the names of tests' files, which don't ship.
const TEST_SUFFIX: &str = ".test.luau";

/// What [`export`] made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exported {
    /// The folder, ready to zip and ship.
    pub folder: PathBuf,
    /// The executable in it that players start.
    pub executable: PathBuf,
    /// How many of the game's files it copied.
    pub files: usize,
    /// What might be wrong with the export, though it was made: each a sentence saying what to do.
    pub warnings: Vec<String>,
}

/// Exports the Luau game in `game` to the folder `out`, as the module docs lay it out, with
/// `executable`, a wingine, which is usually the one running (`std::env::current_exe()`).
///
/// It fails without writing anything if the game's `conf.luau` doesn't read, it has no
/// `main.luau`, or `out` exists and isn't an empty folder.
pub fn export(
    game: impl AsRef<Path>,
    out: impl AsRef<Path>,
    executable: impl AsRef<Path>,
) -> Result<Exported> {
    let (game, out, executable) = (game.as_ref(), out.as_ref(), executable.as_ref());
    let refuse = |message: String| Error::Export {
        game: game.to_owned(),
        message,
    };
    let config = config(game)?;
    if !game.join("main.luau").is_file() {
        return Err(refuse("it has no main.luau".into()));
    }
    let occupied = match fs::read_dir(out) {
        Ok(mut entries) => entries.next().is_some(),
        Err(_) => out.exists(),
    };
    if occupied {
        let message = format!(
            "{} is already there and isn't an empty folder: export into a new one",
            out.display()
        );
        return Err(refuse(message));
    }

    fs::create_dir_all(out).map_err(|source| write_error(out, source))?;
    // The export may be inside the game's folder, which mustn't copy itself.
    let skip = fs::canonicalize(out).map_err(|source| read_error(out, source))?;
    let mut files = 0;
    copy_game(game, &out.join(SHIPPED_FOLDER), &skip, &mut files)?;

    let name = format!("{}{}", config.window.identity, std::env::consts::EXE_SUFFIX);
    let copied = out.join(name);
    fs::copy(executable, &copied).map_err(|source| read_error(executable, source))?;
    if cfg!(windows) {
        let mut bytes = fs::read(&copied).map_err(|source| read_error(&copied, source))?;
        gui_subsystem(&mut bytes).map_err(|problem| {
            refuse(format!(
                "{} can't be shipped: {problem}",
                executable.display()
            ))
        })?;
        fs::write(&copied, bytes).map_err(|source| write_error(&copied, source))?;
    }
    #[cfg(windows)]
    {
        if let Some(icon) = &config.window.icon {
            set_icon(&copied, icon).map_err(|source| write_error(&copied, source))?;
        }
    }

    let mut warnings = Vec::new();
    let licenses = out.join("licenses");
    fs::create_dir_all(&licenses).map_err(|source| write_error(&licenses, source))?;
    for (name, text) in [
        ("wingine-MIT.txt", LICENSE_MIT),
        ("wingine-APACHE.txt", LICENSE_APACHE),
    ] {
        let path = licenses.join(name);
        fs::write(&path, text).map_err(|source| write_error(&path, source))?;
    }
    let notices = executable.with_file_name(NOTICES);
    if notices.is_file() {
        let path = licenses.join(NOTICES);
        fs::copy(&notices, &path).map_err(|source| write_error(&path, source))?;
    } else {
        warnings.push(format!(
            "there's no {NOTICES} beside {}, the licenses of the crates wingine is built of, which \
             ship with the game: the release archives have it, and `cargo about` makes it, as the \
             repository's release.yml does",
            executable.display()
        ));
    }
    if cfg!(debug_assertions) {
        warnings.push(
            "this wingine is a debug build, which runs games far slower: export with a release \
             build, `cargo run --release -- export ...`"
                .into(),
        );
    }
    for library in config.libraries.iter().filter(|library| !library.optional) {
        if !library_path(game, &library.name).is_file() {
            warnings.push(format!(
                "the game needs the native library `{}`, which isn't built for this platform in \
                 its native folder, so it won't start",
                library.name
            ));
        }
    }
    Ok(Exported {
        folder: out.to_owned(),
        executable: copied,
        files,
        warnings,
    })
}

/// Copies the game's folder `from` to `to`, but for hidden files and folders, tests, and `skip`,
/// counting the files in `files`.
fn copy_game(from: &Path, to: &Path, skip: &Path, files: &mut usize) -> Result<()> {
    fs::create_dir_all(to).map_err(|source| write_error(to, source))?;
    let entries = fs::read_dir(from).map_err(|source| read_error(from, source))?;
    for entry in entries {
        let entry = entry.map_err(|source| read_error(from, source))?;
        let name = entry.file_name();
        let shown = name.to_string_lossy();
        if shown.starts_with('.') || shown.ends_with(TEST_SUFFIX) {
            continue;
        }
        let path = entry.path();
        let metadata = fs::metadata(&path).map_err(|source| read_error(&path, source))?;
        if metadata.is_dir() {
            if fs::canonicalize(&path).is_ok_and(|canonical| canonical == skip) {
                continue;
            }
            copy_game(&path, &to.join(&name), skip, files)?;
        } else if metadata.is_file() {
            let copy = to.join(&name);
            fs::copy(&path, &copy).map_err(|source| write_error(&copy, source))?;
            *files += 1;
        }
    }
    Ok(())
}

fn read_error(path: &Path, source: std::io::Error) -> Error {
    Error::ReadFile {
        path: path.to_owned(),
        source,
    }
}

fn write_error(path: &Path, source: std::io::Error) -> Error {
    Error::WriteFile {
        path: path.to_owned(),
        source,
    }
}

/// How far into a PE file's optional header its `Subsystem` is, in PE32 and PE32+ files alike.
const SUBSYSTEM: usize = 68;
/// The subsystems a Windows program runs in: with a window of its own, or in a console.
const GUI: u16 = 2;
const CONSOLE: u16 = 3;

/// Makes the Windows executable in `bytes` a program of the GUI subsystem, which Windows starts
/// without a console window, from one of the console's, which it opens one for: the field
/// `editbin /SUBSYSTEM:WINDOWS` changes. Its standard output and error then go nowhere, unless
/// whatever starts it hands it some. One that's already the GUI's stays as it is; anything else,
/// such as what isn't a PE file, is an error saying what it is.
pub fn gui_subsystem(bytes: &mut [u8]) -> std::result::Result<(), String> {
    let not_windows = || "it isn't a Windows executable".to_owned();
    if bytes.get(..2) != Some(b"MZ".as_slice()) {
        return Err(not_windows());
    }
    let header = bytes
        .get(0x3c..0x40)
        .and_then(|offset| offset.try_into().ok())
        .map(|offset| u32::from_le_bytes(offset) as usize)
        .ok_or_else(not_windows)?;
    if bytes.get(header..header + 4) != Some(b"PE\0\0".as_slice()) {
        return Err(not_windows());
    }
    // After the signature, the COFF file header, 20 bytes, then the optional header.
    let optional = header + 4 + 20;
    let magic = read_u16(bytes, optional).ok_or_else(not_windows)?;
    if magic != 0x10b && magic != 0x20b {
        return Err(not_windows());
    }
    match read_u16(bytes, optional + SUBSYSTEM).ok_or_else(not_windows)? {
        GUI => Ok(()),
        CONSOLE => {
            bytes[optional + SUBSYSTEM..optional + SUBSYSTEM + 2]
                .copy_from_slice(&GUI.to_le_bytes());
            Ok(())
        }
        other => Err(format!(
            "it's a program of Windows' subsystem {other}, not the console's"
        )),
    }
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let pair = bytes.get(at..at + 2)?;
    Some(u16::from_le_bytes([pair[0], pair[1]]))
}

/// The sizes, in pixels across, of the pictures of an exported executable's icon, among which
/// Windows picks as it shows it: those it shows at each scale of the screen, and the largest.
#[cfg(any(windows, test))]
const ICON_SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];

/// What gives a Windows executable `icon`: its pictures, each a PNG of one of [`ICON_SIZES`], for
/// resources of the kind `RT_ICON` numbered from 1, and the group of them, for an `RT_GROUP_ICON`,
/// which lists each one's size and number.
#[cfg(any(windows, test))]
fn icon_resources(icon: &WindowIcon) -> (Vec<u8>, Vec<Vec<u8>>) {
    let pictures: Vec<Vec<u8>> = ICON_SIZES
        .iter()
        .map(|&size| {
            let mut png = std::io::Cursor::new(Vec::new());
            icon.resized(size)
                .write_to(&mut png, image::ImageFormat::Png)
                .expect("a PNG encodes in memory");
            png.into_inner()
        })
        .collect();
    // A GRPICONDIR: reserved, the kind (1, icons rather than cursors) and how many there are.
    let mut group = [0u16, 1, pictures.len() as u16]
        .iter()
        .flat_map(|field| field.to_le_bytes())
        .collect::<Vec<u8>>();
    for (number, (&size, picture)) in (1u16..).zip(ICON_SIZES.iter().zip(&pictures)) {
        // A GRPICONDIRENTRY: its width and height, 0 meaning 256, which a byte can't hold; no
        // palette; reserved; one plane of 32 bits a pixel; its length; and its RT_ICON's number.
        let side = u8::try_from(size).unwrap_or(0);
        group.extend([side, side, 0, 0]);
        group.extend(1u16.to_le_bytes());
        group.extend(32u16.to_le_bytes());
        group.extend((picture.len() as u32).to_le_bytes());
        group.extend(number.to_le_bytes());
    }
    (group, pictures)
}

/// Gives the Windows executable at `path` `icon`, in place of any it had, through Windows'
/// `UpdateResourceW`, which lays out its resources again as it writes them. Explorer shows the
/// first icon group, which the executables Rust builds have none before.
#[cfg(windows)]
fn set_icon(path: &Path, icon: &WindowIcon) -> std::io::Result<()> {
    use std::{ffi::c_void, io, os::windows::ffi::OsStrExt, ptr};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn BeginUpdateResourceW(file_name: *const u16, delete_existing: i32) -> *mut c_void;
        fn UpdateResourceW(
            update: *mut c_void,
            kind: *const u16,
            name: *const u16,
            language: u16,
            data: *const c_void,
            length: u32,
        ) -> i32;
        fn EndUpdateResourceW(update: *mut c_void, discard: i32) -> i32;
    }
    /// The kinds of resource icons are: a picture, and the group listing them.
    const RT_ICON: usize = 3;
    const RT_GROUP_ICON: usize = 14;
    /// A resource's kind or name by number, as `MAKEINTRESOURCEW` passes it.
    fn by_number(number: usize) -> *const u16 {
        ptr::without_provenance(number)
    }

    let (group, pictures) = icon_resources(icon);
    let resources = (1..)
        .zip(&pictures)
        .map(|(number, picture)| (RT_ICON, number, picture))
        .chain([(RT_GROUP_ICON, 1, &group)]);
    let file_name: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: the name is a wide string ending in a null.
    let update = unsafe { BeginUpdateResourceW(file_name.as_ptr(), 0) };
    if update.is_null() {
        return Err(io::Error::last_os_error());
    }
    let mut failed = None;
    for (kind, number, data) in resources {
        // SAFETY: the handle is the open update's, and the data is `length` bytes, which last
        // until it ends. Language 0 is neutral.
        let written = unsafe {
            UpdateResourceW(
                update,
                by_number(kind),
                by_number(number),
                0,
                data.as_ptr().cast(),
                data.len() as u32,
            )
        };
        if written == 0 {
            failed = Some(io::Error::last_os_error());
            break;
        }
    }
    // SAFETY: the handle is the open update's, which this ends, discarding it if a write failed.
    let ended = unsafe { EndUpdateResourceW(update, i32::from(failed.is_some())) };
    match failed {
        Some(err) => Err(err),
        None if ended == 0 => Err(io::Error::last_os_error()),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::TemporaryFolder;

    /// The start of a PE file whose optional header begins with `magic`, of the subsystem
    /// `subsystem`.
    fn pe(magic: u16, subsystem: u16) -> Vec<u8> {
        let mut bytes = vec![0; 0x200];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        let optional = 0x80 + 24;
        bytes[optional..optional + 2].copy_from_slice(&magic.to_le_bytes());
        bytes[optional + SUBSYSTEM..optional + SUBSYSTEM + 2]
            .copy_from_slice(&subsystem.to_le_bytes());
        bytes
    }

    #[test]
    fn console_programs_become_the_guis() {
        for magic in [0x10b, 0x20b] {
            let mut bytes = pe(magic, CONSOLE);
            gui_subsystem(&mut bytes).unwrap();
            assert_eq!(bytes, pe(magic, GUI), "only the subsystem changes");
            gui_subsystem(&mut bytes).unwrap();
            assert_eq!(
                bytes,
                pe(magic, GUI),
                "one that's the GUI's already stays so"
            );
        }
        let mut native = pe(0x20b, 1);
        assert!(
            gui_subsystem(&mut native)
                .unwrap_err()
                .contains("subsystem 1")
        );
        for mut bytes in [b"\x7fELF and the rest".to_vec(), b"MZ".to_vec(), Vec::new()] {
            assert_eq!(
                gui_subsystem(&mut bytes).unwrap_err(),
                "it isn't a Windows executable"
            );
        }
        let mut no_signature = pe(0x20b, CONSOLE);
        no_signature[0x80] = b'X';
        assert!(gui_subsystem(&mut no_signature).is_err());
    }

    /// A temporary folder holding a game of `files`, by their paths in it.
    fn game(files: &[(&str, &str)]) -> TemporaryFolder {
        let folder = TemporaryFolder::new();
        for (path, contents) in files {
            let path = folder.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        folder
    }

    /// The files in `folder`, by their paths in it with `/` between names, in order.
    fn files_in(folder: &Path) -> Vec<String> {
        fn visit(folder: &Path, path: &str, found: &mut Vec<String>) {
            for entry in fs::read_dir(folder.join(path)).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name().into_string().unwrap();
                let inside = if path.is_empty() {
                    name
                } else {
                    format!("{path}/{name}")
                };
                if entry.file_type().unwrap().is_dir() {
                    visit(folder, &inside, found);
                } else {
                    found.push(inside);
                }
            }
        }
        let mut found = Vec::new();
        visit(folder, "", &mut found);
        found.sort();
        found
    }

    #[test]
    fn exports_hold_the_game_the_executable_and_the_licenses() {
        let game = game(&[
            ("main.luau", "print('hi')"),
            (
                "conf.luau",
                "return { identity = 'tiny', libraries = { 'missing', { name = 'extra', optional = true } } }",
            ),
            ("levels/one.scene.luau", "return { format = 1 }"),
            ("tests/main.test.luau", "test('x', function() end)"),
            ("game.test.luau", "test('y', function() end)"),
            (".luaurc", "{}"),
            (".test-output/shot.actual.png", ""),
        ]);
        // Exported inside the game's folder, which it doesn't copy into itself.
        let out = game.path().join("dist/tiny");
        let executable = std::env::current_exe().unwrap();
        let exported = export(game.path(), &out, &executable).unwrap();

        let name = format!("tiny{}", std::env::consts::EXE_SUFFIX);
        assert_eq!(exported.executable, out.join(&name));
        assert_eq!(exported.files, 3);
        let mut expected = vec![
            "game/conf.luau".to_owned(),
            "game/levels/one.scene.luau".into(),
            "game/main.luau".into(),
            "licenses/wingine-APACHE.txt".into(),
            "licenses/wingine-MIT.txt".into(),
            name.clone(),
        ];
        expected.sort();
        assert_eq!(files_in(&out), expected);
        assert_eq!(
            fs::read_to_string(out.join("licenses/wingine-MIT.txt")).unwrap(),
            LICENSE_MIT
        );

        let copied = fs::read(&exported.executable).unwrap();
        let original = fs::read(&executable).unwrap();
        assert_eq!(copied.len(), original.len());
        if cfg!(windows) {
            let mut expected = original;
            gui_subsystem(&mut expected).unwrap();
            assert_eq!(
                copied, expected,
                "the copy is the GUI's, and otherwise the same"
            );
        } else {
            assert_eq!(copied, original);
        }

        let warnings = exported.warnings.join("\n");
        assert!(
            warnings.contains("there's no THIRD-PARTY-NOTICES.txt"),
            "{warnings}"
        );
        assert!(warnings.contains("debug build"), "{warnings}");
        assert!(warnings.contains("native library `missing`"), "{warnings}");
        assert!(
            !warnings.contains("`extra`"),
            "optional libraries may be missing"
        );
    }

    /// A 2x2 icon of checks.
    fn checks() -> WindowIcon {
        let rgba = [
            [255, 0, 0, 255],
            [0, 0, 255, 255],
            [0, 0, 255, 255],
            [255, 0, 0, 255],
        ];
        WindowIcon::from_rgba(rgba.concat(), 2, 2).unwrap()
    }

    #[test]
    fn icon_groups_list_a_png_for_each_size() {
        let (group, pictures) = icon_resources(&checks());
        let u16_at = |at: usize| u16::from_le_bytes([group[at], group[at + 1]]);
        let u32_at = |at: usize| u32::from_le_bytes(group[at..at + 4].try_into().unwrap());
        assert_eq!([u16_at(0), u16_at(2), u16_at(4)], [0, 1, 8]);
        assert_eq!(group.len(), 6 + 14 * ICON_SIZES.len());
        assert_eq!(pictures.len(), ICON_SIZES.len());
        for (i, (&size, picture)) in ICON_SIZES.iter().zip(&pictures).enumerate() {
            let entry = 6 + 14 * i;
            let side = if size == 256 { 0 } else { size as u8 };
            assert_eq!(group[entry..entry + 4], [side, side, 0, 0], "{size}");
            assert_eq!([u16_at(entry + 4), u16_at(entry + 6)], [1, 32]);
            assert_eq!(u32_at(entry + 8) as usize, picture.len());
            assert_eq!(u16_at(entry + 12) as usize, i + 1, "numbered from 1");
            let decoded = image::load_from_memory(picture).unwrap().into_rgba8();
            assert_eq!(decoded.dimensions(), (size, size));
            assert_eq!(decoded.get_pixel(0, 0).0, [255, 0, 0, 255]);
        }
    }

    #[test]
    fn exported_executables_show_the_games_icon_on_windows() {
        let game = game(&[
            ("main.luau", ""),
            (
                "conf.luau",
                "return { identity = 'tiny', icon = 'icon.png' }",
            ),
        ]);
        checks()
            .resized(2)
            .save(game.path().join("icon.png"))
            .unwrap();
        let out = game.path().join("out");
        let executable = std::env::current_exe().unwrap();
        let exported = export(game.path(), &out, &executable).unwrap();
        assert!(out.join("game/icon.png").is_file());

        let copied = fs::read(&exported.executable).unwrap();
        let (group, pictures) = icon_resources(&checks());
        // From the end, where the resources Windows writes usually are, since the copy is large.
        let holds = |bytes: &[u8]| {
            copied
                .windows(bytes.len())
                .rev()
                .any(|window| window == bytes)
        };
        if cfg!(windows) {
            // Windows keeps resources as they're written, so the group and the largest picture
            // are in it as they are.
            assert!(holds(&group), "the icon's group");
            assert!(holds(&pictures[pictures.len() - 1]), "its largest picture");
        } else {
            assert_eq!(
                copied,
                fs::read(&executable).unwrap(),
                "only Windows' have icons"
            );
        }
    }

    #[test]
    fn exports_refuse_what_isnt_a_game_or_isnt_empty() {
        let executable = std::env::current_exe().unwrap();
        let no_main = game(&[("conf.luau", "return {}")]);
        let out = no_main.path().join("out");
        let err = export(no_main.path(), &out, &executable).unwrap_err();
        assert!(err.to_string().contains("it has no main.luau"), "{err}");
        assert!(!out.exists(), "nothing is written");

        let game = game(&[("main.luau", ""), ("out/old.txt", "keep me")]);
        let out = game.path().join("out");
        let err = export(game.path(), &out, &executable).unwrap_err();
        assert!(err.to_string().contains("isn't an empty folder"), "{err}");
        assert_eq!(files_in(&out), ["old.txt"]);
    }

    #[test]
    fn the_game_beside_an_executable_is_its_game_folder() {
        let folder = game(&[("game/main.luau", "")]);
        let executable = folder.path().join("pong.exe");
        assert_eq!(
            super::super::game_beside(&executable),
            Some(folder.path().join("game"))
        );
        let elsewhere = folder.path().join("game/pong.exe");
        assert_eq!(super::super::game_beside(&elsewhere), None);
    }
}
