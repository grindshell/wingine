//! Playing the game being edited, in a process of its own, so a game that fails or hangs can't
//! take the editor with it. The process is the editor's own executable run with `--run`, which
//! runs the game as the `wingine` executable would.

use std::{
    ffi::OsStr,
    io::{self, BufRead, BufReader, Read},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
};

/// A game running in its own process, and what it prints.
pub struct Play {
    child: Child,
    lines: Receiver<String>,
}

impl Play {
    /// Runs the game in `folder` with `runner`, an executable that runs the game it's given after
    /// `--run`: the editor itself. `env` sets environment variables for it.
    pub fn start<'a>(
        runner: &Path,
        folder: &Path,
        env: impl IntoIterator<Item = (&'a OsStr, &'a OsStr)>,
    ) -> io::Result<Self> {
        let mut child = Command::new(runner)
            .arg("--run")
            .arg(folder)
            .envs(env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let (sender, lines) = mpsc::channel();
        let stdout = child
            .stdout
            .take()
            .map(|out| Box::new(out) as Box<dyn Read + Send>);
        let stderr = child
            .stderr
            .take()
            .map(|err| Box::new(err) as Box<dyn Read + Send>);
        for stream in [stdout, stderr].into_iter().flatten() {
            let sender = sender.clone();
            thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            });
        }
        Ok(Self { child, lines })
    }

    /// What the game has printed since this was last called, a line at a time.
    pub fn lines(&self) -> impl Iterator<Item = String> + '_ {
        self.lines.try_iter()
    }

    /// How the game ended, once it has.
    pub fn exited(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    /// Ends the game.
    pub fn stop(&mut self) {
        // It may have ended already, which is fine.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Play {
    fn drop(&mut self) {
        self.stop();
    }
}
