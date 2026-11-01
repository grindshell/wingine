//! A client of luau-lsp, the Luau language server, which the Script screen asks about the game's
//! scripts: their errors, what a name under the pointer is, what could be typed next, and the
//! call being typed. The server runs in a process of its own, spoken to over its standard input
//! and output in JSON-RPC messages, each after a `Content-Length` header. Threads write and read
//! them, so the editor never waits on the server: it asks, and picks the answers up a frame or
//! more later with [`LanguageServer::poll`].
//!
//! The server checks scripts against the engine's API through its definitions file,
//! [`wingine::runtime::DEFINITIONS`], which the editor writes to a folder of its own for it.

use std::{
    collections::HashMap,
    env, fs,
    io::{self, BufRead, BufReader, Write},
    ops::Range,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

/// Luau's settings for every game, under any `.luaurc` of the game's own. The engine calls a
/// game's global `update` and `draw`, which Luau would otherwise warn are never used.
pub const BASE_LUAURC: &str = r#"{ "lint": { "FunctionUnused": false } }"#;

/// What luau-lsp is run as, unless `WINGINE_LUAU_LSP` names another program: one on the `PATH`.
pub const PROGRAM: &str = "luau-lsp";

/// The language server to run: `WINGINE_LUAU_LSP` if it's set, or luau-lsp on the `PATH`.
pub fn program() -> PathBuf {
    env::var_os("WINGINE_LUAU_LSP").map_or_else(|| PROGRAM.into(), PathBuf::from)
}

/// A place in a script as the server counts: lines from 0, and characters along the line in
/// UTF-16 code units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
}

/// Something wrong with a script, or worth knowing about it.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub start: Position,
    pub end: Position,
    pub severity: Severity,
    pub message: String,
}

/// Something that could be typed where the cursor is.
#[derive(Clone, Debug, PartialEq)]
pub struct Completion {
    pub label: String,
    /// What it is, such as a function's type.
    pub detail: String,
    /// Its docs, in Markdown.
    pub documentation: String,
    /// What typing it puts in place of the word being typed.
    pub insert: String,
    /// Where it goes in the list.
    pub sort: String,
}

/// A function being called where the cursor is, and which of its parameters the cursor is at.
#[derive(Clone, Debug, PartialEq)]
pub struct Signature {
    pub label: String,
    /// The bytes of `label` that are the parameter the cursor is at.
    pub parameter: Option<Range<usize>>,
    /// Its docs, in Markdown.
    pub documentation: String,
}

/// Identifies a request to the server, which its answer comes back with.
pub type RequestId = i64;

/// What the server has said.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// What's wrong with the script at `uri` now, replacing what was before.
    Diagnostics {
        uri: String,
        diagnostics: Vec<Diagnostic>,
    },
    /// What's under a place, in Markdown, if anything.
    Hover {
        id: RequestId,
        contents: Option<String>,
    },
    Completions {
        id: RequestId,
        items: Vec<Completion>,
    },
    Signature {
        id: RequestId,
        signature: Option<Signature>,
    },
    /// The server ended, and why.
    Stopped(String),
}

/// What a request was for, to read its answer.
#[derive(Clone, Copy, Debug)]
enum Request {
    Initialize,
    Hover,
    Completion,
    Signature,
    Other,
}

enum Incoming {
    Message(Value),
    Closed(String),
}

/// luau-lsp running for a game's folder.
pub struct LanguageServer {
    child: Child,
    /// Framed messages for the thread that writes them.
    outgoing: Option<Sender<Vec<u8>>>,
    incoming: Receiver<Incoming>,
    next_id: RequestId,
    pending: HashMap<RequestId, Request>,
    /// Whether the server has answered `initialize`; until then, other messages wait.
    ready: bool,
    waiting: Vec<Value>,
    stopped: bool,
}

impl LanguageServer {
    /// Runs `program`, luau-lsp, for the game in `folder`.
    pub fn start(program: &Path, folder: &Path) -> io::Result<Self> {
        let (definitions, luaurc) = support_files()?;
        let folder = std::path::absolute(folder)?;
        let mut definitions_arg = std::ffi::OsString::from("--definitions=@wingine=");
        definitions_arg.push(&definitions);
        let mut luaurc_arg = std::ffi::OsString::from("--base-luaurc=");
        luaurc_arg.push(&luaurc);
        let mut child = Command::new(program)
            .arg("lsp")
            .arg(definitions_arg)
            .arg(luaurc_arg)
            .current_dir(&folder)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut stdin = child.stdin.take().expect("piped");
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");

        let (outgoing, to_write) = mpsc::channel::<Vec<u8>>();
        thread::spawn(move || {
            for message in to_write {
                if stdin
                    .write_all(&message)
                    .and_then(|()| stdin.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        let (sender, incoming) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let why = loop {
                match read_message(&mut reader) {
                    Ok(Some(message)) => {
                        if sender.send(Incoming::Message(message)).is_err() {
                            return;
                        }
                    }
                    Ok(None) => break "it closed its output".to_owned(),
                    Err(err) => break err.to_string(),
                }
            };
            let _ = sender.send(Incoming::Closed(why));
        });
        // What it prints for people, which would fill its pipe if no one read it.
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                log::debug!(target: "luau-lsp", "{line}");
            }
        });

        let mut server = Self {
            child,
            outgoing: Some(outgoing),
            incoming,
            next_id: 1,
            pending: HashMap::new(),
            ready: false,
            waiting: Vec::new(),
            stopped: false,
        };
        let root = uri(&folder);
        let name = folder
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        server.request(
            Request::Initialize,
            "initialize",
            json!({
                "processId": std::process::id(),
                "clientInfo": { "name": "wingine-editor" },
                "rootUri": root,
                "workspaceFolders": [{ "uri": root, "name": name }],
                "capabilities": {
                    "general": { "positionEncodings": ["utf-16"] },
                    "workspace": { "configuration": true },
                    "textDocument": {
                        "synchronization": { "didSave": false },
                        "publishDiagnostics": {},
                        "hover": { "contentFormat": ["markdown", "plaintext"] },
                        "completion": {
                            "completionItem": {
                                "snippetSupport": false,
                                "documentationFormat": ["markdown", "plaintext"],
                            },
                        },
                        "signatureHelp": {
                            "signatureInformation": {
                                "documentationFormat": ["markdown", "plaintext"],
                                "parameterInformation": { "labelOffsetSupport": true },
                            },
                        },
                    },
                },
            }),
        );
        Ok(server)
    }

    /// Whether the server has ended, which [`Event::Stopped`] tells.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Whether the server has started up and answers questions.
    pub fn is_ready(&self) -> bool {
        self.ready && !self.stopped
    }

    /// What the server has said since this was last called.
    pub fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(incoming) = self.incoming.try_recv() {
            match incoming {
                Incoming::Message(message) => self.receive(message, &mut events),
                Incoming::Closed(why) => self.stop(why, &mut events),
            }
        }
        if !self.stopped
            && let Ok(Some(status)) = self.child.try_wait()
        {
            self.stop(format!("it ended ({status})"), &mut events);
        }
        events
    }

    fn stop(&mut self, why: String, events: &mut Vec<Event>) {
        if !self.stopped {
            self.stopped = true;
            self.outgoing = None;
            events.push(Event::Stopped(why));
        }
    }

    /// Tells the server the script at `uri` is open, holding `text`, so it's checked as it is in
    /// the editor rather than on disk.
    pub fn open(&mut self, uri: &str, version: i64, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": { "uri": uri, "languageId": "luau", "version": version, "text": text },
            }),
        );
    }

    /// Tells the server the script at `uri` now holds `text`.
    pub fn change(&mut self, uri: &str, version: i64, text: &str) {
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": text }],
            }),
        );
    }

    /// Tells the server the script at `uri` is closed, so it's read from disk again.
    pub fn close(&mut self, uri: &str) {
        self.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        );
    }

    /// Asks what's at `at` in the script at `uri`, which [`Event::Hover`] answers.
    pub fn hover(&mut self, uri: &str, at: Position) -> Option<RequestId> {
        self.request(Request::Hover, "textDocument/hover", place(uri, at))
    }

    /// Asks what could be typed at `at`, which [`Event::Completions`] answers. `typed` is the
    /// character just typed, if it's one that starts completing, such as `.`.
    pub fn complete(&mut self, uri: &str, at: Position, typed: Option<char>) -> Option<RequestId> {
        let mut params = place(uri, at);
        params["context"] = match typed {
            Some(typed) => json!({ "triggerKind": 2, "triggerCharacter": typed.to_string() }),
            None => json!({ "triggerKind": 1 }),
        };
        self.request(Request::Completion, "textDocument/completion", params)
    }

    /// Asks what call `at` is in, which [`Event::Signature`] answers.
    pub fn signature(&mut self, uri: &str, at: Position) -> Option<RequestId> {
        self.request(
            Request::Signature,
            "textDocument/signatureHelp",
            place(uri, at),
        )
    }

    fn request(&mut self, kind: Request, method: &str, params: Value) -> Option<RequestId> {
        if self.stopped {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.pending.insert(id, kind);
        self.send(message(Some(id), method, params));
        Some(id)
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(message(None, method, params));
    }

    fn send(&mut self, message: Value) {
        let initializing = message["method"] == "initialize";
        if !self.ready && !initializing {
            self.waiting.push(message);
            return;
        }
        if let Some(outgoing) = &self.outgoing {
            let _ = outgoing.send(frame(&message));
        }
    }

    fn reply(&mut self, id: Value, result: Value) {
        let message = json!({ "jsonrpc": "2.0", "id": id, "result": result });
        if let Some(outgoing) = &self.outgoing {
            let _ = outgoing.send(frame(&message));
        }
    }

    fn receive(&mut self, message: Value, events: &mut Vec<Event>) {
        let method = message["method"].as_str();
        match (method, message.get("id")) {
            // A request of the server's own.
            (Some(method), Some(id)) => {
                let result = match method {
                    "workspace/configuration" => {
                        let items = message["params"]["items"].as_array().cloned();
                        let items = items.unwrap_or_default();
                        let settings = items.iter().map(|item| {
                            if item["section"] == "luau-lsp" {
                                settings()
                            } else {
                                Value::Null
                            }
                        });
                        Value::Array(settings.collect())
                    }
                    _ => Value::Null,
                };
                self.reply(id.clone(), result);
            }
            (Some("textDocument/publishDiagnostics"), None) => {
                let params = &message["params"];
                let diagnostics = params["diagnostics"].as_array().cloned();
                events.push(Event::Diagnostics {
                    uri: params["uri"].as_str().unwrap_or_default().to_owned(),
                    diagnostics: diagnostics
                        .unwrap_or_default()
                        .iter()
                        .filter_map(diagnostic)
                        .collect(),
                });
            }
            (Some("window/logMessage" | "window/showMessage"), None) => {
                let text = message["params"]["message"].as_str().unwrap_or_default();
                log::debug!(target: "luau-lsp", "{text}");
            }
            (Some(_), None) => {}
            // An answer to a request of ours.
            (None, Some(id)) => {
                let Some(id) = id.as_i64() else { return };
                let Some(kind) = self.pending.remove(&id) else {
                    return;
                };
                let result = &message["result"];
                match kind {
                    Request::Initialize => {
                        self.ready = true;
                        self.notify("initialized", json!({}));
                        for message in std::mem::take(&mut self.waiting) {
                            self.send(message);
                        }
                    }
                    Request::Hover => events.push(Event::Hover {
                        id,
                        contents: markup(&result["contents"]).filter(|text| !text.is_empty()),
                    }),
                    Request::Completion => events.push(Event::Completions {
                        id,
                        items: completions(result),
                    }),
                    Request::Signature => events.push(Event::Signature {
                        id,
                        signature: signature(result),
                    }),
                    Request::Other => {}
                }
            }
            (None, None) => {}
        }
    }
}

impl Drop for LanguageServer {
    fn drop(&mut self) {
        if !self.stopped {
            if self.ready {
                self.request(Request::Other, "shutdown", Value::Null);
                self.notify("exit", Value::Null);
            }
            // Closing its input ends it too, if it won't say goodbye.
            self.outgoing = None;
            let deadline = Instant::now() + Duration::from_millis(200);
            while Instant::now() < deadline {
                if let Ok(Some(_)) = self.child.try_wait() {
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// luau-lsp's settings, which it asks for as VS Code's extension would give them.
fn settings() -> Value {
    json!({
        "platform": { "type": "standard" },
        "sourcemap": { "enabled": false },
        "diagnostics": { "workspace": false },
        "completion": {
            "imports": { "enabled": false },
            "addParentheses": false,
            "fillCallArguments": false,
            "autocompleteEnd": false,
        },
        "inlayHints": { "parameterNames": "none", "variableTypes": false, "functionReturnTypes": false },
    })
}

/// Writes the definitions file and base `.luaurc` the server is run with, if they aren't written
/// already, to a folder of the editor's own named for what's in them, and returns their paths.
pub fn support_files() -> io::Result<(PathBuf, PathBuf)> {
    let contents = format!("{}{BASE_LUAURC}", wingine::runtime::DEFINITIONS);
    // FNV-1a, so editors of other versions of the engine each have their own.
    let hash = contents
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
        });
    let folder = env::temp_dir()
        .join("wingine-editor")
        .join(format!("{hash:016x}"));
    fs::create_dir_all(&folder)?;
    let definitions = folder.join("wingine.d.luau");
    let luaurc = folder.join("base.luaurc");
    for (path, contents) in [
        (&definitions, wingine::runtime::DEFINITIONS),
        (&luaurc, BASE_LUAURC),
    ] {
        if fs::read_to_string(path).ok().as_deref() != Some(contents) {
            fs::write(path, contents)?;
        }
    }
    Ok((definitions, luaurc))
}

/// A request, with its `id`, or a notification, leaving out `params` if there are none.
fn message(id: Option<RequestId>, method: &str, params: Value) -> Value {
    let mut message = json!({ "jsonrpc": "2.0", "method": method });
    if let Some(id) = id {
        message["id"] = id.into();
    }
    if !params.is_null() {
        message["params"] = params;
    }
    message
}

/// `message` as it's sent: its length, then its JSON.
fn frame(message: &Value) -> Vec<u8> {
    let body = message.to_string();
    let mut framed = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    framed.extend_from_slice(body.as_bytes());
    framed
}

/// Reads the next message from `reader`, or `None` at its end.
fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            if length.is_some() {
                break;
            }
            continue;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0; length.unwrap_or_default()];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn place(uri: &str, at: Position) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "position": { "line": at.line, "character": at.character },
    })
}

fn position(value: &Value) -> Option<Position> {
    Some(Position {
        line: value["line"].as_u64()? as u32,
        character: value["character"].as_u64()? as u32,
    })
}

fn diagnostic(value: &Value) -> Option<Diagnostic> {
    Some(Diagnostic {
        start: position(&value["range"]["start"])?,
        end: position(&value["range"]["end"])?,
        severity: match value["severity"].as_u64() {
            Some(2) => Severity::Warning,
            Some(3) => Severity::Information,
            Some(4) => Severity::Hint,
            _ => Severity::Error,
        },
        message: value["message"].as_str()?.to_owned(),
    })
}

/// Text the server gives as Markdown, plain text, or a list of either.
fn markup(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Object(object) => object.get("value")?.as_str().map(str::to_owned),
        Value::Array(parts) => {
            let parts: Vec<String> = parts.iter().filter_map(markup).collect();
            Some(parts.join("\n\n"))
        }
        _ => None,
    }
}

fn completions(result: &Value) -> Vec<Completion> {
    let items = match result {
        Value::Array(items) => items,
        Value::Object(list) => match list.get("items") {
            Some(Value::Array(items)) => items,
            _ => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| {
            let label = item["label"].as_str()?.to_owned();
            let insert = item["textEdit"]["newText"]
                .as_str()
                .or(item["insertText"].as_str())
                .unwrap_or(&label);
            let insert = if item["insertTextFormat"] == 2 {
                plain_snippet(insert)
            } else {
                insert.to_owned()
            };
            Some(Completion {
                detail: item["detail"].as_str().unwrap_or_default().to_owned(),
                documentation: markup(&item["documentation"]).unwrap_or_default(),
                insert,
                sort: item["sortText"].as_str().unwrap_or(&label).to_owned(),
                label,
            })
        })
        .collect()
}

/// A snippet's text without its placeholders: `f(${1:x})$0` is `f(x)`.
fn plain_snippet(snippet: &str) -> String {
    let mut plain = String::new();
    let mut chars = snippet.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => plain.extend(chars.next()),
            '$' if chars.peek() == Some(&'{') => {
                chars.next();
                while chars.peek().is_some_and(char::is_ascii_digit) {
                    chars.next();
                }
                if chars.peek() == Some(&':') {
                    chars.next();
                }
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                    plain.push(c);
                }
            }
            '$' if chars.peek().is_some_and(char::is_ascii_digit) => {
                while chars.peek().is_some_and(char::is_ascii_digit) {
                    chars.next();
                }
            }
            _ => plain.push(c),
        }
    }
    plain
}

fn signature(result: &Value) -> Option<Signature> {
    let signatures = result["signatures"].as_array()?;
    let active = result["activeSignature"].as_u64().unwrap_or(0) as usize;
    let signature = signatures.get(active).or(signatures.first())?;
    let label = signature["label"].as_str()?.to_owned();
    let parameter_index = signature["activeParameter"]
        .as_u64()
        .or(result["activeParameter"].as_u64());
    let parameter = parameter_index
        .and_then(|i| signature["parameters"].get(i as usize))
        .and_then(|parameter| match &parameter["label"] {
            Value::String(name) => {
                let start = label.find(name.as_str())?;
                Some(start..start + name.len())
            }
            Value::Array(offsets) => {
                let start = utf16_to_byte(&label, offsets.first()?.as_u64()? as usize);
                let end = utf16_to_byte(&label, offsets.get(1)?.as_u64()? as usize);
                Some(start..end)
            }
            _ => None,
        });
    Some(Signature {
        label,
        parameter,
        documentation: markup(&signature["documentation"]).unwrap_or_default(),
    })
}

/// The byte of `text` at `units` UTF-16 code units along it.
fn utf16_to_byte(text: &str, units: usize) -> usize {
    let mut counted = 0;
    for (byte, c) in text.char_indices() {
        if counted >= units {
            return byte;
        }
        counted += c.len_utf16();
    }
    text.len()
}

/// The URI of the file at `path`, as the server names files.
pub fn uri(path: &Path) -> String {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    let path = path.to_string_lossy().replace('\\', "/");
    let mut uri = String::from("file://");
    // A drive, on Windows, is the path's first part: `file:///C:/...`.
    let rest = match path.as_bytes() {
        [drive, b':', ..] if drive.is_ascii_alphabetic() => {
            uri.push('/');
            uri.push_str(&path[..2]);
            &path[2..]
        }
        _ => path.as_str(),
    };
    for byte in rest.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                uri.push(byte as char);
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

/// Whether two URIs name the same file, which the server and the editor may write differently:
/// escaping characters or not, and with drive letters in either case.
pub fn same_file(a: &str, b: &str) -> bool {
    file_key(a) == file_key(b)
}

fn file_key(uri: &str) -> String {
    let mut bytes = Vec::new();
    let mut rest = uri.bytes();
    while let Some(byte) = rest.next() {
        if byte == b'%' {
            let hex: String = rest.by_ref().take(2).map(char::from).collect();
            bytes.push(u8::from_str_radix(&hex, 16).unwrap_or(b'?'));
        } else {
            bytes.push(byte);
        }
    }
    let path = String::from_utf8_lossy(&bytes).replace('\\', "/");
    // Paths on Windows ignore case.
    if cfg!(windows) {
        path.to_lowercase()
    } else {
        path
    }
}

/// Where the character at `index`, counting in characters, is in `text`, as the server counts.
pub fn position_of(text: &str, index: usize) -> Position {
    let mut position = Position::default();
    for c in text.chars().take(index) {
        if c == '\n' {
            position.line += 1;
            position.character = 0;
        } else {
            position.character += c.len_utf16() as u32;
        }
    }
    position
}

/// The byte of `text` at `position`, or the end of its line or the text if it's past them.
pub fn byte_at(text: &str, position: Position) -> usize {
    let mut line_start = 0;
    for _ in 0..position.line {
        match text[line_start..].find('\n') {
            Some(end) => line_start += end + 1,
            None => return text.len(),
        }
    }
    let line_end = text[line_start..]
        .find('\n')
        .map_or(text.len(), |end| line_start + end);
    line_start + utf16_to_byte(&text[line_start..line_end], position.character as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_framed_by_their_length() {
        let message = json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} });
        let mut stream = frame(&message);
        stream.extend(frame(&json!({ "id": 1, "result": "é" })));
        let mut reader = io::Cursor::new(stream);
        assert_eq!(read_message(&mut reader).unwrap(), Some(message));
        assert_eq!(
            read_message(&mut reader).unwrap(),
            Some(json!({ "id": 1, "result": "é" }))
        );
        assert_eq!(read_message(&mut reader).unwrap(), None);
    }

    #[test]
    fn positions_count_utf16_along_lines() {
        let text = "a = 1\nb = \"é𝄞\" .. c\n";
        let c = text.find("c\n").unwrap();
        let char_index = text[..c].chars().count();
        // é is one unit, 𝄞 two.
        let at = position_of(text, char_index);
        assert_eq!(
            at,
            Position {
                line: 1,
                character: 13
            }
        );
        assert_eq!(byte_at(text, at), c);
        // Past the end of a line, the line's end.
        let past = Position {
            line: 0,
            character: 99,
        };
        assert_eq!(byte_at(text, past), 5);
        assert_eq!(
            byte_at(
                text,
                Position {
                    line: 9,
                    character: 0
                }
            ),
            text.len()
        );
    }

    #[test]
    fn uris_of_the_same_file_match() {
        if cfg!(windows) {
            assert!(same_file(
                "file:///c%3A/Users/me/my%20game/main.luau",
                "file:///C:/Users/me/my game/main.luau"
            ));
        }
        assert!(same_file(
            "file:///home/me/my%20game/main.luau",
            "file:///home/me/my game/main.luau"
        ));
        let path = std::path::absolute("some game/main.luau").unwrap();
        let uri = uri(&path);
        assert!(uri.starts_with("file:///"), "{uri}");
        assert!(uri.ends_with("/some%20game/main.luau"), "{uri}");
    }

    #[test]
    fn answers_are_read_from_the_forms_they_take() {
        let items = completions(&json!({ "isIncomplete": false, "items": [
            { "label": "clear", "detail": "(Color) -> ()", "sortText": "1",
              "documentation": { "kind": "markdown", "value": "Fills the canvas." } },
            { "label": "draw", "insertText": "draw(${1:x})$0", "insertTextFormat": 2 },
        ] }));
        assert_eq!(items[0].insert, "clear");
        assert_eq!(items[0].documentation, "Fills the canvas.");
        assert_eq!(items[1].insert, "draw(x)");
        assert_eq!(items[1].sort, "draw");

        let help = signature(&json!({
            "activeSignature": 0,
            "activeParameter": 1,
            "signatures": [{
                "label": "function f(a: number, b: string): ()",
                "parameters": [{ "label": [11, 20] }, { "label": [22, 31] }],
            }],
        }))
        .unwrap();
        assert_eq!(&help.label[help.parameter.unwrap()], "b: string");

        assert_eq!(
            markup(&json!([{ "language": "luau", "value": "x" }, "y"])).as_deref(),
            Some("x\n\ny")
        );
    }
}
