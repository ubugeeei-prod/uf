//! Reading back the JUnit document `bun test --reporter junit` writes.
//!
//! It is the only machine-readable account `bun test` gives of a run, and `uf
//! test` needs one: Bun reports a file that registered nothing as "0 tests"
//! and exits 0, so the only way to tell a suite that ran from a suite that ran
//! nothing is to count the cases Bun says it ran against the files uf's own
//! discovery says declare some.
//!
//! # Why this is not an XML parser
//!
//! The workspace has none, and this reads one document shape from one writer:
//! `<testsuite>` elements, nested one per file and one per `describe`, holding
//! `<testcase>` elements that are empty when the case passed and hold a
//! `<failure>` or `<skipped>` when it did not. The scanner below reads tags and
//! attributes and nothing else; text content, comments and processing
//! instructions are skipped. It is bounded, because the document is a file
//! anything on the machine could have written between Bun finishing and uf
//! reading it: a size limit, a case limit, and a depth limit on the suite stack.

use std::fmt;

/// Largest document read, in bytes.
pub(crate) const MAX_JUNIT_BYTES: usize = 64 * 1024 * 1024;

/// Most cases read from one document.
pub(crate) const MAX_JUNIT_CASES: usize = 1_000_000;

/// Deepest `<testsuite>` nesting believed.
pub(crate) const MAX_SUITE_DEPTH: usize = 256;

/// How one case ended, in the terms `uf test` reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BunOutcome {
    /// The case ran and passed.
    Passed,
    /// The case ran and failed, or errored.
    Failed,
    /// The case was skipped, marked todo, or filtered out.
    Skipped,
}

/// One case Bun reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BunCase {
    /// The file the case is in, as Bun names it: relative to where Bun ran.
    pub(crate) file: String,
    /// The case's full name: its `describe` names and its own, joined with
    /// ` > `, which is how both runners name a case.
    pub(crate) name: String,
    /// How it ended.
    pub(crate) outcome: BunOutcome,
}

/// Why a document could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JunitError {
    /// Larger than [`MAX_JUNIT_BYTES`].
    TooLarge(usize),
    /// More cases than [`MAX_JUNIT_CASES`].
    TooManyCases,
    /// Suites nested deeper than [`MAX_SUITE_DEPTH`].
    TooDeep,
    /// A tag that never closes.
    Unterminated,
}

impl fmt::Display for JunitError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge(bytes) => write!(
                out,
                "the JUnit report `bun test` wrote is {bytes} bytes, past the {MAX_JUNIT_BYTES} byte limit"
            ),
            Self::TooManyCases => write!(
                out,
                "the JUnit report `bun test` wrote names more than {MAX_JUNIT_CASES} cases"
            ),
            Self::TooDeep => write!(
                out,
                "the JUnit report `bun test` wrote nests suites more than {MAX_SUITE_DEPTH} deep"
            ),
            Self::Unterminated => {
                out.write_str("the JUnit report `bun test` wrote ends in the middle of a tag")
            }
        }
    }
}

/// Every case in `document`, in document order.
pub(crate) fn read_cases(document: &str) -> Result<Vec<BunCase>, JunitError> {
    if document.len() > MAX_JUNIT_BYTES {
        return Err(JunitError::TooLarge(document.len()));
    }
    let mut cases = Vec::new();
    // The names of the open `<testsuite>` elements. The outermost one Bun
    // writes per file is named after the file, and is not part of a case's
    // name.
    let mut suites: Vec<Suite> = Vec::new();
    let mut open_case: Option<BunCase> = None;
    let mut rest = document;

    while let Some(at) = rest.find('<') {
        rest = &rest[at + 1..];
        if let Some(skipped) = skip_markup(rest)? {
            rest = skipped;
            continue;
        }
        let end = find_tag_end(rest).ok_or(JunitError::Unterminated)?;
        let tag = &rest[..end];
        rest = &rest[end + 1..];

        let closing = tag.starts_with('/');
        let body = tag.trim_start_matches('/');
        let self_closing = body.ends_with('/');
        let body = body.trim_end_matches('/');
        let (element, attributes) = match body.find(char::is_whitespace) {
            Some(split) => (&body[..split], &body[split..]),
            None => (body, ""),
        };

        match (element, closing) {
            ("testsuite", false) => {
                if suites.len() >= MAX_SUITE_DEPTH {
                    return Err(JunitError::TooDeep);
                }
                let name = attribute(attributes, "name").unwrap_or_default();
                let file = attribute(attributes, "file");
                // A suite named after its own file is the file's suite rather
                // than a `describe`.
                let is_file = file.as_deref() == Some(name.as_str());
                if !self_closing {
                    suites.push(Suite { name, is_file });
                }
            }
            ("testsuite", true) => {
                suites.pop();
            }
            ("testcase", false) => {
                if cases.len() >= MAX_JUNIT_CASES {
                    return Err(JunitError::TooManyCases);
                }
                let own = attribute(attributes, "name").unwrap_or_default();
                let file = attribute(attributes, "file")
                    .or_else(|| {
                        suites
                            .iter()
                            .find(|suite| suite.is_file)
                            .map(|suite| suite.name.clone())
                    })
                    .unwrap_or_default();
                let mut parts: Vec<&str> = suites
                    .iter()
                    .filter(|suite| !suite.is_file && !suite.name.is_empty())
                    .map(|suite| suite.name.as_str())
                    .collect();
                parts.push(own.as_str());
                let case = BunCase {
                    file,
                    name: parts.join(" > "),
                    outcome: BunOutcome::Passed,
                };
                if self_closing {
                    cases.push(case);
                } else {
                    open_case = Some(case);
                }
            }
            ("testcase", true) => {
                if let Some(case) = open_case.take() {
                    cases.push(case);
                }
            }
            ("failure" | "error", false) => {
                if let Some(case) = open_case.as_mut() {
                    case.outcome = BunOutcome::Failed;
                }
            }
            ("skipped", false) => {
                if let Some(case) = open_case.as_mut()
                    && case.outcome != BunOutcome::Failed
                {
                    case.outcome = BunOutcome::Skipped;
                }
            }
            _ => {}
        }
    }
    if let Some(case) = open_case.take() {
        cases.push(case);
    }
    Ok(cases)
}

struct Suite {
    name: String,
    is_file: bool,
}

/// Skip a declaration, comment, CDATA section or processing instruction that
/// `rest` begins with, returning what follows it.
fn skip_markup(rest: &str) -> Result<Option<&str>, JunitError> {
    let (open, close) = if rest.starts_with("!--") {
        ("!--", "-->")
    } else if rest.starts_with("![CDATA[") {
        ("![CDATA[", "]]>")
    } else if rest.starts_with('?') {
        ("?", "?>")
    } else if rest.starts_with('!') {
        ("!", ">")
    } else {
        return Ok(None);
    };
    let after = &rest[open.len()..];
    let end = after.find(close).ok_or(JunitError::Unterminated)?;
    Ok(Some(&after[end + close.len()..]))
}

/// The index of the `>` ending the tag `rest` begins inside, skipping any `>`
/// inside a quoted attribute value.
fn find_tag_end(rest: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (at, character) in rest.char_indices() {
        match (quote, character) {
            (Some(open), close) if close == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(character),
            (None, '>') => return Some(at),
            (None, _) => {}
        }
    }
    None
}

/// The value of attribute `name` in `attributes`, unescaped.
fn attribute(attributes: &str, name: &str) -> Option<String> {
    let mut rest = attributes;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return None;
        }
        let equals = rest.find('=')?;
        let key = rest[..equals].trim();
        let after = rest[equals + 1..].trim_start();
        let quote = after.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let value_and_rest = &after[1..];
        let close = value_and_rest.find(quote)?;
        let value = &value_and_rest[..close];
        if key == name {
            return Some(unescape(value));
        }
        rest = &value_and_rest[close + 1..];
    }
}

/// XML's five named entities and its numeric character references.
fn unescape(value: &str) -> String {
    if !value.contains('&') {
        return value.to_owned();
    }
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let Some(end) = after.find(';').filter(|end| *end <= 10) else {
            out.push('&');
            rest = after;
            continue;
        };
        let entity = &after[..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(character) => {
                out.push(character);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What Bun 1.3.13 wrote for a file with one passing and one failing case.
    const BUN_1_3_13: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="bun test" tests="2" assertions="2" failures="1" skipped="0" time="0.079984">
  <testsuite name="src/math.test.js" file="src/math.test.js" tests="2" assertions="2" failures="1" skipped="0" time="0" hostname="host">
    <testsuite name="math" file="src/math.test.js" line="3" tests="2" assertions="2" failures="1" skipped="0" time="0" hostname="host">
      <testcase name="adds" classname="math" time="0.000025" file="src/math.test.js" line="4" assertions="1" />
      <testcase name="fails on purpose" classname="math" time="0.000323" file="src/math.test.js" line="7" assertions="1">
        <failure type="AssertionError" />
      </testcase>
    </testsuite>
  </testsuite>
</testsuites>"#;

    #[test]
    fn reads_what_bun_writes() {
        let cases = read_cases(BUN_1_3_13).unwrap();

        assert_eq!(
            cases,
            vec![
                BunCase {
                    file: "src/math.test.js".into(),
                    name: "math > adds".into(),
                    outcome: BunOutcome::Passed,
                },
                BunCase {
                    file: "src/math.test.js".into(),
                    name: "math > fails on purpose".into(),
                    outcome: BunOutcome::Failed,
                },
            ]
        );
    }

    #[test]
    fn a_skipped_case_is_skipped_and_a_top_level_case_has_no_suite_name() {
        let document = r#"<testsuites><testsuite name="a.test.js" file="a.test.js">
          <testcase name="later" file="a.test.js"><skipped /></testcase>
          <testcase name="top" file="a.test.js" />
        </testsuite></testsuites>"#;

        let cases = read_cases(document).unwrap();

        assert_eq!(cases[0].name, "later");
        assert_eq!(cases[0].outcome, BunOutcome::Skipped);
        assert_eq!(cases[1].name, "top");
        assert_eq!(cases[1].outcome, BunOutcome::Passed);
    }

    #[test]
    fn names_are_unescaped_and_a_quoted_angle_bracket_does_not_end_a_tag() {
        let document = r#"<testsuite name="f.test.js" file="f.test.js"><testsuite name="a &amp; b" file="f.test.js"><testcase name="x &gt; y &#39;z&#x27; <ok>" file="f.test.js"/></testsuite></testsuite>"#;

        let cases = read_cases(document).unwrap();

        assert_eq!(cases[0].name, "a & b > x > y 'z' <ok>");
    }

    #[test]
    fn a_document_that_ends_inside_a_tag_is_refused() {
        assert_eq!(
            read_cases("<testsuite name=\"a\""),
            Err(JunitError::Unterminated)
        );
    }

    #[test]
    fn suites_nested_past_the_limit_are_refused() {
        let document = "<testsuite name=\"s\">".repeat(MAX_SUITE_DEPTH + 1);
        assert_eq!(read_cases(&document), Err(JunitError::TooDeep));
    }
}
