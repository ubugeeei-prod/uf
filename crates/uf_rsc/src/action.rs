//! Server action identity and the callable-endpoint registry.
//!
//! # Threat model
//!
//! A server action is a function the browser can invoke by id over the network.
//! Everything about how that id is built is therefore security-relevant, and the
//! failure modes below are the ones that have produced real CVEs in other React
//! frameworks:
//!
//! * **Guessable ids.** If the id is the module path plus the export name, or a
//!   plain hash of them, anyone who can read the source — an open-source app, a
//!   published sourcemap — can compute the id of *any* function and call it,
//!   including functions the UI never exposes. [`ActionId`] is therefore an
//!   HMAC-SHA256 keyed with the per-build [`BuildId`]: without the build id, an
//!   attacker cannot derive an id even with the whole repository in hand.
//! * **Ids that survive a rebuild.** Reusing an id across builds lets an
//!   attacker replay an id captured from an older, more permissive deployment.
//!   A new build id changes every action id.
//! * **Oracles in the error path.** Distinguishing "no such module" from "no
//!   such export" from "that action exists but is not callable" turns the
//!   endpoint into an enumeration oracle. [`ServerActionRegistry::resolve`]
//!   answers every failure with the same [`UnknownAction`], and scans the whole
//!   table with a constant-time comparison so the *timing* does not answer
//!   either.
//! * **Dead actions left dialable.** A `"use server"` export nothing reaches is
//!   still an endpoint if it is registered. Actions that no client boundary can
//!   reach are recorded as [`ActionExposure::UnreachableFromClient`] and are
//!   never resolvable.
//!
//! # Id construction
//!
//! ```text
//! id = HMAC-SHA256(
//!         key     = build id bytes,
//!         message = "uf-rsc-action-v1"
//!                || u32be(len(module)) || module   // path relative to the project root
//!                || u32be(len(export)) || export   // export name, or the closure's binding
//!                || kind                           // 0x01 module export, 0x02 inline closure
//!      )
//! ```
//!
//! Every variable-length field is length-prefixed, so no two different tuples
//! can produce the same message, and the domain-separation prefix keeps these
//! digests distinct from any other HMAC the toolchain computes with the same key.

use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::BuildHasher;
use std::time::{SystemTime, UNIX_EPOCH};

use compact_str::CompactString;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;

mod crypto;
mod registry;

pub use registry::{ServerAction, ServerActionRegistry};

use crypto::{constant_time_eq, hex, hex_value, hmac_sha256, os_entropy};

/// Domain separation prefix for action digests.
const ACTION_DOMAIN: &[u8] = b"uf-rsc-action-v1";

/// Domain separation prefix for the manifest fingerprint.
const FINGERPRINT_DOMAIN: &[u8] = b"uf-rsc-manifest-fingerprint-v1";

/// Shortest accepted build id, in bytes.
///
/// A one-character build id is not a key; it is a formality. Anything shorter
/// than this is rejected rather than silently accepted as a weak HMAC key.
pub const MIN_BUILD_ID_BYTES: usize = 8;

/// Longest accepted build id, in bytes.
pub const MAX_BUILD_ID_BYTES: usize = 256;

/// Length of an action id in hexadecimal characters.
pub const ACTION_ID_HEX_LEN: usize = 64;

/// Why a build id was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum BuildIdError {
    /// The build id is too short to be used as an HMAC key.
    #[error("build id must be at least {MIN_BUILD_ID_BYTES} bytes, got {len}")]
    TooShort {
        /// Length that was supplied.
        len: usize,
    },
    /// The build id is longer than [`MAX_BUILD_ID_BYTES`].
    #[error("build id must be at most {MAX_BUILD_ID_BYTES} bytes, got {len}")]
    TooLong {
        /// Length that was supplied.
        len: usize,
    },
}

/// Why an action id string was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ActionIdError {
    /// The string is not exactly [`ACTION_ID_HEX_LEN`] characters.
    #[error("action id must be {ACTION_ID_HEX_LEN} hex characters, got {len}")]
    InvalidLength {
        /// Length that was supplied.
        len: usize,
    },
    /// The string contains something other than lowercase hexadecimal.
    #[error("action id must be lowercase hexadecimal")]
    InvalidCharacter,
}

/// The single error every failed action lookup produces.
///
/// It deliberately carries nothing: no id, no module, no reason. Any detail here
/// would be an enumeration oracle for whoever is probing the endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Error)]
#[error("unknown server action")]
pub struct UnknownAction;

/// The per-build secret that keys every action id.
///
/// Treat it like a signing key: it must not be written into the manifest, logged,
/// or shipped to the browser. [`ServerActionRegistry`] keeps only a fingerprint
/// of it once the ids are computed.
#[derive(Clone, PartialEq, Eq)]
pub struct BuildId {
    value: CompactString,
}

impl BuildId {
    /// Accept a caller-supplied build id.
    pub fn new(value: impl Into<CompactString>) -> Result<Self, BuildIdError> {
        let value = value.into();
        let len = value.len();
        if len < MIN_BUILD_ID_BYTES {
            return Err(BuildIdError::TooShort { len });
        }
        if len > MAX_BUILD_ID_BYTES {
            return Err(BuildIdError::TooLong { len });
        }
        Ok(Self { value })
    }

    /// Generate a fresh build id from operating-system entropy.
    ///
    /// Falls back to mixing the process id, the wall clock and two
    /// `RandomState` seeds when `/dev/urandom` is unavailable. Set `UF_BUILD_ID`
    /// instead when a build must be reproducible; see
    /// [`BuildId::from_env_or_generate`].
    pub fn generate() -> Self {
        let mut entropy = [0u8; 32];
        if !os_entropy(&mut entropy) {
            let mut hasher = Sha256::new();
            hasher.update(b"uf-rsc-build-id-fallback");
            hasher.update(std::process::id().to_le_bytes());
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default();
            hasher.update(nanos.to_le_bytes());
            hasher.update(RandomState::new().hash_one(0u64).to_le_bytes());
            hasher.update(RandomState::new().hash_one(1u64).to_le_bytes());
            entropy.copy_from_slice(&hasher.finalize());
        }
        Self {
            value: hex(&entropy),
        }
    }

    /// Read `UF_BUILD_ID` when it is set and usable, otherwise generate one.
    pub fn from_env_or_generate() -> Self {
        std::env::var("UF_BUILD_ID")
            .ok()
            .and_then(|value| Self::new(value).ok())
            .unwrap_or_else(Self::generate)
    }

    /// The build id bytes, used as the HMAC key.
    fn key(&self) -> &[u8] {
        self.value.as_bytes()
    }

    /// A digest of the build id that is safe to publish.
    pub fn fingerprint(&self) -> CompactString {
        hex(&hmac_sha256(self.key(), FINGERPRINT_DOMAIN))
    }
}

impl fmt::Debug for BuildId {
    /// Redacted: a build id that reaches a log is a leaked key.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BuildId(<redacted>)")
    }
}

/// The public identifier of one server action.
#[derive(Clone, Copy)]
pub struct ActionId([u8; 32]);

impl ActionId {
    /// Derive the id of an action.
    pub fn derive(build_id: &BuildId, module: &str, export: &str, kind: ServerActionKind) -> Self {
        let mut message = Vec::with_capacity(ACTION_DOMAIN.len() + module.len() + export.len() + 9);
        message.extend_from_slice(ACTION_DOMAIN);
        message.extend_from_slice(&(module.len() as u32).to_be_bytes());
        message.extend_from_slice(module.as_bytes());
        message.extend_from_slice(&(export.len() as u32).to_be_bytes());
        message.extend_from_slice(export.as_bytes());
        message.push(kind.tag());
        Self(hmac_sha256(build_id.key(), &message))
    }

    /// Parse the canonical lowercase hexadecimal form.
    pub fn parse(text: &str) -> Result<Self, ActionIdError> {
        // Length is checked before anything is allocated, so an oversized id
        // cannot be used to force work or memory on the server.
        if text.len() != ACTION_ID_HEX_LEN {
            return Err(ActionIdError::InvalidLength { len: text.len() });
        }
        let mut bytes = [0u8; 32];
        let (pairs, _) = text.as_bytes().as_chunks::<2>();
        for (position, pair) in pairs.iter().enumerate() {
            let high = hex_value(pair[0]).ok_or(ActionIdError::InvalidCharacter)?;
            let low = hex_value(pair[1]).ok_or(ActionIdError::InvalidCharacter)?;
            bytes[position] = (high << 4) | low;
        }
        Ok(Self(bytes))
    }

    /// The canonical lowercase hexadecimal form.
    pub fn to_hex(self) -> CompactString {
        hex(&self.0)
    }

    /// The raw digest.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl PartialEq for ActionId {
    /// Constant-time: comparing action ids must not leak how much of a guess matched.
    fn eq(&self, other: &Self) -> bool {
        constant_time_eq(&self.0, &other.0) == 1
    }
}

impl Eq for ActionId {}

impl PartialOrd for ActionId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ActionId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl fmt::Debug for ActionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ActionId({})", self.to_hex())
    }
}

impl fmt::Display for ActionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl Serialize for ActionId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for ActionId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = CompactString::deserialize(deserializer)?;
        Self::parse(&text).map_err(D::Error::custom)
    }
}

/// Where a server action was declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ServerActionKind {
    /// An export of a file-level `"use server"` module.
    ModuleExport,
    /// A closure whose body starts with `"use server"`.
    InlineClosure,
}

impl ServerActionKind {
    /// Byte mixed into the action digest so the two kinds can never collide.
    fn tag(self) -> u8 {
        match self {
            Self::ModuleExport => 0x01,
            Self::InlineClosure => 0x02,
        }
    }
}

/// Whether an action can be called from the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionExposure {
    /// Reachable from a client boundary; registered as a callable endpoint.
    CallableEndpoint,
    /// Nothing on the client can reach it, so it is never dialable.
    UnreachableFromClient,
}

impl ActionExposure {
    /// Whether the action is a callable endpoint.
    pub fn is_callable(self) -> bool {
        matches!(self, Self::CallableEndpoint)
    }
}

#[cfg(test)]
mod tests;
