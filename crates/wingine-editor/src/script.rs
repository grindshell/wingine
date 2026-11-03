//! A script open in the Script screen: its text as the editor has it, what's on disk, and what
//! the language server has found wrong with it.

use std::{fs, io, path::Path, time::SystemTime};

use crate::lsp::{self, Diagnostic};

pub struct Script {
    /// Its path in the project, with `/` between names.
    pub path: String,
    /// Its URI, as the language server names it.
    pub uri: String,
    /// What's in it now, with `\n` ending its lines whatever the file uses.
    pub text: String,
    /// What's in its file, as of the last time it was read or saved.
    saved: String,
    /// Whether its file ends lines with `\r\n`, which saving keeps.
    crlf: bool,
    modified: Option<SystemTime>,
    /// Counts its changes, for the language server.
    pub version: i64,
    /// The version the language server last heard of, if it has the script open.
    pub synced: Option<i64>,
    /// What the language server found wrong with it, last it said.
    pub diagnostics: Vec<Diagnostic>,
}

impl Script {
    /// Opens the script at `path` in the game in `folder`.
    pub fn open(folder: &Path, path: &str) -> io::Result<Self> {
        let file = folder.join(path);
        let (text, crlf) = read(&file)?;
        Ok(Self {
            path: path.to_owned(),
            uri: lsp::uri(&file),
            saved: text.clone(),
            text,
            crlf,
            modified: modified(&file),
            version: 1,
            synced: None,
            diagnostics: Vec::new(),
        })
    }

    /// Its file's name.
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    pub fn is_dirty(&self) -> bool {
        self.text != self.saved
    }

    /// Notes that its text has changed.
    pub fn changed(&mut self) {
        self.version += 1;
    }

    /// Writes it to its file in the game in `folder`.
    pub fn save(&mut self, folder: &Path) -> io::Result<()> {
        let file = folder.join(&self.path);
        let text = if self.crlf {
            self.text.replace('\n', "\r\n")
        } else {
            self.text.clone()
        };
        fs::write(&file, text)?;
        self.saved = self.text.clone();
        self.modified = modified(&file);
        Ok(())
    }

    /// Reads its file again if it has changed on disk and the script has no changes of its own,
    /// returning whether it did.
    pub fn reload_if_changed(&mut self, folder: &Path) -> bool {
        let file = folder.join(&self.path);
        let now = modified(&file);
        if now == self.modified || self.is_dirty() {
            return false;
        }
        self.modified = now;
        match read(&file) {
            Ok((text, crlf)) if text != self.text => {
                self.text = text.clone();
                self.saved = text;
                self.crlf = crlf;
                self.changed();
                true
            }
            _ => false,
        }
    }
}

/// The text of the file at `path`, with its lines ended by `\n`, and whether they were ended by
/// `\r\n`.
fn read(path: &Path) -> io::Result<(String, bool)> {
    let text = fs::read_to_string(path)?;
    let crlf = text.contains("\r\n");
    Ok((text.replace("\r\n", "\n"), crlf))
}

/// When the file at `path` was last changed, if it can be told.
pub(crate) fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
}
