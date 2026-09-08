//! The other direction: a translated file back into a locale module.
//!
//! A vendor is sent the extraction, fills in each entry's `translation`, sets
//! `locale` to the language they wrote, and sends the file back. This reads it
//! and writes the module `defineLocales` loads:
//!
//! ```js
//! const locales = defineLocales(en, {
//!   ja: () => import("./ja.js").then((module) => module.default),
//! });
//! ```
//!
//! That module is plain strings over the same keys — it does not redeclare the
//! parameters, because those belong to the message rather than to the language
//! — which is exactly what a `MessageCatalogue`'s `translation` fields are.
//!
//! # The message that moved while the file was out
//!
//! A translation round takes weeks, and the English does not stop. So the
//! project is extracted again here, now, and each returned entry is held
//! against what its message says *today*: the digest that went out with it
//! comes back beside the translation, and a message whose source or parameters
//! changed since is [`MergeReport::stale`] — reported by key, left out of the
//! module, and a failure of the command.
//!
//! Left out rather than merged, because a translation of a sentence that no
//! longer exists is not a translation of the sentence that replaced it. Where
//! `{$count}` was added, the merged text would render the placeholder to a
//! user; where a word changed, it would render a confident, fluent lie. The
//! source text is the fallback in the meantime, which is the behaviour
//! `translate` already has for a key nobody has translated yet, and the key
//! turns up in the catalogue's `untranslated` list where a test can see it.
//!
//! # What this does not check
//!
//! Whether the translation's placeholders agree with the message's parameters.
//! That needs MF2 parsed, `packages/i18n/syntax.js` is where MF2 is parsed,
//! and a second implementation of it here would be a second answer to what
//! `{$count :number}` means. `translate` performs that check against the
//! source message's parameters when the locale loads, so a translator who drops
//! a `{$count}` fails the build — see `catalogue.js`. The division is the same
//! one the whole crate is built on: Rust walks the repository, the package
//! reads the message.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;
use uf_config::UniflowedConfig;

use crate::extract::{ExtractOptions, ExtractReport, extract};
use crate::{I18nError, MessageCatalogue, ParamKind};

/// A returned translation whose message is no longer the one it translates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StaleMessage {
    /// The key.
    pub key: String,
    /// Where the message is declared now.
    pub declared_at: String,
    /// What changed, in a sentence: the source, the parameters, or both.
    pub what: String,
}

/// What a merge did, and what it declined to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeReport {
    /// The locale the file was written in.
    pub locale: String,
    /// The locale its `source` fields are in.
    pub source_locale: String,
    /// Keys merged into the module.
    pub translated: usize,
    /// Keys whose message changed since the file was extracted.
    pub stale: Vec<StaleMessage>,
    /// Keys in the file that the project no longer declares.
    ///
    /// Not a failure: a message deleted while a translation was out is an
    /// ordinary thing, and the translation of it is simply not wanted.
    pub unknown: Vec<String>,
    /// Keys the project declares that the file does not carry at all.
    ///
    /// Not a failure either: this is what a partly translated locale looks
    /// like, and the catalogue reports them as `untranslated` at run time.
    pub missing: Vec<String>,
    /// Keys the file carries with the source text still in `translation`.
    pub untranslated: Vec<String>,
    /// The module, ready to write.
    pub module: String,
    /// The extraction this was held against.
    pub extraction: ExtractReport,
}

impl MergeReport {
    /// Whether anything here should fail the command.
    ///
    /// A stale message and a broken extraction, and nothing else: the counts
    /// above are the state of a translation in progress rather than mistakes.
    #[must_use]
    pub fn has_problems(&self) -> bool {
        !self.stale.is_empty() || self.extraction.has_problems()
    }
}

/// Read a translated catalogue and build the locale module for it.
///
/// # Errors
///
/// Everything [`extract`] can fail with, plus a file that cannot be read, is
/// not a catalogue, states a format this uf does not know, or is the source
/// catalogue rather than a translation of it.
pub fn merge(
    root: &Utf8Path,
    config: &UniflowedConfig,
    file: &Utf8Path,
) -> Result<MergeReport, I18nError> {
    let text = std::fs::read_to_string(file).map_err(|source| I18nError::Io {
        action: "read",
        path: file.to_path_buf(),
        source,
    })?;
    let translated = MessageCatalogue::from_json(&text)?;

    if translated.locale == translated.source_locale {
        return Err(I18nError::NotATranslation {
            path: file.to_path_buf(),
            locale: translated.locale,
        });
    }

    // The file says which locale its `source` fields are in, so the extraction
    // needs no `--locale` and cannot disagree with the file about it.
    let extraction = extract(
        root,
        config,
        &ExtractOptions {
            locale: Some(translated.source_locale.clone()),
        },
    )?;

    Ok(compare(&translated, extraction))
}

/// Hold a returned file against a fresh extraction.
///
/// Split out so the whole comparison is testable from two values, with no
/// project on disk and no walk.
#[must_use]
pub fn compare(translated: &MessageCatalogue, extraction: ExtractReport) -> MergeReport {
    let mut report = MergeReport {
        locale: translated.locale.clone(),
        source_locale: translated.source_locale.clone(),
        translated: 0,
        stale: Vec::new(),
        unknown: Vec::new(),
        missing: Vec::new(),
        untranslated: Vec::new(),
        module: String::new(),
        extraction,
    };

    let mut merged: BTreeMap<&str, &str> = BTreeMap::new();
    for (key, entry) in &translated.messages {
        let Some(current) = report.extraction.catalogue.messages.get(key) else {
            report.unknown.push(key.clone());
            continue;
        };
        if current.digest != entry.digest {
            report.stale.push(StaleMessage {
                key: key.clone(),
                declared_at: current.declared_at.clone(),
                what: changed(&entry.source, &entry.parameters, current),
            });
            continue;
        }
        if entry.translation == entry.source {
            // Sent out and returned untouched. Not an error and not a
            // translation: leaving it out is what puts the key in the
            // catalogue's `untranslated` list rather than claiming the English
            // is the Japanese.
            report.untranslated.push(key.clone());
            continue;
        }
        merged.insert(key.as_str(), entry.translation.as_str());
    }

    for key in report.extraction.catalogue.messages.keys() {
        if !translated.messages.contains_key(key) {
            report.missing.push(key.clone());
        }
    }

    report.translated = merged.len();
    report.module = render_locale_module(&translated.locale, &merged);
    report
}

/// Which half of a message moved, as the clause a report prints.
fn changed(
    was_source: &str,
    was_parameters: &BTreeMap<String, ParamKind>,
    now: &crate::MessageEntry,
) -> String {
    let source_moved = was_source != now.source;
    let parameters_moved = *was_parameters != now.parameters;
    match (source_moved, parameters_moved) {
        (true, true) => "its text and its parameters both changed".to_owned(),
        (true, false) => "its text changed".to_owned(),
        (false, true) => format!(
            "its parameters changed, from ({}) to ({})",
            list(was_parameters),
            list(&now.parameters)
        ),
        // The digest differed and neither did: only a file whose digest was
        // edited by hand reaches this, and saying so is better than an empty
        // sentence.
        (false, false) => "its digest does not match the message it names".to_owned(),
    }
}

fn list(parameters: &BTreeMap<String, ParamKind>) -> String {
    if parameters.is_empty() {
        return "nothing".to_owned();
    }
    parameters
        .iter()
        .map(|(name, kind)| format!("{name}: {kind}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The Flow module a locale's translations live in.
///
/// A default-exported object literal with no type annotation, which is not an
/// omission: `defineLocales` wants `Partial<Translations<TMessages>>`, and an
/// object literal is checked against that at the call rather than against an
/// indexer written here, so a key this file invented is an error where the
/// locales are declared. Annotating it `{ [string]: string }` would take that
/// check away.
///
/// Every value goes through JSON string encoding, which is a subset of
/// JavaScript's: a message containing a quote, a backslash or the newlines a
/// `.match` message is full of comes out as a literal that says the same
/// thing. Keys are quoted only when they are not identifiers, which is what a
/// formatter would leave behind.
#[must_use]
pub fn render_locale_module(locale: &str, messages: &BTreeMap<&str, &str>) -> String {
    let mut out = String::new();
    out.push_str("// @flow\n//\n");
    let _ = writeln!(
        out,
        "// {locale} translations, written by `uf i18n merge`.\n\
         //\n\
         // Generated: edit the translation file and merge again rather than editing this.\n\
         // A key here is checked against the source catalogue's keys where `defineLocales`\n\
         // is called, and each message is checked against its source message's parameters by\n\
         // `translate` when the locale loads — so a placeholder lost in translation fails the\n\
         // build rather than the page.\n"
    );
    out.push_str("\nexport default {\n");
    for (key, text) in messages {
        let name = if is_identifier(key) {
            (*key).to_owned()
        } else {
            encode(key)
        };
        let _ = writeln!(out, "  {name}: {},", encode(text));
    }
    out.push_str("};\n");
    out
}

/// One JavaScript string literal holding `text`.
///
/// JSON's string grammar is a subset of JavaScript's, and `serde_json` is
/// already here to write the catalogue, so this is exact rather than a set of
/// replacements that will be missing one.
fn encode(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| {
        // `to_string` of a `&str` fails only if the writer does, and the
        // writer is a `String`. A quoted, obviously wrong value beats a panic
        // in a command that is writing a file.
        format!("{text:?}")
    })
}

/// Whether `name` may be written as a property key without quotes.
///
/// Deliberately conservative — ASCII only — because the question is not what
/// JavaScript permits but what a formatter will leave alone, and a key uf
/// quoted needlessly is a diff on the next `uf fmt`.
fn is_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    match characters.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' || first == '$' => {}
        _ => return false,
    }
    characters
        .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '$')
}

/// Write a merged locale module.
///
/// # Errors
///
/// [`I18nError::Io`], naming the path.
pub fn write_module(path: &Utf8Path, module: &str) -> Result<(), I18nError> {
    crate::write_file(path, module)
}

/// Where a merged module goes when the caller did not say.
///
/// Beside the translation file, named for the locale, because that is where
/// the loader in `defineLocales` points: `() => import("./ja.js")`.
#[must_use]
pub fn default_module_path(file: &Utf8Path, locale: &str) -> Utf8PathBuf {
    let directory = file.parent().unwrap_or(Utf8Path::new("."));
    directory.join(format!("{locale}.js"))
}
