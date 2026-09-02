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

use camino::Utf8PathBuf;
use compact_str::CompactString;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::directive::ModuleEnvironment;
use crate::graph::RscGraph;
use crate::scan::ExportKind;

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

/// One server action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerAction {
    /// Keyed identifier the client uses to call it.
    pub id: ActionId,
    /// Declaring module, relative to the project root.
    pub module: Utf8PathBuf,
    /// Export name, or the binding of an inline closure.
    pub export: CompactString,
    /// Where the action was declared.
    pub kind: ServerActionKind,
    /// Whether the client can reach it.
    pub exposure: ActionExposure,
}

/// Every server action of a build, and the lookup the runtime dials into.
#[derive(Debug, Clone)]
pub struct ServerActionRegistry {
    actions: Vec<ServerAction>,
    fingerprint: CompactString,
}

impl ServerActionRegistry {
    /// Collect the actions of a resolved graph.
    pub fn from_graph(graph: &RscGraph, build_id: &BuildId) -> Self {
        let modules = graph.modules();
        let hands_off = hands_off_to_client(graph);

        let mut actions: Vec<ServerAction> = Vec::new();
        for (position, module) in modules.iter().enumerate() {
            let exposure = if hands_off[position] {
                ActionExposure::CallableEndpoint
            } else {
                ActionExposure::UnreachableFromClient
            };

            if module.environment == ModuleEnvironment::ServerActions {
                for export in &module.exports {
                    // Only shapes React can actually invoke become endpoints;
                    // anything else is reported by the graph and left out.
                    if !matches!(
                        export.kind,
                        ExportKind::AsyncFunction | ExportKind::ReExport
                    ) {
                        continue;
                    }
                    actions.push(ServerAction {
                        id: ActionId::derive(
                            build_id,
                            module.path.as_str(),
                            &export.name,
                            ServerActionKind::ModuleExport,
                        ),
                        module: module.path.clone(),
                        export: export.name.clone(),
                        kind: ServerActionKind::ModuleExport,
                        exposure,
                    });
                }
            }

            for directive in &module.function_actions {
                let export = CompactString::from(directive.owner.to_string());
                actions.push(ServerAction {
                    id: ActionId::derive(
                        build_id,
                        module.path.as_str(),
                        &export,
                        ServerActionKind::InlineClosure,
                    ),
                    module: module.path.clone(),
                    export,
                    kind: ServerActionKind::InlineClosure,
                    exposure,
                });
            }
        }

        actions.sort_by(|left, right| {
            left.module
                .cmp(&right.module)
                .then(left.kind.cmp(&right.kind))
                .then(left.export.cmp(&right.export))
        });
        actions.dedup_by(|left, right| {
            left.module == right.module && left.kind == right.kind && left.export == right.export
        });

        Self {
            actions,
            fingerprint: build_id.fingerprint(),
        }
    }

    /// Every action, callable or not, ordered by module and name.
    pub fn actions(&self) -> &[ServerAction] {
        &self.actions
    }

    /// Only the actions that are callable endpoints.
    pub fn callable_actions(&self) -> impl Iterator<Item = &ServerAction> {
        self.actions
            .iter()
            .filter(|action| action.exposure.is_callable())
    }

    /// Number of registered actions.
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// A publishable digest of the build id.
    pub fn build_fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Resolve an id received from a client.
    ///
    /// Malformed, forged, unknown and unreachable ids are all
    /// [`UnknownAction`]: the caller learns nothing beyond "no".
    pub fn resolve(&self, raw_id: &str) -> Result<&ServerAction, UnknownAction> {
        let id = ActionId::parse(raw_id).map_err(|_| UnknownAction)?;
        self.lookup(&id)
    }

    /// Resolve a parsed id.
    ///
    /// The scan visits every entry whatever happens, and compares with
    /// [`constant_time_eq`], so neither the running time nor the branch pattern
    /// depends on which entry matched — or on whether one did.
    pub fn lookup(&self, id: &ActionId) -> Result<&ServerAction, UnknownAction> {
        let mut selected = 0u32;
        let mut found = 0u8;

        for (position, action) in self.actions.iter().enumerate() {
            let matches = constant_time_eq(&action.id.0, &id.0)
                & u8::from(action.exposure == ActionExposure::CallableEndpoint);
            let mask = 0u32.wrapping_sub(u32::from(matches));
            selected = (selected & !mask) | ((position as u32) & mask);
            found |= matches;
        }

        if found == 1 {
            self.actions.get(selected as usize).ok_or(UnknownAction)
        } else {
            Err(UnknownAction)
        }
    }
}

/// Which modules can hand a server action across a client boundary.
///
/// A module qualifies when the client graph already contains it, or when the
/// server renders it *and* it reaches a `"use client"` import — that is the only
/// way a closure defined there can end up as a prop on a Client Component. Every
/// `"use server"` module such a module imports becomes callable with it.
fn hands_off_to_client(graph: &RscGraph) -> Vec<bool> {
    let modules = graph.modules();
    let mut hands_off = vec![false; modules.len()];

    for (position, module) in modules.iter().enumerate() {
        hands_off[position] = module.reachability.is_client_reachable()
            || (module.reachability.is_server_reachable() && module.proximity.reaches_boundary());
    }

    for (position, module) in modules.iter().enumerate() {
        if !hands_off[position] {
            continue;
        }
        for target in module.imports.iter().copied() {
            if modules[target.index()].environment == ModuleEnvironment::ServerActions {
                hands_off[target.index()] = true;
            }
        }
    }

    hands_off
}

/// HMAC-SHA256, RFC 2104.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK_LEN: usize = 64;

    let mut block = [0u8; BLOCK_LEN];
    if key.len() > BLOCK_LEN {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }

    let mut inner_pad = [0x36u8; BLOCK_LEN];
    let mut outer_pad = [0x5cu8; BLOCK_LEN];
    for ((inner, outer), key_byte) in inner_pad
        .iter_mut()
        .zip(outer_pad.iter_mut())
        .zip(block.iter())
    {
        *inner ^= key_byte;
        *outer ^= key_byte;
    }

    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner);
    outer.finalize().into()
}

/// Returns 1 when the two digests are equal, without an early exit.
fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> u8 {
    let mut difference = 0u8;
    for (left_byte, right_byte) in left.iter().zip(right.iter()) {
        difference |= left_byte ^ right_byte;
    }
    (((difference as u32).wrapping_sub(1)) >> 31) as u8
}

fn hex(bytes: &[u8]) -> CompactString {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(DIGITS[(byte >> 4) as usize] as char);
        text.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    CompactString::from(text)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn os_entropy(buffer: &mut [u8]) -> bool {
    use std::io::Read;

    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(buffer))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::directive::FunctionOwner;
    use crate::graph::{EntryKind, RscGraphBuilder, RscModuleInput};

    fn build_id() -> BuildId {
        BuildId::new("build-id-for-tests").expect("valid build id")
    }

    fn graph_with_reachable_action() -> RscGraph {
        let mut builder = RscGraphBuilder::new();
        builder.add_source(
            "app/page.js",
            "import Counter from \"./Counter.js\";\nimport { refresh } from \"../server/actions.js\";\n",
        );
        builder.add_source("app/Counter.js", "\"use client\";\n");
        builder.add_source(
            "server/actions.js",
            "\"use server\";\nexport async function refresh() {}\n",
        );
        builder.add_entry("app/page.js", EntryKind::Server);
        builder.build()
    }

    #[test]
    fn hmac_matches_rfc_4231_test_case_1() {
        let mac = hmac_sha256(&[0x0b; 20], b"Hi There");
        assert_eq!(
            hex(&mac),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn hmac_matches_rfc_4231_test_case_2() {
        let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn hmac_matches_rfc_4231_test_case_6_with_an_oversized_key() {
        let mac = hmac_sha256(
            &[0xaa; 131],
            b"Test Using Larger Than Block-Size Key - Hash Key First",
        );
        assert_eq!(
            hex(&mac),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn a_short_build_id_is_rejected() {
        assert_eq!(
            BuildId::new("short"),
            Err(BuildIdError::TooShort { len: 5 })
        );
    }

    #[test]
    fn an_empty_build_id_is_rejected() {
        assert_eq!(BuildId::new(""), Err(BuildIdError::TooShort { len: 0 }));
    }

    #[test]
    fn an_oversized_build_id_is_rejected() {
        let value = "x".repeat(MAX_BUILD_ID_BYTES + 1);
        assert_eq!(
            BuildId::new(value),
            Err(BuildIdError::TooLong {
                len: MAX_BUILD_ID_BYTES + 1
            })
        );
    }

    #[test]
    fn a_generated_build_id_is_long_and_unique() {
        let first = BuildId::generate();
        let second = BuildId::generate();
        assert!(first.value.len() >= MIN_BUILD_ID_BYTES);
        assert_ne!(first, second);
    }

    #[test]
    fn a_build_id_never_prints_itself() {
        let formatted = format!("{:?}", build_id());
        assert_eq!(formatted, "BuildId(<redacted>)");
        assert!(!formatted.contains("build-id-for-tests"));
    }

    #[test]
    fn action_ids_are_stable_across_runs() {
        let first = ActionId::derive(
            &build_id(),
            "server/actions.js",
            "refresh",
            ServerActionKind::ModuleExport,
        );
        let second = ActionId::derive(
            &build_id(),
            "server/actions.js",
            "refresh",
            ServerActionKind::ModuleExport,
        );
        assert_eq!(first, second);
    }

    #[test]
    fn action_ids_change_with_the_build_id() {
        let first = ActionId::derive(
            &BuildId::new("build-id-one").unwrap(),
            "server/actions.js",
            "refresh",
            ServerActionKind::ModuleExport,
        );
        let second = ActionId::derive(
            &BuildId::new("build-id-two").unwrap(),
            "server/actions.js",
            "refresh",
            ServerActionKind::ModuleExport,
        );
        assert_ne!(first, second);
    }

    #[test]
    fn action_ids_change_with_the_module_path() {
        let first = ActionId::derive(
            &build_id(),
            "server/a.js",
            "refresh",
            ServerActionKind::ModuleExport,
        );
        let second = ActionId::derive(
            &build_id(),
            "server/b.js",
            "refresh",
            ServerActionKind::ModuleExport,
        );
        assert_ne!(first, second);
    }

    #[test]
    fn action_ids_change_with_the_export_name() {
        let first = ActionId::derive(
            &build_id(),
            "server/a.js",
            "refresh",
            ServerActionKind::ModuleExport,
        );
        let second = ActionId::derive(
            &build_id(),
            "server/a.js",
            "reload",
            ServerActionKind::ModuleExport,
        );
        assert_ne!(first, second);
    }

    #[test]
    fn action_ids_change_with_the_declaration_kind() {
        let first = ActionId::derive(
            &build_id(),
            "server/a.js",
            "refresh",
            ServerActionKind::ModuleExport,
        );
        let second = ActionId::derive(
            &build_id(),
            "server/a.js",
            "refresh",
            ServerActionKind::InlineClosure,
        );
        assert_ne!(first, second);
    }

    #[test]
    fn length_prefixing_stops_field_boundaries_from_sliding() {
        let first = ActionId::derive(&build_id(), "ab", "cd", ServerActionKind::ModuleExport);
        let second = ActionId::derive(&build_id(), "a", "bcd", ServerActionKind::ModuleExport);
        assert_ne!(first, second);
    }

    #[test]
    fn an_action_id_never_contains_the_module_path() {
        let id = ActionId::derive(
            &build_id(),
            "server/secret-actions.js",
            "refresh",
            ServerActionKind::ModuleExport,
        );
        assert!(!id.to_hex().contains("secret"));
        assert_eq!(id.to_hex().len(), ACTION_ID_HEX_LEN);
    }

    #[test]
    fn action_ids_round_trip_through_hex() {
        let id = ActionId::derive(
            &build_id(),
            "server/actions.js",
            "refresh",
            ServerActionKind::ModuleExport,
        );
        assert_eq!(ActionId::parse(&id.to_hex()).unwrap(), id);
    }

    #[test]
    fn a_truncated_action_id_is_rejected() {
        let id = ActionId::derive(
            &build_id(),
            "server/actions.js",
            "refresh",
            ServerActionKind::ModuleExport,
        );
        let hex = id.to_hex();
        assert_eq!(
            ActionId::parse(&hex[..hex.len() - 1]),
            Err(ActionIdError::InvalidLength { len: 63 })
        );
    }

    #[test]
    fn an_oversized_action_id_is_rejected_before_any_allocation() {
        let oversized = "a".repeat(1_000_000);
        assert_eq!(
            ActionId::parse(&oversized),
            Err(ActionIdError::InvalidLength { len: 1_000_000 })
        );
    }

    #[test]
    fn an_empty_action_id_is_rejected() {
        assert_eq!(
            ActionId::parse(""),
            Err(ActionIdError::InvalidLength { len: 0 })
        );
    }

    #[test]
    fn a_non_hex_action_id_is_rejected() {
        let text = "z".repeat(ACTION_ID_HEX_LEN);
        assert_eq!(ActionId::parse(&text), Err(ActionIdError::InvalidCharacter));
    }

    #[test]
    fn an_uppercase_action_id_is_rejected() {
        let id = ActionId::derive(
            &build_id(),
            "server/actions.js",
            "refresh",
            ServerActionKind::ModuleExport,
        );
        let upper = id.to_hex().to_uppercase();
        assert!(ActionId::parse(&upper).is_err());
    }

    #[test]
    fn constant_time_comparison_agrees_with_equality() {
        let left = [7u8; 32];
        let mut right = [7u8; 32];
        assert_eq!(constant_time_eq(&left, &right), 1);
        right[31] = 8;
        assert_eq!(constant_time_eq(&left, &right), 0);
        right[31] = 7;
        right[0] = 8;
        assert_eq!(constant_time_eq(&left, &right), 0);
    }

    #[test]
    fn a_reachable_module_export_is_registered_as_an_endpoint() {
        let graph = graph_with_reachable_action();
        let registry = ServerActionRegistry::from_graph(&graph, &build_id());
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.actions()[0].export, "refresh");
        assert_eq!(
            registry.actions()[0].exposure,
            ActionExposure::CallableEndpoint
        );
        assert_eq!(registry.callable_actions().count(), 1);
    }

    #[test]
    fn a_registered_endpoint_resolves_by_its_id() {
        let graph = graph_with_reachable_action();
        let registry = ServerActionRegistry::from_graph(&graph, &build_id());
        let id = registry.actions()[0].id.to_hex();
        assert_eq!(registry.resolve(&id).unwrap().export, "refresh");
    }

    #[test]
    fn an_unreachable_action_is_never_resolvable() {
        let mut builder = RscGraphBuilder::new();
        builder.add_source(
            "server/orphan.js",
            "\"use server\";\nexport async function drop() {}\n",
        );
        let graph = builder.build();
        let registry = ServerActionRegistry::from_graph(&graph, &build_id());

        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry.actions()[0].exposure,
            ActionExposure::UnreachableFromClient
        );
        assert_eq!(registry.callable_actions().count(), 0);

        let id = registry.actions()[0].id.to_hex();
        assert_eq!(registry.resolve(&id), Err(UnknownAction));
    }

    #[test]
    fn an_action_reached_only_from_a_server_module_without_a_boundary_is_not_callable() {
        let mut builder = RscGraphBuilder::new();
        builder.add_source("app/page.js", "import \"../server/actions.js\";\n");
        builder.add_source(
            "server/actions.js",
            "\"use server\";\nexport async function refresh() {}\n",
        );
        builder.add_entry("app/page.js", EntryKind::Server);
        let registry = ServerActionRegistry::from_graph(&builder.build(), &build_id());
        assert_eq!(
            registry.actions()[0].exposure,
            ActionExposure::UnreachableFromClient
        );
    }

    #[test]
    fn an_action_imported_by_a_client_module_is_callable() {
        let mut builder = RscGraphBuilder::new();
        builder.add_source("app/page.js", "import Form from \"./Form.js\";\n");
        builder.add_source(
            "app/Form.js",
            "\"use client\";\nimport { save } from \"../server/actions.js\";\n",
        );
        builder.add_source(
            "server/actions.js",
            "\"use server\";\nexport async function save() {}\n",
        );
        builder.add_entry("app/page.js", EntryKind::Server);
        let registry = ServerActionRegistry::from_graph(&builder.build(), &build_id());
        assert_eq!(
            registry.actions()[0].exposure,
            ActionExposure::CallableEndpoint
        );
    }

    #[test]
    fn an_inline_closure_action_is_registered_separately() {
        let mut builder = RscGraphBuilder::new();
        builder.add_source(
            "app/page.js",
            "import Form from \"./Form.js\";\nexport default function Page() {\n const save = async () => {\n  \"use server\";\n };\n return save;\n}\n",
        );
        builder.add_source("app/Form.js", "\"use client\";\n");
        builder.add_entry("app/page.js", EntryKind::Server);
        let registry = ServerActionRegistry::from_graph(&builder.build(), &build_id());

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.actions()[0].kind, ServerActionKind::InlineClosure);
        assert_eq!(registry.actions()[0].export, "save");
        assert_eq!(
            registry.actions()[0].exposure,
            ActionExposure::CallableEndpoint
        );
    }

    #[test]
    fn a_sync_export_of_a_server_actions_module_is_not_registered() {
        let mut builder = RscGraphBuilder::new();
        builder.add_source(
            "server/actions.js",
            "\"use server\";\nexport function refresh() {}\n",
        );
        let registry = ServerActionRegistry::from_graph(&builder.build(), &build_id());
        assert!(registry.is_empty());
    }

    #[test]
    fn a_non_function_export_of_a_server_actions_module_is_not_registered() {
        let mut builder = RscGraphBuilder::new();
        builder.add_source(
            "server/actions.js",
            "\"use server\";\nexport const n = 1;\n",
        );
        assert!(ServerActionRegistry::from_graph(&builder.build(), &build_id()).is_empty());
    }

    #[test]
    fn a_forged_id_is_rejected_with_the_same_error_as_an_unknown_one() {
        let graph = graph_with_reachable_action();
        let registry = ServerActionRegistry::from_graph(&graph, &build_id());

        let forged = "0".repeat(ACTION_ID_HEX_LEN);
        let malformed = "not-an-id";
        assert_eq!(registry.resolve(&forged), Err(UnknownAction));
        assert_eq!(registry.resolve(malformed), Err(UnknownAction));
        assert_eq!(
            registry.resolve(&forged).unwrap_err().to_string(),
            registry.resolve(malformed).unwrap_err().to_string()
        );
    }

    #[test]
    fn an_id_from_another_build_does_not_resolve() {
        let graph = graph_with_reachable_action();
        let current = ServerActionRegistry::from_graph(&graph, &build_id());
        let previous =
            ServerActionRegistry::from_graph(&graph, &BuildId::new("previous-build-id").unwrap());
        let stale = previous.actions()[0].id.to_hex();
        assert_eq!(current.resolve(&stale), Err(UnknownAction));
    }

    #[test]
    fn the_unknown_action_error_says_nothing_about_the_registry() {
        let message = UnknownAction.to_string();
        assert_eq!(message, "unknown server action");
    }

    #[test]
    fn ids_are_unique_across_a_large_registry() {
        let mut builder = RscGraphBuilder::new();
        builder.add_source("app/page.js", "import \"./Counter.js\";\n");
        builder.add_source("app/Counter.js", "\"use client\";\n");
        for index in 0..2_000 {
            builder.add_module(
                RscModuleInput::new(
                    format!("server/actions{index}.js"),
                    ModuleEnvironment::ServerActions,
                )
                .with_export("refresh", ExportKind::AsyncFunction)
                .with_export("reload", ExportKind::AsyncFunction),
            );
        }
        let registry = ServerActionRegistry::from_graph(&builder.build(), &build_id());
        assert_eq!(registry.len(), 4_000);

        let mut ids: Vec<_> = registry
            .actions()
            .iter()
            .map(|action| action.id.to_hex())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 4_000);
    }

    #[test]
    fn duplicate_declarations_collapse_to_one_action() {
        let mut builder = RscGraphBuilder::new();
        builder.add_module(
            RscModuleInput::new("server/actions.js", ModuleEnvironment::ServerActions)
                .with_export("refresh", ExportKind::AsyncFunction)
                .with_export("refresh", ExportKind::AsyncFunction),
        );
        let registry = ServerActionRegistry::from_graph(&builder.build(), &build_id());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn the_build_fingerprint_is_derived_and_not_the_build_id() {
        let registry =
            ServerActionRegistry::from_graph(&graph_with_reachable_action(), &build_id());
        assert_eq!(registry.build_fingerprint().len(), ACTION_ID_HEX_LEN);
        assert!(!registry.build_fingerprint().contains("build-id-for-tests"));
        assert_eq!(registry.build_fingerprint(), build_id().fingerprint());
    }

    #[test]
    fn registries_built_twice_from_the_same_input_are_identical() {
        let graph = graph_with_reachable_action();
        let first = ServerActionRegistry::from_graph(&graph, &build_id());
        let second = ServerActionRegistry::from_graph(&graph, &build_id());
        assert_eq!(first.actions(), second.actions());
    }

    #[test]
    fn inline_closures_in_the_same_module_get_distinct_ids() {
        let mut builder = RscGraphBuilder::new();
        builder.add_module(
            RscModuleInput::new("app/page.js", ModuleEnvironment::Server)
                .with_function_action(FunctionOwner::Anonymous { ordinal: 0 })
                .with_function_action(FunctionOwner::Anonymous { ordinal: 1 }),
        );
        let registry = ServerActionRegistry::from_graph(&builder.build(), &build_id());
        assert_eq!(registry.len(), 2);
        assert_ne!(registry.actions()[0].id, registry.actions()[1].id);
    }
}
