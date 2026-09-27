//! Official Flow type services beside uf's formatting, linting and config services.
use super::{Frame, read_message, write_message};
use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    io::BufReader,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
use uf_config::UniflowedConfig;

pub(super) struct Server {
    child: Child,
    input: ChildStdin,
    frames: mpsc::Receiver<Result<Frame>>,
    next: u64,
    open: BTreeMap<String, (i64, bool)>,
    diagnostics: VecDeque<(String, Vec<Value>)>,
    binary: Utf8PathBuf,
    root: Utf8PathBuf,
    temporary: tempfile::TempDir,
}

impl Server {
    pub(super) fn start(root: &Utf8Path, config: &UniflowedConfig) -> Result<Self> {
        let version = config
            .flow
            .version
            .as_deref()
            .context("flow.version is absent")?;
        let binary = crate::commands::flow::acquire(version)?;
        crate::commands::flow::configure(root, config)?;
        let cache = root.join(".uf/cache/official-flow");
        std::fs::create_dir_all(&cache)?;
        let temporary = tempfile::tempdir_in(cache)?;
        let mut child = Command::new(&binary)
            .current_dir(root)
            .args([
                "lsp",
                "--flowconfig-name",
                crate::commands::flow::CONFIG_NAME,
                "--temp-dir",
            ])
            .arg(temporary.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let input = child.stdin.take().context("missing Flow input")?;
        let output = child.stdout.take().context("missing Flow output")?;
        let (send, frames) = mpsc::channel();
        std::thread::Builder::new()
            .name("uf-official-flow-reader".into())
            .spawn(move || {
                let mut reader = BufReader::new(output);
                loop {
                    let frame = match read_message(&mut reader) {
                        Ok(Some(frame)) => Ok(frame),
                        Ok(None) => break,
                        Err(error) => Err(error),
                    };
                    let failed = frame.is_err();
                    if send.send(frame).is_err() || failed {
                        break;
                    }
                }
            })?;
        let mut server = Self {
            child,
            input,
            frames,
            next: 1,
            open: BTreeMap::new(),
            diagnostics: VecDeque::new(),
            binary,
            root: root.to_owned(),
            temporary,
        };
        let uri = path_uri(root);
        server.request(
            "initialize",
            json!({
                "processId": std::process::id(), "rootUri": uri,
                "capabilities": { "textDocument": {
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "completion": { "completionItem": { "snippetSupport": false } },
                    "publishDiagnostics": { "relatedInformation": true }
                }, "workspace": { "configuration": false } },
                "workspaceFolders": [{ "uri": uri, "name": root.file_name().unwrap_or("project") }]
            }),
        )?;
        server.notify("initialized", json!({}))?;
        Ok(server)
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        write_message(
            &mut self.input,
            &json!({ "jsonrpc": "2.0", "method": method, "params": params }),
        )
    }

    pub(super) fn changed(&mut self, uri: &str, text: &str) -> Result<()> {
        let opted_out = crate::commands::flow::opted_out(text);
        let version = self.open.get(uri).map_or(1, |(v, _)| v + 1);
        let method = if self.open.contains_key(uri) {
            "textDocument/didChange"
        } else {
            "textDocument/didOpen"
        };
        let params = if version == 1 {
            json!({ "textDocument": { "uri": uri, "languageId": "javascript", "version": version, "text": text } })
        } else {
            json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": [{ "text": text }] })
        };
        self.open.insert(uri.to_owned(), (version, opted_out));
        if opted_out {
            self.diagnostics.push_back((uri.to_owned(), Vec::new()));
        }
        self.notify(method, params)
    }

    pub(super) fn closed(&mut self, uri: &str) -> Result<()> {
        self.open.remove(uri);
        self.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": uri } }),
        )
    }

    /// Request ids stay on this private connection; client ids never collide.
    pub(super) fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        if params
            .pointer("/textDocument/uri")
            .and_then(Value::as_str)
            .is_some_and(|uri| self.open.get(uri).is_some_and(|(_, opt)| *opt))
        {
            return Ok(Value::Null);
        }
        let deadline = Instant::now() + Duration::from_secs(60);
        'connecting: loop {
            let id = self.next;
            self.next += 1;
            write_message(
                &mut self.input,
                &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": &params }),
            )?;
            loop {
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .context("official Flow request timed out")?;
                let frame = self
                    .frames
                    .recv_timeout(remaining)
                    .context("official Flow stopped or timed out")??;
                let Frame::Message(message) = frame else {
                    bail!("official Flow sent malformed JSON")
                };
                if message.get("id").and_then(Value::as_u64) == Some(id)
                    && message.get("method").is_none()
                {
                    if let Some(error) = message.get("error") {
                        // Flow acknowledges initialize before its type server connects.
                        // Flow reconnects on a one-second idle tick. Leave time for that
                        // tick; frequent retries otherwise keep the server disconnected.
                        if error["code"] == -32800
                            && error["message"]
                                .as_str()
                                .is_some_and(|m| m.starts_with("Server not connected"))
                        {
                            std::thread::sleep(Duration::from_millis(1250).min(remaining));
                            continue 'connecting;
                        }
                        bail!("official Flow: {error}")
                    }
                    return Ok(message.get("result").cloned().unwrap_or(Value::Null));
                }
                self.receive(message)?;
            }
        }
    }

    fn receive(&mut self, message: Value) -> Result<()> {
        match message["method"].as_str() {
            Some("textDocument/publishDiagnostics") => {
                if let (Some(uri), Some(found)) = (
                    message.pointer("/params/uri").and_then(Value::as_str),
                    message
                        .pointer("/params/diagnostics")
                        .and_then(Value::as_array),
                ) {
                    let opted_out = self.open.get(uri).is_some_and(|(_, opt)| *opt);
                    self.diagnostics.retain(|(pending, _)| pending != uri);
                    self.diagnostics.push_back((
                        uri.to_owned(),
                        if opted_out { Vec::new() } else { found.clone() },
                    ));
                }
            }
            Some(_) if message.get("id").is_some() => {
                // No dynamic registrations or client configuration were advertised.
                let result = if message["method"] == "workspace/configuration" {
                    let count = message
                        .pointer("/params/items")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len);
                    Value::Array(vec![Value::Null; count])
                } else {
                    Value::Null
                };
                write_message(
                    &mut self.input,
                    &json!({ "jsonrpc": "2.0", "id": message["id"], "result": result }),
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn poll(&mut self) -> Result<Option<(String, Vec<Value>)>> {
        while let Ok(frame) = self.frames.try_recv() {
            if let Frame::Message(message) = frame? {
                self.receive(message)?;
            }
        }
        Ok(self.diagnostics.pop_front())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.notify("exit", Value::Null);
        let _ = self.child.kill();
        let _ = self.child.wait();
        // This server's private temp directory keeps shutdown isolated from other editors.
        if let Ok(mut child) = Command::new(&self.binary)
            .current_dir(&self.root)
            .args([
                "stop",
                "--flowconfig-name",
                crate::commands::flow::CONFIG_NAME,
                "--temp-dir",
            ])
            .arg(self.temporary.path())
            .arg(&self.root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            let deadline = Instant::now() + Duration::from_secs(2);
            while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn path_uri(path: &Utf8Path) -> String {
    let mut uri = String::from("file://");
    if !path.as_str().starts_with('/') {
        uri.push('/');
    }
    const HEX: &[u8] = b"0123456789ABCDEF";
    for byte in path.as_str().bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.~:".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            uri.push('%');
            uri.push(char::from(HEX[(byte >> 4) as usize]));
            uri.push(char::from(HEX[(byte & 15) as usize]));
        }
    }
    uri
}
