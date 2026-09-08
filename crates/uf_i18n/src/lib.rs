#![deny(missing_docs)]
//! What a translator is handed, and what comes back.
//!
//! `@uniflowed/i18n` declares each message beside its parameters in source:
//!
//! ```js
//! const messages = {
//!   greeting: message("Hello, {$name}!", { name: string }),
//!   unread: message(UNREAD, { count: number }),
//! };
//! ```
//!
//! That is the whole catalogue, and until this crate existed nothing turned it
//! into a file. A translation vendor was handed a `grep`, and the answer to
//! "which messages does this application have" was whatever the last person to
//! look happened to find. That is ubugeeei-prod/uf#569.
//!
//! # Why this is Rust
//!
//! Because it is a walk over every source file in a repository, on every
//! message in every module, and the guide is explicit that repeated,
//! repository-wide, CPU-bound work belongs in Rust rather than in JavaScript
//! merely because the public API is Flow. `@uniflowed/i18n` keeps the half
//! that is a *parse of one message* — `parseMessage` and `messageUsage` are
//! already the reader — and this keeps the half that is a walk over a project,
//! which is the same division the formatter and the checker are built on.
//!
//! # What it reads, and what it refuses to guess
//!
//! [`extract`] reads the *declaration*, not the message. A message's
//! parameters are `{ name: string }` written beside its source, so a
//! parameter's name and kind are in the syntax tree and need no MF2 parser
//! here — and deliberately so. A second MF2 implementation in Rust would be a
//! second answer to what `{$count :number}` means, in a repository whose first
//! answer is a tested one in `packages/i18n/syntax.js`. So this crate never
//! parses a message; it copies the source string verbatim and reads the
//! parameter object beside it.
//!
//! The consequence is a boundary worth stating: **placeholder agreement
//! between a translation and its source is not checked here.** It is checked
//! by `translate` at start-up, by the real parser, and that is where it stays.
//! [`merge`](merge::merge) checks the two things it *can* know for certain —
//! that a key still exists, and that the message a translation was made from
//! has not changed since — and says so rather than implying more.
//!
//! Anything the walk cannot read with certainty is an [`ExtractProblem`]
//! naming the file and line, never a message quietly missing from the file a
//! vendor is sent. A silently dropped message is the one failure that costs a
//! release: it is invisible in the extraction, invisible in review, and
//! visible to a user reading English in a Japanese page.
//!
//! # The format, and why JSON of MF2
//!
//! One file, in both directions: [`MessageCatalogue`] serialises to JSON whose
//! `messages` map holds each message's MF2 `source`, its `translation`, the
//! `parameters` it takes, where it is `declaredAt` and a `digest`. A fresh
//! extraction sets `translation` to `source`; a vendor edits `translation` and
//! `locale` and sends the file back; [`merge`](merge::merge) turns it into a
//! locale module the application imports.
//!
//! JSON carrying MF2, rather than XLIFF or a format of uf's own:
//!
//! * **The translatable unit is already MF2**, and it is what
//!   `translate(source, "ja-JP", …)` takes — a flat map of key to MF2 string.
//!   Round-tripping through XLIFF would mean encoding placeholders as `<ph>`
//!   elements and decoding them back, which is a second, lossy model of the
//!   thing MF2 already models. `.match` variants have no XLIFF equivalent that
//!   survives the trip at all.
//! * **A vendor accepts JSON.** Every translation management system imports
//!   it, and unlike XLIFF it is reviewable in a pull request: the diff of a
//!   translation round is readable by the person who has to approve it.
//! * **Nothing here is invented.** The keys are the application's, the text is
//!   MF2, the parameter kinds are the four `@uniflowed/i18n` declares. A
//!   format of uf's own would have to be learned by a vendor, a reviewer and a
//!   script, and would buy nothing that the type these entries describe does
//!   not already give.
//!
//! The file is sorted by key and pretty-printed for the same reason: it lands
//! in a repository, and a diff that reorders itself is a diff nobody reads.

pub mod extract;
pub mod merge;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uf_flow::ParseFailure;
use uf_project::ProjectError;

pub use extract::{ExtractOptions, ExtractProblem, ExtractReport, ProblemKind, extract};
pub use merge::{MergeReport, StaleMessage, merge};

/// The identifier every file this crate writes and reads carries.
///
/// Versioned in the file rather than assumed, because the file leaves the
/// repository: it goes to a vendor, sits in a queue for a month and comes
/// back. A reader that guessed would misread a future revision as a current
/// one, and the failure would be a wrong translation rather than an error.
pub const CATALOGUE_FORMAT: &str = "uf-i18n-catalogue/1";

/// What a message says one of its arguments is.
///
/// The four `@uniflowed/i18n` declares, spelled as the package spells them,
/// because these are copied from the identifier written in the source —
/// `{ count: number }` — and a fifth kind here would be a kind the package
/// does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamKind {
    /// `string`: formatted as text, `{$name}` or `{$name :string}`.
    String,
    /// `number`: `{$count :number}` or `{$n :integer}`.
    Number,
    /// `boolean`: selects in a `.match`, rarely printed.
    Boolean,
    /// `date`: `{$at :date}`, `:time` or `:datetime`.
    Date,
}

impl ParamKind {
    /// The identifier `@uniflowed/i18n` exports for this kind.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Date => "date",
        }
    }

    /// The kind that identifier names, or [`None`] for anything else.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "string" => Some(Self::String),
            "number" => Some(Self::Number),
            "boolean" => Some(Self::Boolean),
            "date" => Some(Self::Date),
            _ => None,
        }
    }
}

impl fmt::Display for ParamKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// One message, as it travels to a translator and back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageEntry {
    /// The MF2 source, in the source locale, exactly as written in the module.
    ///
    /// Never edited by a translator: it is the thing being translated, and it
    /// is what [`merge`](merge::merge) compares against to notice that the
    /// English moved on while the file was out.
    pub source: String,
    /// The MF2 text in the target locale.
    ///
    /// Equal to [`MessageEntry::source`] in a fresh extraction, which is what
    /// gives a translator something to edit rather than an empty field, and is
    /// how `merge` tells a translated message from one that was left alone.
    pub translation: String,
    /// The arguments the message takes, by name.
    ///
    /// A translator needs them — `{$count}` must survive into the translation,
    /// and knowing it is a number is what says whether "1" or "one" belongs in
    /// a variant — and `merge` needs them, because a message whose parameters
    /// changed is a message whose translation is no longer about the same
    /// sentence.
    pub parameters: BTreeMap<String, ParamKind>,
    /// Where the message is declared, as `path:line` relative to the project.
    ///
    /// For a translator, this is context: the same word is a verb in one
    /// screen and a noun in another, and the file it lives in is often the
    /// only clue available. Ignored by `merge`, which is why a message that
    /// moved is not a message that changed.
    pub declared_at: String,
    /// A digest of the source and the parameters together.
    ///
    /// It answers exactly one question: *is this still the message the
    /// translation was made from?* Not a checksum of the file, not a signature,
    /// and no defence against anyone editing it — a translation file is not
    /// hostile input, it is a colleague's work. See [`digest_of`].
    pub digest: String,
}

/// Every message an application declares, in one locale.
///
/// The file `uf i18n extract` writes and `uf i18n merge` reads. The same shape
/// in both directions on purpose: a vendor who receives the extraction, fills
/// in `translation` and changes `locale` has produced a file this can read,
/// with no second schema to learn and no conversion step to lose something in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageCatalogue {
    /// [`CATALOGUE_FORMAT`]. Refused when it is anything else.
    pub format: String,
    /// The locale the `source` of every entry is written in.
    pub source_locale: String,
    /// The locale the `translation` of every entry is written in.
    ///
    /// Equal to [`MessageCatalogue::source_locale`] in a fresh extraction. A
    /// vendor changes it; `merge` refuses a file where the two are still equal,
    /// because that file is the source catalogue rather than a translation of
    /// it, and merging it would write the English into `ja.js`.
    pub locale: String,
    /// The messages, by key, sorted.
    pub messages: BTreeMap<String, MessageEntry>,
}

impl MessageCatalogue {
    /// Render as the JSON that goes in the repository.
    ///
    /// Pretty-printed and newline-terminated: this file is reviewed in a pull
    /// request like any other, and a one-line JSON blob is a diff nobody can
    /// read. The keys are sorted by [`BTreeMap`], so two extractions of one
    /// tree are byte-identical whatever order discovery walked it in.
    ///
    /// # Errors
    ///
    /// When the value cannot be serialised, which for this shape means the
    /// allocator gave up.
    pub fn to_json(&self) -> Result<String, I18nError> {
        let mut text = serde_json::to_string_pretty(self).map_err(I18nError::Serialize)?;
        text.push('\n');
        Ok(text)
    }

    /// Read one back, refusing a file this revision does not understand.
    ///
    /// # Errors
    ///
    /// [`I18nError::Deserialize`] when the text is not this shape, and
    /// [`I18nError::UnknownFormat`] when it is a revision of the format that
    /// this uf does not know — which is a different problem with a different
    /// fix, and saying "missing field" for it would send a reader to edit a
    /// file that is not wrong.
    pub fn from_json(text: &str) -> Result<Self, I18nError> {
        // The discriminator is read on its own first, so that a newer revision
        // is named rather than reported as whichever field happened to move.
        #[derive(Deserialize)]
        struct JustTheFormat {
            format: String,
        }
        let stated: JustTheFormat = serde_json::from_str(text).map_err(I18nError::Deserialize)?;
        if stated.format != CATALOGUE_FORMAT {
            return Err(I18nError::UnknownFormat {
                found: stated.format,
            });
        }
        serde_json::from_str(text).map_err(I18nError::Deserialize)
    }

    /// Write it where `uf i18n extract` was told to put it.
    ///
    /// Creates the directory when it is missing, because the default is a
    /// directory a fresh project does not have yet and "no such file or
    /// directory" for a path uf chose is a poor first answer.
    ///
    /// # Errors
    ///
    /// [`I18nError::Serialize`], and [`I18nError::Io`] naming the path when the
    /// directory or the file could not be written.
    pub fn write_to(&self, path: &Utf8Path) -> Result<(), I18nError> {
        let text = self.to_json()?;
        write_file(path, &text)
    }
}

/// The digest of a message's source and parameters, as [`MessageEntry`] holds.
///
/// # What it is for
///
/// A translation is made from a particular sentence taking particular
/// arguments. When either changes, the translation is about something else,
/// and merging it back would put a stale sentence on a page with nothing to
/// say so. This is what lets `merge` notice: the digest travels out with the
/// message and comes back beside the translation, and a mismatch means the
/// source moved while the file was away.
///
/// # What it is not
///
/// Not a checksum of the file and not a signature. It is a change detector for
/// a file a colleague edits, so sixteen hex characters — sixty-four bits of
/// SHA-256 — is the length: enough that no set of messages a repository holds
/// will collide by accident, short enough that a human reads the file it is
/// printed in. Anyone editing a digest to make a stale translation merge has
/// gone out of their way, and no length here would stop them.
///
/// # Why it is built by hand rather than by hashing the JSON
///
/// Because the JSON carries `declaredAt`, and a message that moved to another
/// file is not a message that changed. Hashing the serialised entry would
/// invalidate every translation in a project the day somebody split a module.
/// The inputs are the two facts a translation actually depends on, separated
/// by a byte that cannot occur in an identifier so that `{ab: string}` and
/// `{a: string, b: …}` cannot hash alike.
#[must_use]
pub fn digest_of(source: &str, parameters: &BTreeMap<String, ParamKind>) -> String {
    const UNIT: u8 = 0x1f;
    let mut hasher = Sha256::new();
    hasher.update(source.as_bytes());
    for (name, kind) in parameters {
        hasher.update([UNIT]);
        hasher.update(name.as_bytes());
        hasher.update([UNIT]);
        hasher.update(kind.name().as_bytes());
    }
    let full = hasher.finalize();
    let mut hex = String::with_capacity(16);
    for byte in &full[..8] {
        // Into a `String`, which cannot fail to be written to.
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Write `text` to `path`, creating the directory it goes in.
///
/// Shared by the catalogue and the locale module because both are files uf
/// puts where uf chose, and both want the same sentence when that fails.
///
/// # Errors
///
/// [`I18nError::Io`], naming the path and whether it was the directory or the
/// file that would not be written.
pub fn write_file(path: &Utf8Path, text: &str) -> Result<(), I18nError> {
    if let Some(directory) = path.parent()
        && !directory.as_str().is_empty()
    {
        std::fs::create_dir_all(directory).map_err(|source| I18nError::Io {
            action: "create the directory for",
            path: path.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(path, text).map_err(|source| I18nError::Io {
        action: "write",
        path: path.to_path_buf(),
        source,
    })
}

/// Everything the two commands can fail with.
#[derive(Debug, Error)]
pub enum I18nError {
    /// Project source discovery failed.
    #[error(transparent)]
    Project(#[from] ProjectError),
    /// The Flow parser refused a source before parsing it.
    #[error(transparent)]
    Parse(#[from] ParseFailure),
    /// A file could not be read or written.
    #[error("failed to {action} {path}: {source}")]
    Io {
        /// The verb the sentence needs — `read`, `write`, or
        /// `create the directory for` — so it says which way it was going.
        action: &'static str,
        /// The path in question.
        path: Utf8PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The catalogue could not be turned into JSON.
    #[error("could not write the catalogue as JSON: {0}")]
    Serialize(#[source] serde_json::Error),
    /// The file is not a catalogue this uf can read.
    #[error("could not read the catalogue: {0}")]
    Deserialize(#[source] serde_json::Error),
    /// The file states a revision of the format uf does not know.
    #[error(
        "this file says it is {found}, and this uf reads {CATALOGUE_FORMAT}; \
         it was written by a different version of uf"
    )]
    UnknownFormat {
        /// What the file said it was.
        found: String,
    },
    /// No locale was given and the project does not state one.
    #[error(
        "no source locale: pass --locale, or call defineCatalogue with a literal tag \
         so uf can read it"
    )]
    NoSourceLocale,
    /// The project's `defineCatalogue` calls name more than one locale.
    #[error(
        "this project declares catalogues in {} different locales ({}); \
         pass --locale to say which one is the source",
        .found.len(),
        .found.join(", ")
    )]
    AmbiguousSourceLocale {
        /// Every locale a `defineCatalogue` call named, sorted.
        found: Vec<String>,
    },
    /// A translation file whose target locale is still the source locale.
    #[error(
        "{path} is the source catalogue, not a translation: its locale and sourceLocale are \
         both {locale}. A translated file has the target locale in `locale`."
    )]
    NotATranslation {
        /// The file.
        path: Utf8PathBuf,
        /// The locale it names twice.
        locale: String,
    },
    /// The thread the walk runs on could not be started, or did not finish.
    ///
    /// Separate from [`I18nError::Io`] because nothing was being read: naming
    /// a file for a thread that would not spawn sends a reader to look at a
    /// disk that is fine.
    #[error("the extraction worker {reason}")]
    Worker {
        /// What happened to it, as a phrase that finishes the sentence.
        reason: &'static str,
    },
}
