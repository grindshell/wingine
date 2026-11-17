//! Folders the player picks, whose files a game reads and writes, as an application's documents.
//!
//! Scripts are sandboxed: they read the game's own files and keep [saves](crate::save), and reach
//! nothing else on the player's computer. A game that works on the player's files, as a text editor
//! does, asks the player to pick a folder in the system's dialog ([`Folders::pick`]), and gets a
//! [`Folder`], which reaches the files in it, and in the folders inside it, and nothing outside it.
//! The engine remembers the folders picked ([`Folders::picked`]), most recently first, so a game
//! opens the one it had open last without asking again, until it forgets it
//! ([`Folders::forget`]).
//!
//! Paths in a folder are relative to it, names joined by `/` on every system, such as
//! `dialogs/wanderer.md`. Nothing in a path may leave the folder: no `..` or `.`, no absolute
//! path, no `\` or `:`, and no link, such as a symbolic link or a Windows junction, that leads out
//! of it. A name may not end with `.` or a space, which Windows drops, nor be a name Windows keeps
//! for a device, such as `con` or `con.txt`, on any system, so a game's paths work on every one.
//! [`Folder::list`] leaves out what no path could name.
//!
//! Writing a file writes it in full to a file of its own first, which then takes the old one's
//! place, as saves are written, so stopping partway through leaves the old one as it was.
//!
//! A folder can go while a game has it: moved, deleted, or on a drive taken out. Then reading in it
//! finds nothing, and writing and renaming in it fail, rather than make the folders a path is in
//! from the folder itself down, which would make it again where it was, or on the disk the drive
//! was mounted on.
//!
//! In a window, the dialog is the system's, opened over the window once the frame that asked for
//! it is done, and the folders picked are remembered in a file beside the game's saves,
//! `.folders.luau`, which isn't a save. A [`Simulation`](crate::headless::Simulation) opens no
//! dialog and remembers nothing between runs: a test answers the dialog with
//! [`Simulation::pick`](crate::headless::Simulation::pick), which hands the game a copy of a folder,
//! so tests never touch the player's files. There are no folders to pick on the web yet.

use std::{
    fs, io, mem,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    Error, Result,
    save::{self, Saves},
    scene::{Value, evaluate_any, write_value},
};

/// The file beside a game's saves that the folders picked are remembered in. Its name isn't a
/// save's, so scripts can't reach it through `save`.
const PICKED_FILE: &str = ".folders.luau";
/// How many folders picked are remembered.
const REMEMBERED: usize = 20;

/// A folder the player picked, whose files a game reads and writes. See the
/// [module docs](self).
///
/// Two are equal when they're the same folder, however it was reached.
#[derive(Clone, Debug)]
pub struct Folder {
    /// Where it is, every link followed, as what paths in it lead to is checked against.
    root: PathBuf,
    /// Where it is, as the system shows it.
    shown: PathBuf,
}

impl PartialEq for Folder {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
    }
}

impl Eq for Folder {}

/// A file or folder in a [`Folder`], as [`Folder::list`] finds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Its path in the folder, which the folder's other functions take.
    pub path: String,
    pub kind: EntryKind,
    /// Its size in bytes, or 0 for a folder.
    pub size: u64,
    /// When it last changed, in seconds since 1970 began, UTC, or 0 where the system can't tell.
    pub modified: f64,
}

/// What an [`Entry`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Folder,
}

impl Folder {
    /// The folder at `path`, which must be one. Games written in Rust reach any folder; scripts
    /// only those the player picks.
    pub fn at(path: impl AsRef<Path>) -> io::Result<Self> {
        let root = fs::canonicalize(path)?;
        if !root.is_dir() {
            return Err(io::Error::new(io::ErrorKind::NotADirectory, "not a folder"));
        }
        let shown = shown(&root);
        Ok(Self { root, shown })
    }

    /// The folder's name, the last of its path's.
    pub fn name(&self) -> String {
        match self.shown.file_name() {
            Some(name) => name.to_string_lossy().into_owned(),
            // The root of a drive.
            None => self.shown.display().to_string(),
        }
    }

    /// Where the folder is.
    pub fn path(&self) -> &Path {
        &self.shown
    }

    /// The files and folders in the folder at `path`, or in this one for `""`, in order of their
    /// paths, or `None` if there's no such folder. Links are listed as what they lead to, if
    /// that's in the folder. Fails if `path` isn't one, or names a file.
    pub fn list(&self, path: &str) -> Result<Option<Vec<Entry>>> {
        let parts = self.parts(path)?;
        let folder = self.resolve(path, &parts, true)?;
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(_) if folder.is_file() => {
                return Err(self.problem(path, "is a file, not a folder"));
            }
            Err(source) => return Err(read_error(&folder, source)),
        };
        let prefix = parts.join("/");
        let mut list = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| read_error(&folder, source))?;
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if part_problem(&name).is_some() {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let metadata = if kind.is_symlink() {
                match fs::canonicalize(entry.path()) {
                    Ok(target) if target.starts_with(&self.root) => fs::metadata(target),
                    _ => continue,
                }
            } else {
                entry.metadata()
            };
            let Ok(metadata) = metadata else {
                continue;
            };
            let kind = if metadata.is_dir() {
                EntryKind::Folder
            } else if metadata.is_file() {
                EntryKind::File
            } else {
                continue;
            };
            list.push(Entry {
                path: if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}/{name}")
                },
                kind,
                size: if kind == EntryKind::File {
                    metadata.len()
                } else {
                    0
                },
                modified: seconds(metadata.modified()),
            });
        }
        list.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Some(list))
    }

    /// What the file at `path` holds, or `None` if there's no such file. Fails if `path` isn't
    /// one, names a folder, or the file can't be read.
    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let parts = self.file_parts(path)?;
        let file = self.resolve(path, &parts, true)?;
        if file.is_dir() {
            return Err(self.problem(path, "is a folder, not a file"));
        }
        match fs::read(&file) {
            Ok(contents) => Ok(Some(contents)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(read_error(&file, source)),
        }
    }

    /// Writes `contents` to the file at `path`, replacing any, and making the folders it's in if
    /// they aren't there. It's written in full to a file of its own first, which then takes the old
    /// one's place, so stopping partway through leaves the old one as it was. Fails if `path`
    /// isn't one, names a folder, the folder itself isn't there any more, or the file can't be
    /// written.
    pub fn write(&self, path: &str, contents: &[u8]) -> Result<()> {
        let parts = self.file_parts(path)?;
        self.still_there(path)?;
        let file = self.resolve(path, &parts, true)?;
        if file.is_dir() {
            return Err(self.problem(path, "is a folder, not a file"));
        }
        make_parent(&file)?;
        save::write_replacing(&file, contents).map_err(|err| match err {
            Error::WriteFile { path, source } => write_error(&path, source),
            err => err,
        })
    }

    /// Moves the file or folder at `from` to `to`, making the folders `to` is in if they aren't
    /// there. Fails if either isn't a path, if the folder itself isn't there any more, if there's
    /// nothing at `from`, or if there's something at `to` already, which renaming never replaces,
    /// unless it's what's at `from`, as when a name changes only its case on a system that
    /// doesn't tell cases apart.
    pub fn rename(&self, from: &str, to: &str) -> Result<()> {
        let (from_parts, to_parts) = (self.file_parts(from)?, self.file_parts(to)?);
        self.still_there(to)?;
        let source = self.resolve(from, &from_parts, false)?;
        let target = self.resolve(to, &to_parts, false)?;
        if fs::symlink_metadata(&source).is_err() {
            return Err(self.problem(from, "there's nothing there to rename"));
        }
        if fs::symlink_metadata(&target).is_ok() && !same_entry(&source, &target) {
            let message = "something is there already, which renaming never replaces";
            return Err(self.problem(to, message));
        }
        make_parent(&target)?;
        fs::rename(&source, &target).map_err(|err| write_error(&target, err))
    }

    /// Removes the file at `path`, if there is one: a link, not what it leads to. Fails if `path`
    /// isn't one, names a folder, which this never removes, or the file can't be removed.
    pub fn remove(&self, path: &str) -> Result<()> {
        let parts = self.file_parts(path)?;
        let file = self.resolve(path, &parts, false)?;
        match fs::symlink_metadata(&file) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Ok(metadata) if metadata.is_dir() => {
                Err(self.problem(path, "is a folder, which `remove` leaves"))
            }
            _ => fs::remove_file(&file).map_err(|err| write_error(&file, err)),
        }
    }

    /// Fails, about `path`, if the folder itself isn't there any more: moved, deleted, or on a
    /// drive taken out. Writing makes the folders a file is in, which would make it again where it
    /// was, or on the disk the drive was mounted on.
    fn still_there(&self, path: &str) -> Result<()> {
        if self.root.is_dir() {
            Ok(())
        } else {
            Err(self.problem(path, "the folder isn't there any more"))
        }
    }

    /// An error about `path` in the folder.
    fn problem(&self, path: &str, message: impl Into<String>) -> Error {
        Error::Folder {
            folder: self.name(),
            path: path.to_owned(),
            message: message.into(),
        }
    }

    /// The names `path` is made of, none for the folder itself, or an error saying why it isn't
    /// a path in a folder.
    fn parts<'a>(&self, path: &'a str) -> Result<Vec<&'a str>> {
        if path.is_empty() {
            return Ok(Vec::new());
        }
        path.split('/')
            .map(|part| match part_problem(part) {
                Some(problem) => Err(self.problem(path, problem)),
                None => Ok(part),
            })
            .collect()
    }

    /// The names `path` is made of, which must name something in the folder, not the folder.
    fn file_parts<'a>(&self, path: &'a str) -> Result<Vec<&'a str>> {
        let parts = self.parts(path)?;
        if parts.is_empty() {
            return Err(self.problem(path, "names the folder itself, not what's in it"));
        }
        Ok(parts)
    }

    /// Where `parts`, the names of `path`, lead, following every link on the way, and the last
    /// name's too if `follow_last`. Fails if a link leads out of the folder, or nowhere.
    fn resolve(&self, path: &str, parts: &[&str], follow_last: bool) -> Result<PathBuf> {
        // Only what's there can be a link; what follows the first name that isn't, can't.
        let mut there = 0;
        let mut at = self.root.clone();
        for part in parts {
            at.push(part);
            if fs::symlink_metadata(&at).is_err() {
                break;
            }
            there += 1;
        }
        let followed = if there == parts.len() && !follow_last {
            there.saturating_sub(1)
        } else {
            there
        };
        let mut resolved = self.root.clone();
        if followed > 0 {
            let mut prefix = self.root.clone();
            prefix.extend(&parts[..followed]);
            resolved = fs::canonicalize(&prefix).map_err(|err| {
                self.problem(path, format!("a link on the way leads nowhere: {err}"))
            })?;
            if !resolved.starts_with(&self.root) {
                return Err(self.problem(path, "a link on the way leads out of the folder"));
            }
        }
        resolved.extend(&parts[followed..]);
        Ok(resolved)
    }
}

/// Why `part` can't be a name in a path, if it can't, said as what a name is.
fn part_problem(part: &str) -> Option<&'static str> {
    if part.is_empty() {
        Some("a path is names joined by `/`, with none at its start or end, and never two together")
    } else if part == "." || part == ".." {
        Some("`.` and `..` aren't names, and a path stays in its folder")
    } else if part
        .chars()
        .any(|c| matches!(c, '\\' | ':') || c.is_control())
    {
        Some("a name can't hold `\\`, `:` or control characters")
    } else if part.ends_with(['.', ' ']) {
        Some("a name can't end with `.` or a space, which Windows drops")
    } else if save::is_device(part) {
        Some("Windows keeps that name for a device")
    } else {
        None
    }
}

/// `path` as the system shows it: on Windows, without the `\\?\` that following its links puts
/// before it.
fn shown(path: &Path) -> PathBuf {
    if cfg!(windows)
        && let Some(text) = path.to_str()
    {
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\")
            && rest.as_bytes().get(1) == Some(&b':')
        {
            return PathBuf::from(rest);
        }
    }
    path.to_owned()
}

/// Whether `a` and `b`, both there, are the same, as two names differing only in case are where
/// cases aren't told apart.
fn same_entry(a: &Path, b: &Path) -> bool {
    let named_alike = match (a.to_str(), b.to_str()) {
        (Some(a), Some(b)) => a.to_lowercase() == b.to_lowercase(),
        _ => false,
    };
    named_alike
        && matches!(
            (fs::canonicalize(a), fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

/// Makes the folders `path` is in, if they aren't there.
fn make_parent(path: &Path) -> Result<()> {
    match path.parent() {
        Some(parent) => fs::create_dir_all(parent).map_err(|err| write_error(parent, err)),
        None => Ok(()),
    }
}

fn read_error(path: &Path, source: io::Error) -> Error {
    Error::ReadFile {
        path: shown(path),
        source,
    }
}

fn write_error(path: &Path, source: io::Error) -> Error {
    Error::WriteFile {
        path: shown(path),
        source,
    }
}

/// `time` in seconds since 1970 began, UTC, or 0 if it can't be told.
fn seconds(time: io::Result<SystemTime>) -> f64 {
    time.ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0.0, |since| since.as_secs_f64())
}

/// Copies the folder `from` to `to`, and everything in it, but links. `to` is made if it isn't
/// there.
pub(crate) fn copy_folder(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_folder(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// The folders the player has picked, and the dialog for picking another. Clones share them. See
/// the [module docs](self).
#[derive(Clone, Debug, Default)]
pub struct Folders(Arc<Mutex<Picking>>);

#[derive(Debug, Default)]
struct Picking {
    /// The folders picked, most recently first, as the system shows them.
    picked: Vec<PathBuf>,
    /// The file they're remembered in, if they're remembered after the game ends.
    file: Option<PathBuf>,
    dialog: Dialog,
    /// The last ticket a dialog was asked for with.
    tickets: u64,
    /// The answer for the next dialog, given before the game asked for it, as a test can.
    next: Option<Option<Folder>>,
}

/// The dialog for picking a folder, as the game asked for it with a ticket.
#[derive(Debug, Default)]
enum Dialog {
    #[default]
    Closed,
    /// Asked for, and not yet opened.
    Asked(u64),
    /// Open, as the window shows it.
    Open(u64),
    /// Closed, with the folder picked or none, until the game takes the answer.
    Answered(u64, Option<Folder>),
}

impl Folders {
    /// The folders picked by a game with `saves`, remembered in a file beside them, and read
    /// from it now. On the web, where there are none, none.
    pub(crate) fn kept_with(saves: &Saves) -> Self {
        if cfg!(target_arch = "wasm32") {
            return Self::default();
        }
        let file = saves.folder().map(|folder| folder.join(PICKED_FILE));
        let picked = file.as_deref().map(read_picked).unwrap_or_default();
        Self(Arc::new(Mutex::new(Picking {
            picked,
            file,
            ..Picking::default()
        })))
    }

    fn lock(&self) -> MutexGuard<'_, Picking> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The folders the player has picked and the game hasn't forgotten, most recently first:
    /// those that are still there.
    pub fn picked(&self) -> Vec<Folder> {
        let paths = self.lock().picked.clone();
        paths
            .iter()
            .filter_map(|path| Folder::at(path).ok())
            .collect()
    }

    /// Leaves `folder` out of the folders picked from now on, until it's picked again.
    pub fn forget(&self, folder: &Folder) {
        let mut picking = self.lock();
        picking.picked.retain(|path| *path != folder.shown);
        picking.keep();
    }

    /// Asks for the dialog for picking a folder, which a window opens once the frame is done,
    /// and returns the ticket [`Folders::answer_to`] tells the answer by. An answer the game
    /// hasn't taken from a dialog before is dropped. Fails while a dialog is open, or on the
    /// web.
    pub fn pick(&self) -> Result<u64> {
        if cfg!(target_arch = "wasm32") {
            return Err(Error::PickFolder(
                "there are no folders to pick on the web yet".into(),
            ));
        }
        let mut picking = self.lock();
        if matches!(picking.dialog, Dialog::Asked(_) | Dialog::Open(_)) {
            return Err(Error::PickFolder(
                "the dialog for it is open already".into(),
            ));
        }
        picking.tickets += 1;
        let ticket = picking.tickets;
        picking.dialog = match picking.next.take() {
            Some(folder) => Dialog::Answered(ticket, folder),
            None => Dialog::Asked(ticket),
        };
        Ok(ticket)
    }

    /// Whether the dialog is asked for or open.
    pub fn is_picking(&self) -> bool {
        matches!(self.lock().dialog, Dialog::Asked(_) | Dialog::Open(_))
    }

    /// The answer to the dialog asked for with `ticket`, once it's closed, which this takes: the
    /// folder picked, or `None` if the player picked none. `None` while it's open. A ticket whose
    /// answer was taken, or dropped, is answered with no folder.
    pub fn answer_to(&self, ticket: u64) -> Option<Option<Folder>> {
        let mut picking = self.lock();
        match picking.dialog {
            Dialog::Answered(answered, _) if answered == ticket => {
                let Dialog::Answered(_, folder) = mem::take(&mut picking.dialog) else {
                    unreachable!("it's answered");
                };
                Some(folder)
            }
            Dialog::Asked(asked) | Dialog::Open(asked) if asked == ticket => None,
            _ => Some(None),
        }
    }

    /// Whether a dialog has been asked for, which the window then opens, as this notes.
    pub(crate) fn open_asked(&self) -> bool {
        let mut picking = self.lock();
        match picking.dialog {
            Dialog::Asked(ticket) => {
                picking.dialog = Dialog::Open(ticket);
                true
            }
            _ => false,
        }
    }

    /// Closes the dialog with `picked`, remembering the folder. Without a dialog asked for, it's
    /// the answer for the next.
    pub(crate) fn answer(&self, picked: Option<Folder>) {
        let mut picking = self.lock();
        if let Some(folder) = &picked {
            picking.remember(folder);
        }
        match picking.dialog {
            Dialog::Asked(ticket) | Dialog::Open(ticket) => {
                picking.dialog = Dialog::Answered(ticket, picked);
            }
            _ => picking.next = Some(picked),
        }
    }
}

impl Picking {
    /// Puts `folder` first among the folders picked.
    fn remember(&mut self, folder: &Folder) {
        self.picked.retain(|path| *path != folder.shown);
        self.picked.insert(0, folder.shown.clone());
        self.picked.truncate(REMEMBERED);
        self.keep();
    }

    /// Writes the folders picked to their file, if they have one. Failing to only logs a warning:
    /// the game goes on with them as they are.
    fn keep(&self) {
        let Some(file) = &self.file else {
            return;
        };
        let paths = self
            .picked
            .iter()
            .filter_map(|path| path.to_str())
            .map(|path| Value::String(path.to_owned()))
            .collect();
        let written = write_value(&Value::List(paths))
            .map_err(|message| message.to_string())
            .and_then(|text| {
                make_parent(file)
                    .and_then(|()| save::write_replacing(file, text.as_bytes()))
                    .map_err(|err| err.to_string())
            });
        if let Err(err) = written {
            log::warn!("the folders picked weren't remembered: {err}");
        }
    }
}

/// The folders picked, as the file at `path` remembers them: none if there's no file, or it
/// can't be read, which logs a warning.
fn read_picked(path: &Path) -> Vec<PathBuf> {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(err) => {
            log::warn!(
                "can't read the folders picked from {}: {err}",
                path.display()
            );
            return Vec::new();
        }
    };
    match evaluate_any(&source, path) {
        Ok(Value::List(paths)) => paths
            .into_iter()
            .filter_map(|path| match path {
                Value::String(path) => Some(PathBuf::from(path)),
                _ => None,
            })
            .collect(),
        Ok(_) => {
            log::warn!("{} doesn't list folders", path.display());
            Vec::new()
        }
        Err(err) => {
            log::warn!("can't read the folders picked: {err}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::TemporaryFolder;

    /// A folder for a test, with `files` in it, and the temporary folder it's in.
    fn folder(files: &[(&str, &str)]) -> (Folder, TemporaryFolder) {
        let temporary = TemporaryFolder::new();
        let root = temporary.path().join("project");
        fs::create_dir_all(&root).unwrap();
        for (path, contents) in files {
            let file = root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, contents).unwrap();
        }
        (Folder::at(&root).unwrap(), temporary)
    }

    fn text(folder: &Folder, path: &str) -> Option<String> {
        let contents = folder.read(path).unwrap()?;
        Some(String::from_utf8(contents).unwrap())
    }

    fn paths(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.path.as_str()).collect()
    }

    #[test]
    fn files_are_read_written_and_replaced() {
        let (folder, _temporary) = folder(&[("wanderer.md", "# Hello\r\n")]);
        assert_eq!(folder.name(), "project");
        assert_eq!(text(&folder, "wanderer.md").as_deref(), Some("# Hello\r\n"));
        assert_eq!(folder.read("missing.md").unwrap(), None);

        folder
            .write("wanderer.md", "# Goodbye\n".as_bytes())
            .unwrap();
        assert_eq!(text(&folder, "wanderer.md").as_deref(), Some("# Goodbye\n"));
        folder.write("deep/er/down.md", b"\xff not UTF-8").unwrap();
        assert_eq!(
            folder.read("deep/er/down.md").unwrap().as_deref(),
            Some(&b"\xff not UTF-8"[..])
        );
        assert!(!folder.path().join("wanderer.md.partial").exists());
    }

    #[test]
    fn a_folder_that_has_gone_is_never_made_again() {
        let (folder, _temporary) = folder(&[("a.md", "a")]);
        let moved = folder.path().with_file_name("moved");
        fs::rename(folder.path(), &moved).unwrap();

        for err in [
            folder.write("b.md", b"b").unwrap_err(),
            folder.write("deep/b.md", b"b").unwrap_err(),
            folder.rename("a.md", "b.md").unwrap_err(),
        ] {
            let err = err.to_string();
            assert!(err.contains("the folder isn't there any more"), "{err}");
        }
        assert!(!folder.path().exists(), "the folder wasn't made again");
        // As if there were nothing in it.
        assert_eq!(folder.list("").unwrap(), None);
        assert_eq!(folder.read("a.md").unwrap(), None);
        folder.remove("a.md").unwrap();
        assert!(moved.join("a.md").exists());

        // Back where it was, it works again.
        fs::rename(&moved, folder.path()).unwrap();
        folder.write("b.md", b"b").unwrap();
        assert_eq!(text(&folder, "b.md").as_deref(), Some("b"));
    }

    #[test]
    fn folders_list_what_they_hold() {
        let (folder, _temporary) = folder(&[
            ("b.md", "bee"),
            ("a.md", ""),
            ("notes/c.md", "sea"),
            ("notes/deeper/d.md", ""),
        ]);
        let list = folder.list("").unwrap().unwrap();
        assert_eq!(paths(&list), ["a.md", "b.md", "notes"]);
        assert_eq!(list[1].kind, EntryKind::File);
        assert_eq!(list[1].size, 3);
        assert!(list[1].modified > 1.0e9, "{}", list[1].modified);
        assert_eq!(list[2].kind, EntryKind::Folder);
        assert_eq!(list[2].size, 0);
        let notes = folder.list("notes").unwrap().unwrap();
        assert_eq!(paths(&notes), ["notes/c.md", "notes/deeper"]);
        assert_eq!(folder.list("nowhere").unwrap(), None);
        let err = folder.list("a.md").unwrap_err().to_string();
        assert!(err.contains("is a file"), "{err}");
    }

    #[test]
    fn what_no_path_could_name_is_left_out_of_lists() {
        let (folder, _temporary) = folder(&[("kept.md", "")]);
        if cfg!(not(windows)) {
            // Names Windows can't keep at all.
            fs::write(folder.path().join("trailing."), "").unwrap();
            fs::write(folder.path().join("con.md"), "").unwrap();
            fs::write(folder.path().join("back\\slash"), "").unwrap();
        }
        let list = folder.list("").unwrap().unwrap();
        assert_eq!(paths(&list), ["kept.md"]);
    }

    #[test]
    fn paths_stay_in_their_folder() {
        let (folder, _temporary) = folder(&[("a.md", "")]);
        let bad = [
            ("", "names the folder itself"),
            ("/a.md", "with none at its start"),
            ("a.md/", "with none at its start or end"),
            ("notes//a.md", "never two together"),
            ("../a.md", "`..` aren't names"),
            ("./a.md", "`..` aren't names"),
            ("C:/a.md", "can't hold"),
            ("notes\\a.md", "can't hold"),
            ("a.md:stream", "can't hold"),
            ("tab\t.md", "can't hold"),
            ("a.md.", "can't end with"),
            ("a.md ", "can't end with"),
            ("con", "for a device"),
            ("notes/NUL.txt", "for a device"),
            ("com1.md", "for a device"),
        ];
        for (path, expected) in bad {
            let err = folder.read(path).unwrap_err().to_string();
            assert!(err.contains(expected), "{path:?}: {err}");
            assert!(folder.write(path, b"").is_err(), "{path:?}");
            assert!(folder.remove(path).is_err(), "{path:?}");
            assert!(folder.rename("a.md", path).is_err(), "{path:?}");
        }
        for path in [
            "console.md",
            "com10.md",
            "a.b.c",
            ".hidden",
            "with space.md",
            "é.md",
        ] {
            folder.write(path, b"fine").unwrap();
        }
    }

    #[test]
    fn links_out_of_the_folder_lead_nowhere() {
        let (folder, temporary) = folder(&[("inside.md", "in")]);
        let outside = temporary.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.md"), "out").unwrap();
        // Windows makes links only for administrators or in developer mode, or junctions, which
        // only lead to folders.
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside, folder.path().join("out")).is_ok()
            && std::os::unix::fs::symlink(
                outside.join("secret.md"),
                folder.path().join("secret.md"),
            )
            .is_ok()
            && std::os::unix::fs::symlink(
                folder.path().join("inside.md"),
                folder.path().join("in.md"),
            )
            .is_ok();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(&outside, folder.path().join("out")).is_ok()
            && std::os::windows::fs::symlink_file(
                outside.join("secret.md"),
                folder.path().join("secret.md"),
            )
            .is_ok()
            && std::os::windows::fs::symlink_file(
                folder.path().join("inside.md"),
                folder.path().join("in.md"),
            )
            .is_ok();
        if !linked {
            eprintln!("links can't be made here, so they aren't checked");
            return;
        }
        for path in ["out/secret.md", "secret.md"] {
            let err = folder.read(path).unwrap_err().to_string();
            assert!(err.contains("leads out of the folder"), "{path}: {err}");
            assert!(folder.write(path, b"changed").is_err(), "{path}");
        }
        assert!(folder.list("out").is_err());
        assert!(folder.write("out/new.md", b"").is_err());
        assert_eq!(
            fs::read_to_string(outside.join("secret.md")).unwrap(),
            "out"
        );
        assert!(!outside.join("new.md").exists());
        // A link in the folder to what's in it is followed, and listed as what it leads to.
        assert_eq!(text(&folder, "in.md").as_deref(), Some("in"));
        let list = folder.list("").unwrap().unwrap();
        assert_eq!(paths(&list), ["in.md", "inside.md"]);
        // A link is removed, not what it leads to.
        folder.remove("secret.md").unwrap();
        assert!(outside.join("secret.md").exists());
    }

    #[test]
    fn renaming_never_replaces_anything() {
        let (folder, _temporary) = folder(&[("a.md", "A"), ("b.md", "B")]);
        let err = folder.rename("a.md", "b.md").unwrap_err().to_string();
        assert!(err.contains("renaming never replaces"), "{err}");
        assert_eq!(text(&folder, "b.md").as_deref(), Some("B"));
        let err = folder.rename("missing.md", "c.md").unwrap_err().to_string();
        assert!(err.contains("nothing there"), "{err}");

        folder.rename("a.md", "moved/c.md").unwrap();
        assert_eq!(folder.read("a.md").unwrap(), None);
        assert_eq!(text(&folder, "moved/c.md").as_deref(), Some("A"));
        folder.rename("moved", "kept").unwrap();
        assert_eq!(text(&folder, "kept/c.md").as_deref(), Some("A"));
        // Changing only the case renames it, on any system.
        folder.rename("b.md", "B.md").unwrap();
        let list = folder.list("").unwrap().unwrap();
        assert_eq!(paths(&list), ["B.md", "kept"]);
    }

    #[test]
    fn removing_takes_only_files() {
        let (folder, _temporary) = folder(&[("a.md", ""), ("notes/b.md", "")]);
        folder.remove("a.md").unwrap();
        assert_eq!(folder.read("a.md").unwrap(), None);
        folder.remove("a.md").unwrap();
        let err = folder.remove("notes").unwrap_err().to_string();
        assert!(err.contains("is a folder"), "{err}");
        assert!(folder.write("notes", b"").is_err());
        assert!(folder.read("notes").is_err());
        assert!(folder.path().join("notes/b.md").exists());
    }

    #[test]
    fn folders_are_the_same_however_they_were_reached() {
        let (folder, _temporary) = folder(&[("notes/a.md", "")]);
        let again = Folder::at(folder.path().join("notes").join("..")).unwrap();
        assert_eq!(again, folder);
        assert_eq!(again.name(), "project");
        assert_ne!(Folder::at(folder.path().join("notes")).unwrap(), folder);
        assert!(Folder::at(folder.path().join("notes/a.md")).is_err());
        assert!(Folder::at(folder.path().join("missing")).is_err());
        if cfg!(windows) {
            let shown = folder.path().to_str().unwrap();
            assert!(!shown.starts_with(r"\\?\"), "{shown}");
        }
    }

    #[test]
    fn a_dialog_is_answered_by_the_ticket_it_was_asked_for_with() {
        let (folder, _temporary) = folder(&[]);
        let folders = Folders::default();
        assert!(folders.picked().is_empty());
        let ticket = folders.pick().unwrap();
        assert!(folders.is_picking());
        assert!(folders.pick().is_err(), "one at a time");
        assert_eq!(folders.answer_to(ticket), None);
        assert!(folders.open_asked());
        assert!(!folders.open_asked(), "once");
        assert_eq!(folders.answer_to(ticket), None);
        folders.answer(Some(folder.clone()));
        assert!(!folders.is_picking());
        assert_eq!(folders.answer_to(ticket), Some(Some(folder.clone())));
        assert_eq!(folders.answer_to(ticket), Some(None), "taken");
        assert_eq!(folders.picked(), vec![folder.clone()]);

        // Cancelled.
        let ticket = folders.pick().unwrap();
        folders.answer(None);
        assert_eq!(folders.answer_to(ticket), Some(None));

        // Answered before it's asked for, as a test answers.
        folders.answer(Some(folder.clone()));
        let ticket = folders.pick().unwrap();
        assert!(!folders.is_picking());
        assert_eq!(folders.answer_to(ticket), Some(Some(folder.clone())));

        // An answer not taken is dropped when another dialog is asked for.
        let old = folders.pick().unwrap();
        folders.answer(Some(folder.clone()));
        let new = folders.pick().unwrap();
        assert_eq!(folders.answer_to(old), Some(None));
        assert_eq!(folders.answer_to(new), None);
    }

    #[test]
    fn folders_picked_are_remembered_most_recently_first() {
        let saves_folder = TemporaryFolder::new();
        let saves = Saves::in_folder(saves_folder.path());
        let (a, _a) = folder(&[]);
        let (b, _b) = folder(&[]);
        let (gone, gone_folder) = folder(&[]);
        let folders = Folders::kept_with(&saves);
        for folder in [&gone, &a, &b, &a] {
            folders.pick().unwrap();
            folders.answer(Some(folder.clone()));
        }
        assert_eq!(folders.picked(), [a.clone(), b.clone(), gone.clone()]);
        drop(gone_folder);
        assert_eq!(
            folders.picked(),
            [a.clone(), b.clone()],
            "only those still there"
        );

        let again = Folders::kept_with(&saves);
        assert_eq!(again.picked(), [a.clone(), b.clone()]);
        again.forget(&a);
        assert_eq!(again.picked(), vec![b.clone()]);
        assert_eq!(Folders::kept_with(&saves).picked(), vec![b]);
        // Not a save.
        assert_eq!(saves.list().unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_file_of_folders_that_cant_be_read_remembers_none() {
        let saves_folder = TemporaryFolder::new();
        fs::create_dir_all(saves_folder.path()).unwrap();
        let saves = Saves::in_folder(saves_folder.path());
        fs::write(saves_folder.path().join(PICKED_FILE), "return {").unwrap();
        assert!(Folders::kept_with(&saves).picked().is_empty());
        fs::write(saves_folder.path().join(PICKED_FILE), "return 3").unwrap();
        assert!(Folders::kept_with(&saves).picked().is_empty());
    }

    #[test]
    fn copies_hold_what_the_folder_held() {
        let (folder, temporary) = folder(&[("a.md", "A"), ("notes/b.md", "B")]);
        let copy = temporary.path().join("copy");
        copy_folder(folder.path(), &copy).unwrap();
        let copy = Folder::at(copy).unwrap();
        assert_eq!(text(&copy, "a.md").as_deref(), Some("A"));
        assert_eq!(text(&copy, "notes/b.md").as_deref(), Some("B"));
    }
}
