//! Which lint diagnostics uf can fix, which it fixes only when asked, and
//! which it will not fix at all.
//!
//! A fix is a promise. The editor applies the edit without asking again, often
//! on save and often to a file nobody is looking at, and `uf lint --fix`
//! applies every one of them to every file in the project at once. So the bar
//! is higher than "the message suggests something": the replacement has to be
//! the *only* right answer, and it has to still be the right answer for the
//! text the file holds now — which is not always the text the linter saw,
//! because an editor may ask for actions against a range it computed before
//! the last keystroke. Every fix below therefore re-reads the line at the
//! position the rule reported and offers nothing when it does not find what
//! the rule found. That is what makes the resulting edit apply cleanly rather
//! than land four bytes into someone's identifier.
//!
//! # Where the line between safe and unsafe is
//!
//! [`Safety::Safe`] is not "small" and not "usually right". A safe fix is one
//! where **the program after the edit means what the program before it meant**:
//! the replacement is a synonym the language already treats as the original,
//! so no reader and no runtime can tell the two apart. A safe fix needs no
//! permission because there is nothing to permit — running it changes the
//! spelling and nothing else.
//!
//! [`Safety::Unsafe`] is the edit the rule is actually asking for, spelled
//! correctly, whose *correctness* rests on something the rule could not check.
//! It is not a guess at intent — a fix uf cannot spell at all is not in this
//! catalogue in either tier — but applying it can change what the program does,
//! so a person has to ask for it by name. `--fix` never applies one;
//! `--fix-unsafe` applies both tiers.
//!
//! The two tiers live on [`Fix`] rather than in each caller, because "may this
//! edit be applied without being asked for" is a fact about the edit and every
//! caller that answered it separately would eventually answer it differently.
//!
//! # The rule that has a safe fix
//!
//! `flow/deprecated-type` — `bool` is Flow's retired spelling of `boolean` and
//! means exactly it, so the replacement carries no judgement about intent. The
//! rule refuses to fire inside a string, after a `.`, or on a property name
//! (see `uf_lint`'s `run_flow_deprecated_type`), so the four bytes it points
//! at are a type annotation and nothing else.
//!
//! # The rule that has an unsafe fix
//!
//! `flow/non-const-var-export` — `export let x = 1` becomes `export const
//! x = 1`. The edit is the whole of what the rule asks for and there is no
//! second plausible spelling of it, but it is only *correct* when nothing
//! reassigns the binding, and that is the question the rule could not answer
//! either. A file where something does reassign it still parses and still
//! formats; it throws at run time instead. That is exactly the shape of an
//! unsafe fix — a mechanical edit whose correctness depends on a fact outside
//! the line — and it is why the tier exists rather than being a label on a
//! flag.
//!
//! It is narrowed further than the rule is: it applies only to a single
//! binding with an initialiser that fits on the reported line, because
//! `export const x;` and `export const a = 1, b;` are both syntax errors and a
//! fix that produces one is not a fix. Anything wider is left to a person.
//!
//! The rule also had to learn the in-string test its siblings already had
//! before this fix could exist. uf's own code generators print Flow source
//! into template literals, and a line of one that reads `export let x = 1` is
//! a string being built rather than a binding being exported — see
//! `run_flow_non_const_var_export`. Rewriting inside it is precisely the
//! mistake `flow/unnecessary-optional-chain` is refused a fix for below.
//!
//! # The rules whose fix is the formatter
//!
//! [`FORMATTED_AWAY`] — whitespace hygiene has no targeted edit worth writing,
//! because `uf fmt` already reprints the file from its syntax tree and that is
//! uf's actual answer to "what should this whitespace be".
//!
//! # The rules that deliberately have none
//!
//! Each of these has a fix a person could write and a machine should not, in
//! either tier — not because applying it is risky but because uf cannot spell
//! it:
//!
//! - `flow/unclear-type` — `any` becomes `mixed`, an opaque type, or a
//!   generated router type depending on what the author meant. Three answers
//!   is none.
//! - `flow/ambiguous-object-type` — `{|` and `...` are opposite claims about
//!   the same object, and the rule fires precisely because the author never
//!   made one.
//! - `flow/internal-type` — `React$Node` has a public equivalent, but reaching
//!   it needs an import that may or may not already be in the file under a
//!   name uf does not know.
//! - `flow/unnecessary-optional-chain` — the edit itself is trivial (drop one
//!   `?`), but the rule matches `this?.` inside string literals too, so a fix
//!   would rewrite the contents of a string. `const s = "this?.foo";` is
//!   reported today; its sibling rules in the same runner guard the same
//!   search with an in-string test and this one does not.
//! - `server/use-client-directive-position`, `server/use-server-actions` — the
//!   directive has to land before the first *statement* but after the docblock
//!   comment that carries `@flow`, and finding that line means re-running the
//!   comment scanner that `uf_lint` owns.
//! - `security/*`, `fetch/no-global-override`, most of `react/*`,
//!   `uniflowed/no-npm-script-invocation` — these ask for a different design,
//!   not a different spelling.
//!
//! # The two rules whose fix is waiting on a shape
//!
//! `react/no-derived-state-effect` and `react/no-redundant-memo` are the first
//! rules here whose rewrite *is* mechanical and still cannot be spelled as a
//! [`Fix`]. Both edits span lines and statements — deleting an effect and a
//! `useState` and leaving one `const` behind, or unwrapping a call whose
//! argument is a multi-line arrow — and both need text taken from the file
//! rather than a `&'static str`. [`Fix`] is one line and one constant by
//! design, and widening it is a change to `--fix`, `--fix-unsafe`, `uf
//! prepare` and the editor's code actions all at once. It is the change the
//! comment on [`Fix`] anticipates; it is not this catalogue growing a row.
//!
//! # Two fixes that touch the same bytes
//!
//! [`plan`] applies the first of an overlapping pair and drops the rest, and
//! the caller lints the result and plans again. Refusing the pair would leave
//! a file the tool can see a fix for and will not make; applying both would
//! splice two replacements into one range and produce text neither rule asked
//! for. Applying one and re-running converges on the same answer either way,
//! and it is the same loop that makes `uf lint --fix` idempotent, so it costs
//! nothing that was not already being paid.

pub(crate) mod files;

use uf_lint::Diagnostic;

/// Rules whose answer is `uf fmt`, not a targeted edit.
///
/// Offering the formatter for these is only honest when the formatter actually
/// removes them, which it does not always do: `uf fmt` reprints from the
/// syntax tree and so preserves the inside of a template literal, while
/// `uniflowed/no-trailing-whitespace` reports the raw line and therefore fires
/// on trailing spaces inside one. The caller checks before offering; see
/// `commands::dev::formatter_answer`.
pub(crate) const FORMATTED_AWAY: [&str; 2] =
    ["uniflowed/no-tabs", "uniflowed/no-trailing-whitespace"];

/// Whether applying a fix can change what the program does.
///
/// See the module documentation for where the line is drawn and why it lives
/// here rather than in the commands that apply fixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Safety {
    /// The replacement is a synonym: the program means what it meant before.
    Safe,
    /// The edit is what the rule asks for, but its correctness rests on
    /// something the rule could not check, so it is applied only when asked
    /// for by name.
    Unsafe,
}

/// A replacement for a byte range on one line of the document.
///
/// One line, because every fix uf has is a word swap. A fix that spanned lines
/// would need the document rather than the line and can be added when one
/// exists; inventing the shape first would be inventing the fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fix {
    /// What the editor puts in the lightbulb menu.
    pub(crate) title: &'static str,
    /// Whether this edit may be applied without being asked for.
    pub(crate) safety: Safety,
    /// Zero-based line the edit lands on, as the protocol counts lines.
    pub(crate) line: usize,
    /// Byte offset within that line's text where the replaced text starts.
    pub(crate) start: usize,
    /// Byte offset just past the replaced text.
    pub(crate) end: usize,
    /// Text to put in its place. Empty would be a deletion; nothing needs one yet.
    pub(crate) replacement: &'static str,
}

/// The fix for `diagnostic`, given the line the document currently holds there.
///
/// [`None`] means "uf has no mechanical answer for this", which covers both a
/// rule with no fix at all and a rule whose fix no longer applies because the
/// line changed underneath it. The answer does not depend on which tiers the
/// caller wants: a caller that only applies safe fixes filters on
/// [`Fix::safety`], and a caller that wants to *say* how many unfixed findings
/// had an unsafe answer needs to be told about them.
pub(crate) fn fix_for(diagnostic: &Diagnostic, line: &str) -> Option<Fix> {
    match diagnostic.rule {
        "flow/deprecated-type" => deprecated_type(diagnostic, line),
        "flow/non-const-var-export" => mutable_export(diagnostic, line),
        _ => None,
    }
}

/// Every fix that applies to `source`, in document order, with overlaps resolved.
///
/// `allow_unsafe` admits [`Safety::Unsafe`] fixes as well as safe ones. Two
/// fixes that share a byte cannot both be applied to the same text, so the
/// first in document order wins and the rest are dropped; the caller re-lints
/// and plans again, which is the same loop that gives idempotence.
///
/// The result is safe to hand straight to [`apply`]: no two of its fixes
/// overlap, and they are sorted.
pub(crate) fn plan(source: &str, diagnostics: &[Diagnostic], allow_unsafe: bool) -> Vec<Fix> {
    let lines = line_spans(source);
    let fixes: Vec<Fix> = diagnostics
        .iter()
        .filter_map(|diagnostic| {
            let (_, text) = lines.get(diagnostic.line.checked_sub(1)?)?;
            fix_for(diagnostic, text)
        })
        .filter(|fix| allow_unsafe || fix.safety == Safety::Safe)
        .collect();
    resolve_overlaps(fixes)
}

/// `fixes` sorted into document order, with everything that overlaps something
/// already kept dropped.
///
/// Split out from [`plan`] because it is the half with a decision in it: the
/// catalogue is narrow enough today that no two of its fixes can land on the
/// same bytes of a real file, and a rule that only ever ran on inputs that
/// cannot occur is a rule nobody would notice breaking.
fn resolve_overlaps(mut fixes: Vec<Fix>) -> Vec<Fix> {
    // Document order, so "the first one wins" is a property of the file rather
    // than of the order the rules happened to run in.
    fixes.sort_by_key(|fix| (fix.line, fix.start, fix.end));

    let mut kept: Vec<Fix> = Vec::with_capacity(fixes.len());
    for fix in fixes {
        let overlaps = kept
            .last()
            .is_some_and(|last| last.line == fix.line && fix.start < last.end);
        // An empty range cannot overlap anything by the test above, so an
        // exact duplicate of a zero-width fix would be kept twice. Nothing
        // produces one, and `apply` would splice it twice if something did.
        let duplicate = kept.last() == Some(&fix);
        if !overlaps && !duplicate {
            kept.push(fix);
        }
    }
    kept
}

/// `source` with `fixes` applied.
///
/// `fixes` must be sorted and non-overlapping, which is what [`plan`] returns.
/// The text is spliced by byte offset rather than rebuilt from its lines, so a
/// file with CRLF terminators keeps them: joining `str::lines` back together
/// would rewrite every line ending in the file and call it a lint fix.
pub(crate) fn apply(source: &str, fixes: &[Fix]) -> String {
    let lines = line_spans(source);
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    for fix in fixes {
        let Some(&(offset, text)) = lines.get(fix.line) else {
            continue;
        };
        // The same guard the fixes themselves use: a range that is not on a
        // character boundary, or runs past the line, is not one to splice.
        if text.get(fix.start..fix.end).is_none() {
            continue;
        }
        let (start, end) = (offset + fix.start, offset + fix.end);
        if start < cursor {
            continue;
        }
        output.push_str(&source[cursor..start]);
        output.push_str(fix.replacement);
        cursor = end;
    }
    output.push_str(&source[cursor..]);
    output
}

/// Each line of `source` as `(byte offset of the line, the line's text)`.
///
/// The text excludes the terminator and a CRLF `\r`, so it is byte-for-byte
/// what `str::lines` yields and what a diagnostic's column counts into. The
/// offset is what turns a column back into a position in the whole file.
fn line_spans(source: &str) -> Vec<(usize, &str)> {
    let mut spans = Vec::new();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let text = line
            .strip_suffix('\n')
            .map_or(line, |line| line.strip_suffix('\r').unwrap_or(line));
        spans.push((offset, text));
        offset += line.len();
    }
    spans
}

/// `bool` → `boolean`, at the column the rule reported.
fn deprecated_type(diagnostic: &Diagnostic, line: &str) -> Option<Fix> {
    const DEPRECATED: &str = "bool";

    let start = diagnostic.column.checked_sub(1)?;
    let end = start.checked_add(DEPRECATED.len())?;
    // `get` rather than indexing: the column is a byte offset into the line the
    // *linter* read, and a stale request can point past the end of this one or
    // into the middle of a multi-byte character.
    if line.get(start..end) != Some(DEPRECATED) {
        return None;
    }
    // `boolean` and `boolish` both start with `bool`; only the standalone word
    // is the deprecated alias.
    if line.as_bytes().get(end).is_some_and(is_word_byte) {
        return None;
    }

    Some(Fix {
        title: "Replace `bool` with `boolean`",
        safety: Safety::Safe,
        line: diagnostic.line.saturating_sub(1),
        start,
        end,
        replacement: "boolean",
    })
}

/// `export let x = …` → `export const x = …`, at the keyword the rule reported.
///
/// Unsafe by the definition in the module documentation: nothing here knows
/// whether the binding is reassigned, and `const` turns a reassignment from
/// something that works into a `TypeError`. Narrowed to the one shape where
/// the edit is at least guaranteed to still be a *program*: a single plain
/// binding, with an initialiser, whose declaration finishes on this line.
/// `export const x;` and `export const a = 1, b;` do not parse, and a fix that
/// produces text the parser rejects is not a fix at any tier.
fn mutable_export(diagnostic: &Diagnostic, line: &str) -> Option<Fix> {
    let start = diagnostic.column.checked_sub(1)?;
    let keyword = ["let", "var"]
        .into_iter()
        .find(|keyword| line.get(start..start + keyword.len()) == Some(*keyword))?;
    let end = start + keyword.len();
    // `letter` and `variant` both start with a keyword; only the standalone
    // word is a declaration.
    if line.as_bytes().get(end).is_some_and(is_word_byte) {
        return None;
    }
    if !single_initialised_binding(&line[end..]) {
        return None;
    }

    Some(Fix {
        title: if keyword == "let" {
            "Replace `let` with `const`"
        } else {
            "Replace `var` with `const`"
        },
        safety: Safety::Unsafe,
        line: diagnostic.line.saturating_sub(1),
        start,
        end,
        replacement: "const",
    })
}

/// Whether what follows a `let`/`var` keyword is one plain binding with an
/// initialiser, and nothing else on the line that `const` would break.
///
/// Deliberately syntactic and deliberately narrow. It says yes to
/// `export let count: number = 0;` and no to everything it is not certain
/// about — declaration lists, destructuring, bindings with no initialiser, and
/// initialisers that run on to the next line. Saying no costs a fix somebody
/// can still apply by hand; saying yes wrongly costs them a file that no
/// longer parses.
fn single_initialised_binding(tail: &str) -> bool {
    let bytes = tail.as_bytes();
    let mut at = skip_spaces(bytes, 0);

    // The binding name: a plain identifier. `[a, b]` and `{ a }` are bindings
    // too and `const` accepts both, but they bring the comma question with
    // them and this is the shape worth being sure about.
    let name = at;
    while bytes.get(at).is_some_and(is_word_byte) {
        at += 1;
    }
    if at == name || bytes[name].is_ascii_digit() {
        return false;
    }

    // `const` without an initialiser does not parse, so its absence ends the
    // question — and a Flow annotation stands between the name and the `=`
    // often enough that skipping over one is the common case rather than the
    // exotic one.
    let Some(equals) = initialiser_start(bytes, at) else {
        return false;
    };
    single_declarator(bytes, equals + 1)
}

/// The offset of the `=` that begins the initialiser, searching from `at`.
///
/// Everything between the binding name and that `=` is a Flow type annotation,
/// so the brackets to balance include `<` and `>`: `Array<number>` and
/// `{| a: number |}` both have to be walked past without their insides being
/// read as the statement. `=>` is the one `=` in type position that begins
/// nothing, and it is skipped whole so its `>` is not counted as a closer.
///
/// [`None`] means there is no initialiser on this line, which includes the
/// declaration list `export let a, b = 1;` — the first declarator there has
/// none, and `const` needs one.
fn initialiser_start(bytes: &[u8], mut at: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    while at < bytes.len() {
        let byte = bytes[at];
        match quote {
            Some(open) => {
                if byte == b'\\' {
                    at += 1;
                } else if byte == open {
                    quote = None;
                }
            }
            None => match byte {
                b'"' | b'\'' | b'`' => quote = Some(byte),
                b'(' | b'[' | b'{' | b'<' => depth += 1,
                b')' | b']' | b'}' | b'>' => depth = depth.checked_sub(1)?,
                b',' if depth == 0 => return None,
                b'=' if depth == 0 => match bytes.get(at + 1) {
                    // `=>` opens a function type and `==` is not a thing in
                    // type position, but neither is an initialiser.
                    Some(b'=' | b'>') => at += 1,
                    _ => return Some(at),
                },
                _ => {}
            },
        }
        at += 1;
    }
    None
}

/// Whether the initialiser starting at `at` is the whole of this declaration.
///
/// A comma outside brackets and strings starts a second declarator, which
/// `const` would then need an initialiser for; a line that ends inside
/// brackets is a declaration whose rest this function cannot see. Angle
/// brackets are *not* balanced here: past the `=` a `<` is a comparison as
/// often as it is a type argument, and the only question left is where the
/// commas are.
fn single_declarator(bytes: &[u8], mut at: usize) -> bool {
    let mut depth = 0usize;
    let mut quote = None;
    while at < bytes.len() {
        let byte = bytes[at];
        match quote {
            Some(open) => {
                if byte == b'\\' {
                    at += 1;
                } else if byte == open {
                    quote = None;
                }
            }
            None => match byte {
                b'"' | b'\'' | b'`' => quote = Some(byte),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => match depth.checked_sub(1) {
                    Some(outer) => depth = outer,
                    // More closers than openers: this line is the tail of
                    // something bigger and is not a shape to reason about.
                    None => return false,
                },
                b',' if depth == 0 => return false,
                _ => {}
            },
        }
        at += 1;
    }
    depth == 0 && quote.is_none()
}

/// Whether a byte can appear inside a JavaScript identifier.
///
/// ASCII only, like the rules that produce these diagnostics: a multi-byte
/// character is not one of these bytes, so a word ending is never claimed in
/// the middle of one.
fn is_word_byte(byte: &u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')
}

/// The offset of the first byte at or after `at` that is not a space or tab.
fn skip_spaces(bytes: &[u8], mut at: usize) -> usize {
    while matches!(bytes.get(at), Some(b' ' | b'\t')) {
        at += 1;
    }
    at
}

#[cfg(test)]
mod tests;
