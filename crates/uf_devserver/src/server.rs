//! Binding, state, and the accept loop.
//!
//! # Threat model
//!
//! Two properties are decided here and nowhere else.
//!
//! **Exposure is read from the socket, not from the configuration string.**
//! [`DevServer::bind`] classifies the server by asking the *bound address*
//! whether it is loopback. A config that says `127.0.0.1` but resolves to a
//! routable interface is treated as exposed, because the socket is the truth.
//! An exposed bind with no `dev.allowedHosts` list fails to start.
//!
//! **Every connection is bounded.** Read and write timeouts, a ceiling on the
//! request head, and a single response per connection. A dev server that can be
//! held open by a slow client is a dev server an attacker can wedge.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uf_config::DevConfig;

use crate::http::{MAX_REQUEST_HEAD_BYTES, RequestHead, Response, Status, respond};
use crate::network::{Exposure, NetworkPolicy, NetworkPolicyError};
use crate::policy::{FsPolicy, PolicyError};

#[cfg(test)]
mod tests;

/// Directory, relative to the project root, holding dev server state.
pub const STATE_DIR: &str = ".uf";

/// File, inside [`STATE_DIR`], describing the running server.
pub const STATE_FILE: &str = "dev-server.json";

/// The engine name reported in the state file.
pub const ENGINE: &str = "uf-native";

/// The plugin contract this server implements.
pub const PLUGIN_CONTRACT: &str = "uf-plugin-v1";

/// How long a single connection may take to send its head or read its body.
pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(15);

/// Why the dev server could not start or could not serve.
#[derive(Debug, Error)]
pub enum DevServerError {
    /// The listener could not be bound.
    #[error("failed to bind {host}:{port}: {message}")]
    Bind {
        /// The requested host.
        host: CompactString,
        /// The requested port.
        port: u16,
        /// The underlying failure.
        message: CompactString,
    },
    /// The filesystem policy is invalid.
    #[error(transparent)]
    Policy(#[from] PolicyError),
    /// The network policy is invalid.
    #[error(transparent)]
    Network(#[from] NetworkPolicyError),
    /// The state file could not be written.
    #[error("failed to write {path}: {message}")]
    State {
        /// The file that could not be written.
        path: Utf8PathBuf,
        /// The underlying failure.
        message: CompactString,
    },
    /// The listener failed.
    #[error("dev server listener failed: {message}")]
    Listener {
        /// The underlying failure.
        message: CompactString,
    },
}

/// The contents of `.uf/dev-server.json`.
///
/// The access-control posture is part of the state file on purpose: a developer
/// who wants to know what their dev server will serve should be able to read it
/// rather than infer it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevServerState {
    /// The bound address.
    pub host: CompactString,
    /// The bound port.
    pub port: u16,
    /// Always `uf-native`.
    pub engine: CompactString,
    /// The plugin contract this server implements.
    pub plugin_contract: CompactString,
    /// The health endpoint.
    pub health: CompactString,
    /// Whether the socket is reachable beyond loopback.
    pub exposure: Exposure,
    /// The configured host allowlist.
    pub allowed_hosts: Vec<CompactString>,
    /// The configured origin allowlist.
    pub allowed_origins: Vec<CompactString>,
    /// The canonical roots that may be served.
    pub fs_allow: Vec<Utf8PathBuf>,
    /// The deny patterns applied to every canonical path.
    pub fs_deny: Vec<CompactString>,
}

/// A bound, access-controlled development server.
#[derive(Debug)]
pub struct DevServer {
    listener: TcpListener,
    address: SocketAddr,
    fs: FsPolicy,
    network: NetworkPolicy,
}

impl DevServer {
    /// Bind a dev server for `root` using `config`.
    ///
    /// # Errors
    ///
    /// Returns [`DevServerError`] when the socket cannot be bound, when the
    /// filesystem policy is invalid, or when an exposed bind has no allowed
    /// hosts.
    pub fn bind(root: &Utf8Path, config: &DevConfig) -> Result<Self, DevServerError> {
        let listener = bind_listener(config.host.as_str(), config.port, config.strict_port)?;
        let address = listener
            .local_addr()
            .map_err(|error| DevServerError::Bind {
                host: config.host.clone(),
                port: config.port,
                message: CompactString::new(error.to_string()),
            })?;

        let fs = FsPolicy::new(root, &config.fs.allow, &config.fs.deny)?;
        let exposure = if address.ip().is_loopback() {
            Exposure::Loopback
        } else {
            Exposure::Exposed
        };
        let network = NetworkPolicy::new(exposure, &config.allowed_hosts, &config.allowed_origins)?;

        Ok(Self {
            listener,
            address,
            fs,
            network,
        })
    }

    /// The address the listener actually bound.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// The filesystem policy in force.
    pub fn fs_policy(&self) -> &FsPolicy {
        &self.fs
    }

    /// The network policy in force.
    pub fn network_policy(&self) -> &NetworkPolicy {
        &self.network
    }

    /// Describe the running server.
    pub fn state(&self) -> DevServerState {
        DevServerState {
            host: CompactString::new(self.address.ip().to_string()),
            port: self.address.port(),
            engine: CompactString::const_new(ENGINE),
            plugin_contract: CompactString::const_new(PLUGIN_CONTRACT),
            health: CompactString::const_new(crate::http::HEALTH_TARGET),
            exposure: self.network.exposure(),
            allowed_hosts: self
                .network
                .allowed_hosts()
                .map(CompactString::new)
                .collect(),
            allowed_origins: self
                .network
                .allowed_origins()
                .map(CompactString::new)
                .collect(),
            fs_allow: self.fs.roots().to_vec(),
            fs_deny: self.fs.deny_patterns().map(CompactString::new).collect(),
        }
    }

    /// Write `.uf/dev-server.json` under `root` and return its path.
    ///
    /// # Errors
    ///
    /// Returns [`DevServerError::State`] if the directory or file cannot be
    /// written.
    pub fn write_state(&self, root: &Utf8Path) -> Result<Utf8PathBuf, DevServerError> {
        let directory = root.join(STATE_DIR);
        let path = directory.join(STATE_FILE);
        std::fs::create_dir_all(&directory).map_err(|error| DevServerError::State {
            path: directory.clone(),
            message: CompactString::new(error.to_string()),
        })?;
        let mut json =
            serde_json::to_string_pretty(&self.state()).map_err(|error| DevServerError::State {
                path: path.clone(),
                message: CompactString::new(error.to_string()),
            })?;
        json.push('\n');
        std::fs::write(&path, json).map_err(|error| DevServerError::State {
            path: path.clone(),
            message: CompactString::new(error.to_string()),
        })?;
        Ok(path)
    }

    /// Accept one connection, answer it, and return the status that was sent.
    ///
    /// # Errors
    ///
    /// Returns [`DevServerError::Listener`] only when `accept` itself fails. A
    /// connection that misbehaves gets a refusal, not a server shutdown.
    pub fn serve_next(&self) -> Result<Status, DevServerError> {
        let (stream, _) = self
            .listener
            .accept()
            .map_err(|error| DevServerError::Listener {
                message: CompactString::new(error.to_string()),
            })?;
        Ok(self.serve_connection(stream))
    }

    /// Serve until the listener fails.
    ///
    /// # Errors
    ///
    /// Returns [`DevServerError::Listener`] when `accept` fails.
    pub fn serve_forever(&self) -> Result<(), DevServerError> {
        loop {
            self.serve_next()?;
        }
    }

    fn serve_connection(&self, mut stream: TcpStream) -> Status {
        let _ = stream.set_read_timeout(Some(CONNECTION_TIMEOUT));
        let _ = stream.set_write_timeout(Some(CONNECTION_TIMEOUT));
        let response = match read_head(&mut stream) {
            Ok(head) => match RequestHead::parse(&head) {
                Ok(request) => respond(&request, &self.fs, &self.network),
                Err(_) => Response::refusal(Status::BadRequest),
            },
            Err(status) => Response::refusal(status),
        };
        let status = response.status;
        let _ = stream.write_all(&response.to_bytes());
        let _ = stream.flush();
        status
    }
}

fn bind_listener(host: &str, port: u16, strict_port: bool) -> Result<TcpListener, DevServerError> {
    match TcpListener::bind((host, port)) {
        Ok(listener) => Ok(listener),
        Err(_) if !strict_port => {
            TcpListener::bind((host, 0)).map_err(|error| DevServerError::Bind {
                host: CompactString::new(host),
                port,
                message: CompactString::new(error.to_string()),
            })
        }
        Err(error) => Err(DevServerError::Bind {
            host: CompactString::new(host),
            port,
            message: CompactString::new(error.to_string()),
        }),
    }
}

/// Read up to the blank line that ends the request head.
///
/// Bounded twice over: the buffer stops growing at [`MAX_REQUEST_HEAD_BYTES`],
/// and the socket has a read timeout, so neither a large head nor a slow one
/// can hold the accept loop.
fn read_head(stream: &mut TcpStream) -> Result<Vec<u8>, Status> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(end) = find_head_end(&buffer) {
            buffer.truncate(end);
            return Ok(buffer);
        }
        if buffer.len() >= MAX_REQUEST_HEAD_BYTES {
            return Err(Status::BadRequest);
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Err(Status::BadRequest),
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
            Err(_) => return Err(Status::BadRequest),
        }
    }
}

fn find_head_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}
