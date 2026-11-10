//! Playing the game being edited, in a process of its own, so a game that fails or hangs can't
//! take the editor with it. The process is the editor's own executable run with `--run`, which
//! runs the game as `wingine --watch` would, so it reloads as the editor saves its files.

use std::{
    ffi::OsStr,
    io::{self, BufRead, BufReader, Read},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use wingine::Waker;

/// A game running in its own process, and what it prints.
pub struct Play {
    child: Child,
    lines: Receiver<String>,
    /// Whether the game has closed its output, as it does as it ends.
    closed: bool,
}

impl Play {
    /// Runs the game in `folder` with `runner`, an executable that runs the game it's given after
    /// `--run`: the editor itself. `env` sets environment variables for it. `waker` wakes the
    /// editor when the game prints something, and when it closes its output.
    pub fn start<'a>(
        runner: &Path,
        folder: &Path,
        env: impl IntoIterator<Item = (&'a OsStr, &'a OsStr)>,
        waker: Waker,
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
            let waker = waker.clone();
            thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    if sender.send(line).is_err() {
                        return;
                    }
                    waker.wake();
                }
                // The game is ending, which the editor looks for.
                drop(sender);
                waker.wake();
            });
        }
        Ok(Self {
            child,
            lines,
            closed: false,
        })
    }

    /// What the game has printed since this was last called, a line at a time.
    pub fn lines(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        loop {
            match self.lines.try_recv() {
                Ok(line) => lines.push(line),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.closed = true;
                    break;
                }
            }
        }
        lines
    }

    /// What the game printed that hasn't been taken yet, once it's ended: the lines the threads
    /// reading its output still hold too, waiting up to `timeout` for them to read the last.
    pub fn last_lines(&mut self, timeout: Duration) -> Vec<String> {
        let deadline = Instant::now() + timeout;
        let mut lines = Vec::new();
        while !self.closed {
            match self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(line) => lines.push(line),
                Err(RecvTimeoutError::Disconnected) => self.closed = true,
                Err(RecvTimeoutError::Timeout) => break,
            }
        }
        lines
    }

    /// Whether the game has closed its output, as it does as it ends, as of the last
    /// [`Play::lines`].
    pub fn is_closed(&self) -> bool {
        self.closed
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
