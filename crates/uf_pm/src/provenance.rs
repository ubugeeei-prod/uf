//! npm provenance: reading the attestation a package published, and refusing
//! one that is not about the package in front of us.
//!
//! # The question an integrity hash does not answer
//!
//! `uf.lock` records an integrity hash for every artefact. That is the right
//! primitive and it answers a different question: it says the bytes are the
//! bytes uf resolved, not that those bytes came from the source the package
//! claims. A maintainer account taken over publishes a new version with a
//! perfectly good integrity hash, and every hash check in the world passes.
//!
//! An attestation is the other half. npm publishes one for a package built by
//! a CI workflow with an OIDC identity: a signed in-toto statement saying which
//! repository, which workflow and which commit produced *this exact tarball*.
//!
//! # What uf checks, and what it therefore proves
//!
//! Four things, in this order, and each one is a refusal:
//!
//! 1. The envelope is an in-toto statement and carries a signature at all.
//! 2. The statement's subject is this package at this version — the purl,
//!    decoded, rather than compared as a string somebody chose the spelling of.
//! 3. The subject's SHA-512 is the tarball's SHA-512, taken from the lockfile's
//!    own `integrity`. **This is the load-bearing one.** An attestation for a
//!    different tarball, or one whose digest has been edited, fails here.
//! 4. When the bundle carries a Rekor transparency-log entry, the payload hash
//!    that entry records is the SHA-256 of the payload actually in the
//!    envelope. A statement swapped underneath a genuine, publicly logged entry
//!    fails here.
//!
//! Together those prove: *this attestation is about this artefact, and the
//! statement is the one the transparency log saw.* An attestation reused from
//! another package, an attestation whose subject digest was edited to match a
//! replaced tarball, and a statement rewritten under a real log entry are all
//! refused.
//!
//! # What it does not prove
//!
//! **uf does not verify the Sigstore signature itself.** Checking the DSSE
//! signature means an ECDSA verification against a Fulcio certificate, and the
//! certificate chain, and its OIDC identity extensions — a cryptographic stack
//! uf does not link and will not grow inside a package-manager module without
//! a decision made deliberately. Until it does, a registry that is *itself*
//! hostile can serve a self-consistent attestation naming any repository it
//! likes, and the checks above will pass. See ubugeeei-prod/uf#552 and the
//! `docs/security.md` row, which say so in the same words.
//!
//! What is closed today is the attacker who can replace an artefact but not
//! rewrite the whole attestation around it — the compromised mirror, the
//! swapped tarball, the copied attestation — and the reporting that makes an
//! *absent* attestation visible at all, which is the signal a taken-over
//! publishing account produces: a package that had provenance and stops having
//! it.
//!
//! # Absence is reported, never refused
//!
//! Most of npm has no attestation. A tool that refused them all would be a tool
//! nobody runs, and a security control nobody runs protects nothing. So a
//! package with no attestation is [`Outcome::Unattested`] and a line in the
//! summary; a package with one that does not check out is an error that stops
//! the command.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use compact_str::{CompactString, ToCompactString};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use thiserror::Error;

use crate::registry::MAX_PACKUMENT_BYTES;

/// The largest DSSE payload uf will decode.
///
/// A SLSA statement is a few kilobytes. This is the bound past which uf stops
/// rather than allocating whatever a registry chose to send; the transport is
/// already bounded by [`MAX_PACKUMENT_BYTES`], and this bounds what comes out
/// of the base64 inside it.
pub const MAX_STATEMENT_BYTES: usize = 1024 * 1024;

/// How many subjects uf will look at in one statement.
///
/// npm publishes one. A statement with thousands is a statement built to make
/// a scan expensive.
pub const MAX_SUBJECTS: usize = 64;

/// The `payloadType` a DSSE envelope must carry.
const IN_TOTO_PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";

/// The predicate types uf recognises as build provenance.
///
/// A closed table, the way every other name uf trusts is a closed table. An
/// attestation of some other kind — npm's own publish attestation, for one —
/// is not a provenance statement and is not read as one.
pub const PROVENANCE_PREDICATES: [&str; 2] = [
    "https://slsa.dev/provenance/v1",
    "https://slsa.dev/provenance/v0.2",
];

/// The artefact an attestation has to be about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Artifact<'a> {
    /// Published package name, `@scope/name` included.
    pub name: &'a str,
    /// The exact version.
    pub version: &'a str,
    /// `dist.integrity` as a lockfile records it: `sha512-<base64>`.
    pub integrity: &'a str,
}

/// Whether the statement was in a public transparency log, as far as uf can
/// tell without verifying a signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transparency {
    /// A Rekor entry was present, and it records the payload uf read.
    Logged {
        /// The log index, which is what a reader looks the entry up by.
        index: u64,
    },
    /// The bundle carried no transparency-log entry.
    Absent,
}

/// What a checked attestation says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    /// The SLSA predicate type the statement declares.
    pub predicate_type: CompactString,
    /// The repository the build claims to have run in.
    pub source_repository: Option<CompactString>,
    /// The workflow, or other build entry point, it claims to have run.
    pub build_entry: Option<CompactString>,
    /// The commit it claims to have built.
    pub source_revision: Option<CompactString>,
    /// What the transparency-log entry said, if there was one.
    pub transparency: Transparency,
}

impl Provenance {
    /// The one-line "built by" a report prints, or `None` when the statement
    /// names nothing uf recognises.
    #[must_use]
    pub fn origin(&self) -> Option<CompactString> {
        let repository = self.source_repository.as_ref()?;
        Some(match &self.build_entry {
            Some(entry) => format!("{repository} ({entry})").to_compact_string(),
            None => repository.clone(),
        })
    }
}

/// Why an attestation was refused.
///
/// Every one of these stops the command. An attestation that is present and
/// does not check out is the case this module exists for, and there is no
/// version of it that is a warning.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProvenanceError {
    /// The registry's answer was not the shape an attestation document has.
    #[error("the attestation {registry_or_file} published for {name}@{version} is not readable")]
    Malformed {
        /// Package.
        name: CompactString,
        /// Version.
        version: CompactString,
        /// Where it came from, for the message.
        registry_or_file: CompactString,
    },
    /// The DSSE envelope carried no signature.
    #[error(
        "the provenance attestation for {name}@{version} carries no signature at all, \
         so there is nothing it could be signed by"
    )]
    Unsigned {
        /// Package.
        name: CompactString,
        /// Version.
        version: CompactString,
    },
    /// The envelope's payload is not an in-toto statement.
    #[error(
        "the provenance attestation for {name}@{version} is `{payload_type}`, \
         not an in-toto statement"
    )]
    NotInToto {
        /// Package.
        name: CompactString,
        /// Version.
        version: CompactString,
        /// What it said it was, length-bounded.
        payload_type: CompactString,
    },
    /// The statement is about a different package.
    #[error(
        "the provenance attestation served for {name}@{version} is about `{subject}`.\n\n  \
         An attestation is a statement about one artefact. One that names another \
         has been copied from somewhere it was true."
    )]
    WrongSubject {
        /// Package.
        name: CompactString,
        /// Version.
        version: CompactString,
        /// The subject it actually names, length-bounded.
        subject: CompactString,
    },
    /// The statement is about a different tarball.
    ///
    /// The failure this module exists to make loud.
    #[error(
        "the provenance attestation for {name}@{version} is about a different tarball.\n\n  \
         attested  sha512:{attested}\n  \
         installed sha512:{installed}\n\n  \
         The bytes being installed are not the bytes anybody attested to. \
         Do not install this."
    )]
    DigestMismatch {
        /// Package.
        name: CompactString,
        /// Version.
        version: CompactString,
        /// The digest the statement claims, in hex, length-bounded.
        attested: CompactString,
        /// The digest of what is being installed, in hex.
        installed: CompactString,
    },
    /// The statement, or the lockfile, has no SHA-512 to compare.
    #[error(
        "the provenance attestation for {name}@{version} carries no sha512 digest, \
         so there is nothing to compare the tarball against"
    )]
    NoDigest {
        /// Package.
        name: CompactString,
        /// Version.
        version: CompactString,
    },
    /// The attestation is not a build-provenance statement.
    #[error(
        "`{predicate_type}` is not a provenance predicate uf reads; \
         the ones it does are {known}"
    )]
    UnknownPredicate {
        /// What the statement said, length-bounded.
        predicate_type: CompactString,
        /// The closed table, for the message.
        known: CompactString,
    },
    /// The transparency-log entry is about a different statement.
    #[error(
        "the transparency-log entry for {name}@{version} records a different statement.\n\n  \
         logged   sha256:{logged}\n  \
         served   sha256:{served}\n\n  \
         The statement in the bundle is not the one the public log saw."
    )]
    TransparencyMismatch {
        /// Package.
        name: CompactString,
        /// Version.
        version: CompactString,
        /// What the log entry recorded, length-bounded.
        logged: CompactString,
        /// What was actually served.
        served: CompactString,
    },
}

/// What uf found out about one package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// An attestation exists and is about this artefact.
    Attested(Box<Provenance>),
    /// The registry has no provenance attestation for it.
    ///
    /// Most of npm. Reported, never refused — see the module docs.
    Unattested,
    /// uf could not ask.
    ///
    /// A registry that is down is not evidence of anything, and a build that
    /// failed because a network was slow would train people to turn this off.
    Unavailable(CompactString),
}

impl Outcome {
    /// Whether this package's provenance was read and checked.
    #[must_use]
    pub const fn is_attested(&self) -> bool {
        matches!(self, Self::Attested(_))
    }

    /// The word a table prints in an `attested` column.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Attested(_) => "yes",
            Self::Unattested => "no",
            Self::Unavailable(_) => "unknown",
        }
    }
}

/// Read and check one package's provenance from a registry.
///
/// # Errors
///
/// [`ProvenanceError`] when an attestation exists and does not check out. Not
/// being able to reach the registry is [`Outcome::Unavailable`] rather than an
/// error: see the module docs for why the two are not the same answer.
pub fn read(registry: &str, artifact: Artifact<'_>) -> Result<Outcome, ProvenanceError> {
    let Some(url) = attestation_url(registry, artifact.name, artifact.version) else {
        return Ok(Outcome::Unavailable(
            "the package name or version is not one uf will put in a URL".to_compact_string(),
        ));
    };
    let body = match crate::registry::get(&url, "application/json") {
        Ok(body) => body,
        // A registry that answered says this package has no attestation. That
        // is a fact, and a different one from a registry that did not answer.
        Err(failure) if failure.answered() => return Ok(Outcome::Unattested),
        Err(failure) => return Ok(Outcome::Unavailable(bounded(failure.detail()))),
    };
    let Ok(document) = serde_json::from_slice::<Value>(&body) else {
        return Err(ProvenanceError::Malformed {
            name: bounded(artifact.name),
            version: bounded(artifact.version),
            registry_or_file: bounded(registry),
        });
    };
    check_document(&document, artifact, registry)
}

/// Read the provenance of a version whose bytes uf does not have in front of
/// it, using the digest the registry publishes for that version.
///
/// `uf pm approve-builds` is the caller: it is about to let a package run code
/// on this machine, and "who built this" is the question worth asking at that
/// moment — but it is reading `node_modules`, where there is no tarball and no
/// integrity hash to compare against.
///
/// So the subject is bound to `dist.integrity` from the registry rather than to
/// local bytes. **That is a weaker statement than [`read`] makes**: it says the
/// attestation is about the tarball this registry publishes for this version,
/// not about the bytes in this tree. Binding those is the install's job, and
/// the lockfile's integrity hash is what does it.
///
/// # Errors
///
/// [`ProvenanceError`] when an attestation exists and is not about the version
/// it was asked for.
pub fn read_published(
    registry: &str,
    name: &str,
    version: &str,
) -> Result<Outcome, ProvenanceError> {
    let Some(integrity) = published_integrity(registry, name, version) else {
        return Ok(Outcome::Unavailable(
            "the registry does not publish a sha512 for this version".to_compact_string(),
        ));
    };
    read(
        registry,
        Artifact {
            name,
            version,
            integrity: &integrity,
        },
    )
}

/// One package a caller wants the provenance of.
///
/// Owned rather than borrowed because a worker thread holds it, and carrying
/// the integrity here is what keeps the two callers honest about which question
/// they are asking: `Some` binds the attestation to the bytes the lockfile
/// pins, `None` binds it to the tarball the registry publishes for that version
/// — see [`read`] and [`read_published`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    /// Published package name.
    pub name: CompactString,
    /// The exact version.
    pub version: CompactString,
    /// The lockfile's `integrity`, when the caller has one.
    pub integrity: Option<CompactString>,
}

/// Read several packages' provenance at once, each routed to its own registry.
///
/// One or two requests per package, so a hundred packages read one after
/// another would be a minute of an install spent waiting. The same fixed worker
/// count the packument reads use, for the same reason: fast enough for a real
/// project, polite enough for a registry that is answering everybody else too.
///
/// Answers come back in the order they were asked for, so a caller can zip them
/// against the rows it is about to draw.
#[must_use]
pub fn read_many(
    routing: &crate::registry::RegistryRouting,
    subjects: &[Subject],
) -> Vec<Result<Outcome, ProvenanceError>> {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let answers: Mutex<std::collections::BTreeMap<usize, Result<Outcome, ProvenanceError>>> =
        Mutex::new(std::collections::BTreeMap::new());
    let next = AtomicUsize::new(0);
    let workers = crate::registry::CONCURRENCY.min(subjects.len().max(1));

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(subject) = subjects.get(index) else {
                        return;
                    };
                    let registry = routing.route(&subject.name).registry;
                    let answer = match subject.integrity.as_deref() {
                        Some(integrity) => read(
                            registry,
                            Artifact {
                                name: &subject.name,
                                version: &subject.version,
                                integrity,
                            },
                        ),
                        None => read_published(registry, &subject.name, &subject.version),
                    };
                    // `unwrap` on a mutex a scope owns: the only way it is
                    // poisoned is a panic in this closure, and there is nothing
                    // above to unwind into.
                    answers
                        .lock()
                        .expect("no other thread panicked holding this")
                        .insert(index, answer);
                }
            });
        }
    });

    answers
        .into_inner()
        .expect("the scope joined every worker")
        .into_values()
        .collect()
}

/// `dist.integrity` for one exact version, as the registry publishes it.
fn published_integrity(registry: &str, name: &str, version: &str) -> Option<CompactString> {
    if !registry.starts_with("https://")
        || registry.contains('@')
        || !crate::registry::is_safe_package_name(name)
        || !is_safe_version(version)
    {
        return None;
    }
    // The same `%2f` encoding a packument read uses, so a scope cannot become
    // a directory in the URL.
    let url = format!(
        "{}/{}/{version}",
        registry.trim_end_matches('/'),
        name.replace('/', "%2f")
    );
    let body = crate::registry::get(&url, "application/json").ok()?;
    let manifest: Value = serde_json::from_slice(&body).ok()?;
    let integrity = manifest.get("dist")?.get("integrity")?.as_str()?;
    // Checked here rather than trusted downstream: a value that is not a
    // sha512 has nothing an in-toto digest can be compared against, and
    // `sha512_hex_of` would report that as "no digest" much further away.
    sha512_hex_of(integrity).map(|_| integrity.to_compact_string())
}

/// Check a registry's whole attestation document.
///
/// Split from [`read`] so the checking is a pure function of bytes: every test
/// below hands it a document rather than standing up a registry.
///
/// # Errors
///
/// [`ProvenanceError`] when the document holds a provenance attestation that
/// does not check out.
pub fn check_document(
    document: &Value,
    artifact: Artifact<'_>,
    source: &str,
) -> Result<Outcome, ProvenanceError> {
    let Some(attestations) = document.get("attestations").and_then(Value::as_array) else {
        // An answer with no attestations array is an answer that this package
        // has none. npm serves exactly that.
        return Ok(Outcome::Unattested);
    };
    for attestation in attestations.iter().take(MAX_SUBJECTS) {
        // The index's own `predicateType` is the registry's claim about what
        // is inside; the statement's is the statement's. uf reads the
        // statement, and skips an entry the index says is something else so a
        // publish attestation is not read as a provenance one.
        let declared = attestation
            .get("predicateType")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !declared.is_empty() && !PROVENANCE_PREDICATES.contains(&declared) {
            continue;
        }
        // An entry the index calls provenance with nothing under it is not an
        // absent attestation; it is an answer uf cannot read, and the two must
        // not report the same.
        let bundle = attestation
            .get("bundle")
            .ok_or_else(|| ProvenanceError::Malformed {
                name: bounded(artifact.name),
                version: bounded(artifact.version),
                registry_or_file: bounded(source),
            })?;
        return check_bundle(bundle, artifact)
            .map(|provenance| Outcome::Attested(Box::new(provenance)));
    }
    Ok(Outcome::Unattested)
}

/// Check one Sigstore bundle against the artefact it claims to be about.
///
/// # Errors
///
/// [`ProvenanceError`], one variant per thing that can be wrong, because
/// "provenance failed" is not a sentence anybody can act on.
pub fn check_bundle(bundle: &Value, artifact: Artifact<'_>) -> Result<Provenance, ProvenanceError> {
    let name = bounded(artifact.name);
    let version = bounded(artifact.version);
    let malformed = || ProvenanceError::Malformed {
        name: name.clone(),
        version: version.clone(),
        registry_or_file: CompactString::const_new("the bundle"),
    };

    let envelope = bundle.get("dsseEnvelope").ok_or_else(malformed)?;

    // An envelope with no signature is not an attestation, whatever else it
    // says. uf does not verify the signature — see the module docs — but an
    // absent one is a shape check, and a bundle that fails it is not a bundle.
    let signed = envelope
        .get("signatures")
        .and_then(Value::as_array)
        .is_some_and(|signatures| {
            signatures.iter().any(|signature| {
                signature
                    .get("sig")
                    .and_then(Value::as_str)
                    .is_some_and(|sig| !sig.is_empty())
            })
        });
    if !signed {
        return Err(ProvenanceError::Unsigned { name, version });
    }

    let payload_type = envelope
        .get("payloadType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if payload_type != IN_TOTO_PAYLOAD_TYPE {
        return Err(ProvenanceError::NotInToto {
            name,
            version,
            payload_type: bounded(payload_type),
        });
    }

    let encoded = envelope
        .get("payload")
        .and_then(Value::as_str)
        .ok_or_else(malformed)?;
    if encoded.len() > MAX_PACKUMENT_BYTES {
        return Err(malformed());
    }
    let payload = BASE64.decode(encoded).map_err(|_| malformed())?;
    if payload.len() > MAX_STATEMENT_BYTES {
        return Err(malformed());
    }
    let statement: Value = serde_json::from_slice(&payload).map_err(|_| malformed())?;

    let predicate_type = statement
        .get("predicateType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !PROVENANCE_PREDICATES.contains(&predicate_type) {
        return Err(ProvenanceError::UnknownPredicate {
            predicate_type: bounded(predicate_type),
            known: PROVENANCE_PREDICATES.join(", ").to_compact_string(),
        });
    }

    let subjects = statement
        .get("subject")
        .and_then(Value::as_array)
        .ok_or_else(malformed)?;
    let subject = subjects
        .iter()
        .take(MAX_SUBJECTS)
        .find(|subject| {
            subject
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|named| names_the_same_package(named, artifact.name, artifact.version))
        })
        .ok_or_else(|| ProvenanceError::WrongSubject {
            name: name.clone(),
            version: version.clone(),
            subject: bounded(
                subjects
                    .first()
                    .and_then(|subject| subject.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("nothing at all"),
            ),
        })?;

    let attested = subject
        .get("digest")
        .and_then(|digest| digest.get("sha512"))
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase);
    let installed = sha512_hex_of(artifact.integrity);
    match (attested, installed) {
        (Some(attested), Some(installed)) if attested == installed => {}
        (Some(attested), Some(installed)) => {
            return Err(ProvenanceError::DigestMismatch {
                name,
                version,
                attested: bounded_digest(&attested),
                installed: bounded_digest(&installed),
            });
        }
        _ => return Err(ProvenanceError::NoDigest { name, version }),
    }

    let transparency = transparency_of(bundle, &payload, &name, &version)?;

    Ok(Provenance {
        predicate_type: bounded(predicate_type),
        source_repository: source_repository(&statement),
        build_entry: build_entry(&statement),
        source_revision: source_revision(&statement),
        transparency,
    })
}

/// Check the Rekor entry against the payload that was actually served.
///
/// The entry's `canonicalizedBody` records a SHA-256 of the DSSE payload. That
/// hash is the one thing in a bundle uf *can* check against the public log
/// without a signature: a statement swapped underneath a genuine entry no
/// longer matches the hash the log holds.
fn transparency_of(
    bundle: &Value,
    payload: &[u8],
    name: &CompactString,
    version: &CompactString,
) -> Result<Transparency, ProvenanceError> {
    let Some(entry) = bundle
        .get("verificationMaterial")
        .and_then(|material| material.get("tlogEntries"))
        .and_then(Value::as_array)
        .and_then(|entries| entries.first())
    else {
        return Ok(Transparency::Absent);
    };
    let index = entry
        .get("logIndex")
        .and_then(|value| {
            // Rekor writes it as a JSON string in some bundle versions and a
            // number in others; both are the same integer.
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .unwrap_or_default();
    let Some(logged) = entry
        .get("canonicalizedBody")
        .and_then(Value::as_str)
        .and_then(|body| BASE64.decode(body).ok())
        .filter(|body| body.len() <= MAX_STATEMENT_BYTES)
        .and_then(|body| serde_json::from_slice::<Value>(&body).ok())
        .and_then(|body| payload_hash_in(&body))
    else {
        // An entry uf cannot read the payload hash out of proves nothing, and
        // pretending otherwise would be worse than saying so.
        return Ok(Transparency::Absent);
    };
    let served = hex(&Sha256::digest(payload));
    if logged.eq_ignore_ascii_case(&served) {
        return Ok(Transparency::Logged { index });
    }
    Err(ProvenanceError::TransparencyMismatch {
        name: name.clone(),
        version: version.clone(),
        logged: bounded_digest(&logged),
        served: bounded_digest(&served),
    })
}

/// The payload hash a Rekor entry records, in either of the two shapes.
///
/// `dsse` v0.0.1 puts it at `spec.payloadHash`; `intoto` v0.0.2 nests it under
/// `spec.content`. Both are a SHA-256 of the same bytes.
fn payload_hash_in(body: &Value) -> Option<String> {
    let spec = body.get("spec")?;
    let hash = spec.get("payloadHash").or_else(|| {
        spec.get("content")
            .and_then(|content| content.get("payloadHash"))
    })?;
    if hash.get("algorithm").and_then(Value::as_str)? != "sha256" {
        return None;
    }
    Some(hash.get("value").and_then(Value::as_str)?.to_owned())
}

/// Whether a purl subject names this package at this version.
///
/// Compared after decoding rather than as a string: npm writes
/// `pkg:npm/%40scope/name@1.0.0`, the purl specification allows
/// `pkg:npm/%40scope%2fname@1.0.0`, and a check that only accepted one spelling
/// would refuse honest attestations while a check that compared substrings
/// would accept `@evil/react` for `react`.
fn names_the_same_package(subject: &str, name: &str, version: &str) -> bool {
    let Some(rest) = subject.strip_prefix("pkg:npm/") else {
        return false;
    };
    // The version is after the last `@`, because the name may start with one.
    let Some(at) = rest.rfind('@') else {
        return false;
    };
    let (named, attested_version) = rest.split_at(at);
    let attested_version = &attested_version[1..];
    // A purl may carry qualifiers or a subpath after the version; neither
    // changes which artefact it is about, and neither is trusted here.
    let attested_version = attested_version
        .split_once(['?', '#'])
        .map_or(attested_version, |(head, _)| head);
    percent_decode(named) == name && percent_decode(attested_version) == version
}

/// Decode `%XX` escapes, and nothing else.
///
/// A twenty-line decoder rather than a URL crate: the input is one path
/// segment of a purl, the only escapes npm and the purl specification produce
/// here are `%40` and `%2F`, and an invalid escape is left as written so a
/// malformed subject fails the comparison rather than becoming a different one.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (
                char::from(bytes[index + 1]).to_digit(16),
                char::from(bytes[index + 2]).to_digit(16),
            )
        {
            let byte = u8::try_from(high * 16 + low).unwrap_or(b'%');
            out.push(char::from(byte));
            index += 3;
            continue;
        }
        // Pushing by byte is safe here because a non-`%` byte of a UTF-8
        // string is only re-assembled from its own bytes in order.
        out.push_str(&text[index..=index]);
        index += 1;
    }
    out
}

/// `sha512-<base64>` as lower-case hex, which is the form an in-toto digest is
/// written in.
///
/// npm records the tarball digest in base64 with a prefix, and in-toto records
/// it in hex. They are the same 64 bytes and this is the only place that has to
/// know it.
fn sha512_hex_of(integrity: &str) -> Option<String> {
    // A lockfile's `integrity` may carry more than one algorithm, space
    // separated. SHA-1 is not one uf compares against — a digest that can be
    // forged is a check in name only, the same rule `uf_env::archive` states.
    let digest = integrity
        .split_whitespace()
        .find_map(|entry| entry.strip_prefix("sha512-"))?;
    let bytes = BASE64.decode(digest).ok()?;
    (bytes.len() == 64).then(|| hex(&bytes))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Writing into a `String` cannot fail.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// The repository a SLSA statement claims, in either predicate version.
fn source_repository(statement: &Value) -> Option<CompactString> {
    let predicate = statement.get("predicate")?;
    let v1 = predicate
        .get("buildDefinition")
        .and_then(|definition| definition.get("externalParameters"))
        .and_then(|parameters| parameters.get("workflow"))
        .and_then(|workflow| workflow.get("repository"))
        .and_then(Value::as_str);
    let v02 = predicate
        .get("invocation")
        .and_then(|invocation| invocation.get("configSource"))
        .and_then(|source| source.get("uri"))
        .and_then(Value::as_str);
    v1.or(v02).map(bounded)
}

/// The workflow, or other build entry point, the statement claims.
fn build_entry(statement: &Value) -> Option<CompactString> {
    let predicate = statement.get("predicate")?;
    let v1 = predicate
        .get("buildDefinition")
        .and_then(|definition| definition.get("externalParameters"))
        .and_then(|parameters| parameters.get("workflow"))
        .and_then(|workflow| workflow.get("path"))
        .and_then(Value::as_str);
    let v02 = predicate
        .get("invocation")
        .and_then(|invocation| invocation.get("configSource"))
        .and_then(|source| source.get("entryPoint"))
        .and_then(Value::as_str);
    v1.or(v02).map(bounded)
}

/// The commit the statement claims to have built.
fn source_revision(statement: &Value) -> Option<CompactString> {
    let predicate = statement.get("predicate")?;
    let v1 = predicate
        .get("buildDefinition")
        .and_then(|definition| definition.get("resolvedDependencies"))
        .and_then(Value::as_array)
        .and_then(|dependencies| dependencies.first())
        .and_then(|dependency| dependency.get("digest"))
        .and_then(|digest| digest.get("gitCommit"))
        .and_then(Value::as_str);
    let v02 = predicate
        .get("invocation")
        .and_then(|invocation| invocation.get("configSource"))
        .and_then(|source| source.get("digest"))
        .and_then(|digest| digest.get("sha1"))
        .and_then(Value::as_str);
    v1.or(v02).map(bounded)
}

/// npm's attestation endpoint for one exact version.
///
/// `None` when the name or the version is not something uf will put in a URL —
/// the same closed grammar the packument reads use, plus a version alphabet,
/// because both halves are project text and neither gets to choose a path.
fn attestation_url(registry: &str, name: &str, version: &str) -> Option<String> {
    if !registry.starts_with("https://") || registry.contains('@') {
        return None;
    }
    if !crate::registry::is_safe_package_name(name) || !is_safe_version(version) {
        return None;
    }
    Some(format!(
        "{}/-/npm/v1/attestations/{name}@{version}",
        registry.trim_end_matches('/')
    ))
}

/// A semantic version's alphabet, and nothing else.
fn is_safe_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 256
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+' | b'_'))
}

/// Keep untrusted text out of error messages beyond a fixed budget.
fn bounded(value: &str) -> CompactString {
    crate::registry::bounded_text(value)
}

/// The same, for a digest, which is a different shape of untrusted text.
///
/// A registry's digest is still registry text — it does not get to put control
/// characters in a terminal — but the general budget would cut a SHA-512 in
/// half, and half a digest is useless to a reader who wants to look the other
/// one up. So the alphabet is the bound instead: hex characters, up to the
/// length a SHA-512 actually is.
fn bounded_digest(value: &str) -> CompactString {
    value
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(128)
        .collect()
}

#[cfg(test)]
mod tests;
