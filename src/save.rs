//! Saves: data a game keeps between runs, such as the player's progress or settings.
//!
//! Each save is a file named after it, `<name>.luau`, in a folder of the game's own, holding
//! plain data as Luau that returns it: a boolean, number, string, or table of them, where a table
//! is a list or has strings for keys, as a scene's props hold. Saves are written in the canonical
//! layout scenes are, so they stay easy to read.
//!
//! In a window, the folder is `Wingine/<identity>` in the folder the system keeps applications'
//! data in: `%APPDATA%` on Windows, `~/Library/Application Support` on macOS, and
//! `$XDG_DATA_HOME` or `~/.local/share` elsewhere, where the identity is
//! [`WindowConfig::identity`]. [`WindowConfig::save_folder`] puts it anywhere else. A
//! [`Simulation`](crate::headless::Simulation) without one keeps its saves in a temporary folder
//! of its own, which goes when it does, so tests and scripted runs start with none and never
//! touch the player's.
//!
//! On the web, there are no files: a save is kept in the browser's local storage for the page,
//! under the key its file would have, `Wingine/<identity>/<name>.luau`.

use std::{
    env,
    ffi::OsString,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    Error, Result, WindowConfig,
    scene::{Value, evaluate_any, write_value},
};

/// The extension of save files.
const EXTENSION: &str = "luau";

/// Whether `name` can name a save, a game's save folder or a native library: it's part of a file
/// name, so it may hold only letters, digits, `_` and `-`.
pub(crate) fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A game's saves, in a folder of its own. See the [module docs](self).
///
/// Reading, listing and removing saves work even when there's nowhere to keep them, finding none;
/// only writing one fails.
#[derive(Clone, Debug)]
pub struct Saves {
    /// The folder, or why there's none.
    folder: Result<PathBuf, String>,
}

impl Default for Saves {
    /// Saves with nowhere to go.
    fn default() -> Self {
        Self {
            folder: Err("the game has no folder for them".to_owned()),
        }
    }
}

impl Saves {
    /// Saves kept in `folder`, which is made when the first is written.
    pub fn in_folder(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: Ok(folder.into()),
        }
    }

    /// The saves of a game in a window with `config`: in its `save_folder`, or one named by its
    /// identity among applications' data.
    ///
    /// # Panics
    ///
    /// If the identity isn't a name of letters, digits, `_` and `-`.
    pub(crate) fn for_window(config: &WindowConfig) -> Self {
        if let Some(folder) = &config.save_folder {
            return Self::in_folder(folder);
        }
        let identity = &config.identity;
        assert!(
            is_name(identity),
            "`{identity}` can't name a game's saves: use only letters, digits, `_` and `-`"
        );
        if cfg!(target_arch = "wasm32") {
            return Self::in_folder(Path::new("Wingine").join(identity));
        }
        match data_folder(|name| env::var_os(name)) {
            Some(data) => Self::in_folder(data.join("Wingine").join(identity)),
            None => Self {
                folder: Err("the system names no folder for applications' data".to_owned()),
            },
        }
    }

    /// The folder saves are kept in, if there's one. It's made when the first save is written.
    pub fn folder(&self) -> Option<&Path> {
        self.folder.as_deref().ok()
    }

    /// Writes `data` to the save `name`, replacing any. The save is written in full to a file of
    /// its own first, then takes the old one's place, so a game that stops partway through
    /// writing it leaves the old one as it was.
    ///
    /// Fails if `name` isn't a save's name, if a number isn't finite, or if there's nowhere to
    /// keep saves.
    pub fn write(&self, name: &str, data: &Value) -> Result<()> {
        let error = |message: String| Error::Save {
            name: name.to_owned(),
            message,
        };
        let Some(path) = self.path(name)? else {
            let why = self.folder.as_ref().unwrap_err();
            return Err(error(format!("there's nowhere to keep saves: {why}")));
        };
        let text = write_value(data).map_err(error)?;
        if cfg!(target_arch = "wasm32") {
            return web_storage::write(&path, &text).map_err(|source| Error::WriteFile {
                path: path.clone(),
                source,
            });
        }
        let folder = path.parent().expect("saves are in a folder");
        fs::create_dir_all(folder).map_err(|source| Error::WriteFile {
            path: folder.to_owned(),
            source,
        })?;
        write_replacing(&path, &text)
    }

    /// The data in the save `name`, or `None` if there's no such save.
    ///
    /// Fails if `name` isn't a save's name, or if the file can't be read or doesn't hold plain
    /// data.
    pub fn read(&self, name: &str) -> Result<Option<Value>> {
        let Some(path) = self.path(name)? else {
            return Ok(None);
        };
        let read = if cfg!(target_arch = "wasm32") {
            web_storage::read(&path)
        } else {
            fs::read_to_string(&path)
        };
        let source = match read {
            Ok(source) => source,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(Error::ReadFile { path, source }),
        };
        let data = evaluate_any(&source, &path).map_err(|message| Error::Save {
            name: name.to_owned(),
            message,
        })?;
        Ok(Some(data))
    }

    /// Removes the save `name`, if there is one.
    ///
    /// Fails if `name` isn't a save's name, or if the file can't be removed.
    pub fn remove(&self, name: &str) -> Result<()> {
        let Some(path) = self.path(name)? else {
            return Ok(());
        };
        let removed = if cfg!(target_arch = "wasm32") {
            web_storage::remove(&path)
        } else {
            fs::remove_file(&path)
        };
        match removed {
            Err(err) if err.kind() != io::ErrorKind::NotFound => {
                Err(Error::WriteFile { path, source: err })
            }
            _ => Ok(()),
        }
    }

    /// The names of the saves there are, in order.
    pub fn list(&self) -> Result<Vec<String>> {
        let Ok(folder) = &self.folder else {
            return Ok(Vec::new());
        };
        if cfg!(target_arch = "wasm32") {
            return web_storage::list(folder).map_err(|source| Error::ReadFile {
                path: folder.clone(),
                source,
            });
        }
        let entries = match fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(Error::ReadFile {
                    path: folder.clone(),
                    source,
                });
            }
        };
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| Error::ReadFile {
                path: folder.clone(),
                source,
            })?;
            let path = entry.path();
            let name = path.file_stem().and_then(|stem| stem.to_str());
            let is_save = path
                .extension()
                .is_some_and(|extension| extension == EXTENSION);
            if let Some(name) = name
                && is_save
                && is_name(name)
                && entry.file_type().is_ok_and(|kind| kind.is_file())
            {
                names.push(name.to_owned());
            }
        }
        names.sort();
        Ok(names)
    }

    /// The path of the save `name`'s file, or `None` for saves with nowhere to go. Fails if
    /// `name` isn't a save's name, whether or not there's a folder.
    fn path(&self, name: &str) -> Result<Option<PathBuf>> {
        if !is_name(name) {
            return Err(Error::Save {
                name: name.to_owned(),
                message: "a save's name may hold only letters, digits, `_` and `-`".to_owned(),
            });
        }
        let folder = self.folder();
        Ok(folder.map(|folder| folder.join(format!("{name}.{EXTENSION}"))))
    }
}

/// Writes `text` to the file at `path`, replacing any: in full to `<path>.partial` first, which
/// then takes the old file's place, so stopping partway through leaves the old one as it was.
pub(crate) fn write_replacing(path: &Path, text: &str) -> Result<()> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    let partial = PathBuf::from(partial);
    write_synced(&partial, text)
        .and_then(|()| fs::rename(&partial, path))
        .map_err(|source| {
            let _ = fs::remove_file(&partial);
            Error::WriteFile {
                path: path.to_owned(),
                source,
            }
        })
}

/// Writes `text` to a new file at `path`, and waits until it's on the disk.
fn write_synced(path: &Path, text: &str) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

/// Off the web, there's no local storage, and saves are files.
#[cfg(not(target_arch = "wasm32"))]
mod web_storage {
    use std::{io, path::Path};

    fn unsupported() -> io::Error {
        io::Error::new(io::ErrorKind::Unsupported, "only the web has local storage")
    }

    pub(super) fn write(_: &Path, _: &str) -> io::Result<()> {
        Err(unsupported())
    }

    pub(super) fn read(_: &Path) -> io::Result<String> {
        Err(unsupported())
    }

    pub(super) fn remove(_: &Path) -> io::Result<()> {
        Err(unsupported())
    }

    pub(super) fn list(_: &Path) -> io::Result<Vec<String>> {
        Err(unsupported())
    }
}

/// Saves on the web, in the browser's local storage for the page, under the paths their files
/// would have.
#[cfg(target_arch = "wasm32")]
mod web_storage {
    use std::{io, path::Path};

    /// The key the file at `path` is kept under.
    fn key(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    fn storage() -> io::Result<web_sys::Storage> {
        let missing = || io::Error::other("the browser keeps no local storage for the page");
        web_sys::window()
            .ok_or_else(missing)?
            .local_storage()
            .map_err(browser_error)?
            .ok_or_else(missing)
    }

    fn browser_error(err: impl std::fmt::Debug) -> io::Error {
        io::Error::other(format!("local storage failed: {err:?}"))
    }

    pub(super) fn write(path: &Path, text: &str) -> io::Result<()> {
        storage()?.set_item(&key(path), text).map_err(browser_error)
    }

    pub(super) fn read(path: &Path) -> io::Result<String> {
        storage()?
            .get_item(&key(path))
            .map_err(browser_error)?
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }

    pub(super) fn remove(path: &Path) -> io::Result<()> {
        storage()?.remove_item(&key(path)).map_err(browser_error)
    }

    /// The names of the saves in `folder`, in order.
    pub(super) fn list(folder: &Path) -> io::Result<Vec<String>> {
        let storage = storage()?;
        let prefix = format!("{}/", key(folder));
        let mut names = Vec::new();
        for index in 0..storage.length().map_err(browser_error)? {
            let Some(key) = storage.key(index).map_err(browser_error)? else {
                continue;
            };
            let name = key
                .strip_prefix(&prefix)
                .and_then(|file| file.strip_suffix(&format!(".{}", super::EXTENSION)));
            if let Some(name) = name
                && super::is_name(name)
            {
                names.push(name.to_owned());
            }
        }
        names.sort();
        Ok(names)
    }
}

/// The folder the system keeps applications' data in, given `var`, which looks up environment
/// variables: `%APPDATA%` on Windows, `~/Library/Application Support` on macOS, and
/// `$XDG_DATA_HOME` or `~/.local/share` elsewhere.
fn data_folder(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    // A relative path would depend on where the game was started from.
    let absolute = |name: &str| {
        var(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    if cfg!(windows) {
        absolute("APPDATA")
    } else if cfg!(target_os = "macos") {
        Some(absolute("HOME")?.join("Library/Application Support"))
    } else {
        absolute("XDG_DATA_HOME").or_else(|| Some(absolute("HOME")?.join(".local/share")))
    }
}

/// A folder for saves of a [`Simulation`](crate::headless::Simulation)'s own, removed with
/// whatever is in it when this is dropped.
pub(crate) struct TemporaryFolder(PathBuf);

impl TemporaryFolder {
    /// A new folder's path, in the system's folder for temporary files, which isn't made until a
    /// save is written.
    pub fn new() -> Self {
        static COUNT: AtomicU64 = AtomicU64::new(0);
        let count = COUNT.fetch_add(1, Ordering::Relaxed);
        let name = format!("wingine-saves-{}-{count}", process::id());
        let folder = env::temp_dir().join(name);
        // Left behind by a process that had the same ID and didn't finish.
        let _ = fs::remove_dir_all(&folder);
        Self(folder)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryFolder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn saves() -> (Saves, TemporaryFolder) {
        let folder = TemporaryFolder::new();
        (Saves::in_folder(folder.path()), folder)
    }

    fn map(entries: &[(&str, Value)]) -> Value {
        Value::Map(
            entries
                .iter()
                .map(|(key, value)| (key.to_string(), value.clone()))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    #[test]
    fn data_reads_back_as_it_was_written() {
        let (saves, _folder) = saves();
        let data = map(&[
            ("quest", Value::String("hunt".into())),
            ("said", Value::String("\"Hi,\"\n\tshe said. \\o/".into())),
            ("tenth", Value::Number(0.1)),
            ("negative_zero", Value::Number(-0.0)),
            ("huge", Value::Number(1e300)),
            ("tiny", Value::Number(-5e-324)),
            ("done", Value::Bool(false)),
            ("not a name", Value::List(vec![])),
            (
                "levels",
                map(&[(
                    "village",
                    map(&[
                        ("found", Value::List(vec![Value::String("3,4".into())])),
                        (
                            "foes",
                            Value::List(vec![map(&[("at", Value::Number(2.0))])]),
                        ),
                    ]),
                )]),
            ),
        ]);
        saves.write("progress", &data).unwrap();
        let read = saves.read("progress").unwrap();
        assert_eq!(read.as_ref(), Some(&data));
        // -0 equals 0, so only its sign tells them apart.
        let Some(Value::Map(fields)) = read else {
            unreachable!()
        };
        assert!(matches!(fields["negative_zero"], Value::Number(zero) if zero.is_sign_negative()));

        saves.write("best", &Value::Number(1234.0)).unwrap();
        assert_eq!(saves.read("best").unwrap(), Some(Value::Number(1234.0)));
        let text = fs::read_to_string(saves.folder().unwrap().join("best.luau")).unwrap();
        assert_eq!(text, "return 1234\n");
    }

    #[test]
    fn writing_replaces_a_save() {
        let (saves, _folder) = saves();
        saves.write("slot", &Value::Bool(true)).unwrap();
        saves.write("slot", &Value::Bool(false)).unwrap();
        assert_eq!(saves.read("slot").unwrap(), Some(Value::Bool(false)));
        assert_eq!(saves.list().unwrap(), ["slot"]);
    }

    #[test]
    fn a_file_that_fails_to_be_replaced_stays_as_it_was() {
        let folder = TemporaryFolder::new();
        fs::create_dir(folder.path()).unwrap();
        let path = folder.path().join("level.scene.luau");
        write_replacing(&path, "return 1\n").unwrap();
        write_replacing(&path, "return 2\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "return 2\n");
        assert!(!folder.path().join("level.scene.luau.partial").exists());

        // Something in the way of the new file.
        fs::create_dir(folder.path().join("level.scene.luau.partial")).unwrap();
        let err = write_replacing(&path, "return 3\n").unwrap_err();
        assert!(err.to_string().contains("level.scene.luau"), "{err}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "return 2\n");
    }

    #[test]
    fn missing_saves_are_none() {
        let (saves, _folder) = saves();
        assert_eq!(saves.read("nothing").unwrap(), None);
        assert_eq!(saves.list().unwrap(), Vec::<String>::new());
        saves.remove("nothing").unwrap();
    }

    #[test]
    fn saves_are_listed_in_order_and_removed() {
        let (saves, _folder) = saves();
        for name in ["b", "a", "c-2"] {
            saves.write(name, &Value::Number(1.0)).unwrap();
        }
        let folder = saves.folder().unwrap();
        fs::write(folder.join("notes.txt"), "").unwrap();
        fs::write(folder.join("a.luau.partial"), "").unwrap();
        fs::write(folder.join("not a save.luau"), "").unwrap();
        fs::create_dir(folder.join("d.luau")).unwrap();
        assert_eq!(saves.list().unwrap(), ["a", "b", "c-2"]);
        saves.remove("b").unwrap();
        assert_eq!(saves.list().unwrap(), ["a", "c-2"]);
        assert_eq!(saves.read("b").unwrap(), None);
    }

    #[test]
    fn names_are_letters_digits_underscores_and_dashes() {
        let (saves, _folder) = saves();
        for name in ["", "../escape", "a/b", "a.b", "slot 1", "é"] {
            let err = saves.write(name, &Value::Bool(true)).unwrap_err();
            assert!(err.to_string().contains("only letters, digits"), "{err}");
            assert!(saves.read(name).is_err(), "{name}");
            assert!(saves.remove(name).is_err(), "{name}");
        }
        assert!(saves.folder().is_some_and(|folder| !folder.exists()));
    }

    #[test]
    fn numbers_must_be_finite() {
        let (saves, _folder) = saves();
        let data = Value::List(vec![Value::Number(f64::NAN)]);
        let err = saves.write("slot", &data).unwrap_err().to_string();
        assert!(
            err.contains("save `slot`") && err.contains("finite"),
            "{err}"
        );
        assert_eq!(saves.read("slot").unwrap(), None);
    }

    #[test]
    fn files_that_arent_plain_data_fail_to_read() {
        let (saves, _folder) = saves();
        saves.write("slot", &Value::Bool(true)).unwrap();
        let path = saves.folder().unwrap().join("slot.luau");
        fs::write(&path, "return { f = function() end }").unwrap();
        let err = saves.read("slot").unwrap_err().to_string();
        assert!(err.contains("`f` is a function"), "{err}");
        fs::write(&path, "return {").unwrap();
        let err = saves.read("slot").unwrap_err().to_string();
        assert!(
            err.contains("save `slot`") && err.contains("slot.luau"),
            "{err}"
        );
        fs::write(&path, "return os.time()").unwrap();
        assert!(saves.read("slot").is_err());
    }

    #[test]
    fn saves_with_nowhere_to_go_are_never_found() {
        let saves = Saves::default();
        assert_eq!(saves.folder(), None);
        assert_eq!(saves.read("slot").unwrap(), None);
        assert_eq!(saves.list().unwrap(), Vec::<String>::new());
        saves.remove("slot").unwrap();
        let err = saves.write("slot", &Value::Bool(true)).unwrap_err();
        assert!(err.to_string().contains("nowhere to keep saves"), "{err}");
    }

    #[test]
    fn a_temporary_folder_goes_with_what_it_holds() {
        let folder = TemporaryFolder::new();
        let path = folder.path().to_owned();
        Saves::in_folder(&path)
            .write("slot", &Value::Bool(true))
            .unwrap();
        assert!(path.join("slot.luau").is_file());
        drop(folder);
        assert!(!path.exists());
        assert_ne!(TemporaryFolder::new().path(), path);
    }

    #[test]
    fn the_data_folder_comes_from_the_environment() {
        /// Looks environment variables up in `vars`, names and values.
        fn vars<'a>(vars: &'a [(&str, &str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
            move |name| {
                vars.iter()
                    .find(|(var, _)| *var == name)
                    .map(|(_, value)| OsString::from(value))
            }
        }
        if cfg!(windows) {
            let roaming = r"C:\Users\pat\AppData\Roaming";
            let folder = data_folder(vars(&[("APPDATA", roaming)]));
            assert_eq!(folder, Some(PathBuf::from(roaming)));
            assert_eq!(data_folder(vars(&[("APPDATA", "relative")])), None);
        } else if cfg!(target_os = "macos") {
            let folder = data_folder(vars(&[("HOME", "/Users/pat")]));
            let expected = PathBuf::from("/Users/pat/Library/Application Support");
            assert_eq!(folder, Some(expected));
        } else {
            let folder = data_folder(vars(&[("HOME", "/home/pat")]));
            assert_eq!(folder, Some(PathBuf::from("/home/pat/.local/share")));
            let both = [("HOME", "/home/pat"), ("XDG_DATA_HOME", "/data")];
            let folder = data_folder(vars(&both));
            assert_eq!(folder, Some(PathBuf::from("/data")));
            let relative = [("HOME", "/home/pat"), ("XDG_DATA_HOME", "data")];
            let folder = data_folder(vars(&relative));
            assert_eq!(folder, Some(PathBuf::from("/home/pat/.local/share")));
        }
        assert_eq!(data_folder(vars(&[])), None);
    }
}
