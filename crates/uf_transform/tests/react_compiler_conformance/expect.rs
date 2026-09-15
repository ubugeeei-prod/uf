//! Reading a fixture's `.expect.md`.
//!
//! The snapshot is written by `writeOutputToString` in facebook/react's
//! `compiler/packages/snap/src/reporter.ts`: an `## Input` block, then
//! `## Code` when the plugin produced output, `## Logs` for a
//! `@loggerTestOnly` fixture, and `## Error` when the plugin threw. Each block
//! is fenced with three backticks. `### Eval output` follows and is the result
//! of *running* the code, which is not what this run measures.

/// The sections of one snapshot this run compares.
#[derive(Debug, Default)]
pub struct Expected {
    /// `## Code`: the compiled module, formatted by Prettier.
    pub code: Option<String>,
    /// `## Error`: the message the plugin threw, file prefix removed.
    pub error: Option<String>,
    /// `## Logs`: one `JSON.stringify`d logger event per line.
    pub logs: Option<String>,
}

/// Split a snapshot into its sections.
pub fn parse(snapshot: &str) -> Expected {
    // The input is fenced too, and a fixture can say anything inside it; the
    // headings that matter all come after its closing fence.
    let after_input = fenced_after(snapshot, "\n## Input\n").map_or(0, |(_, end)| end);
    let rest = &snapshot[after_input..];
    Expected {
        code: fenced_after(rest, "\n## Code\n").map(|(body, _)| body.to_owned()),
        error: fenced_after(rest, "\n## Error\n").map(|(body, _)| body.to_owned()),
        logs: fenced_after(rest, "\n## Logs\n").map(|(body, _)| body.to_owned()),
    }
}

/// The body of the first fenced block after `heading`, and the offset just
/// past its closing fence.
fn fenced_after<'a>(text: &'a str, heading: &str) -> Option<(&'a str, usize)> {
    let at = text.find(heading)? + heading.len();
    let open = at + text[at..].find("```")?;
    let body_start = open + text[open..].find('\n')? + 1;
    let close = body_start + text[body_start..].find("\n```")?;
    Some((&text[body_start..close], close + 4))
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn a_compiled_fixture_has_code_and_no_error() {
        let expected = parse(
            "\n## Input\n\n```javascript\nfunction f() {}\n\n```\n\n## Code\n\n```javascript\nfunction f() {}\n\n```\n      \n### Eval output\n(kind: ok)",
        );
        assert_eq!(expected.code.as_deref(), Some("function f() {}\n"));
        assert!(expected.error.is_none());
        assert!(expected.logs.is_none());
    }

    #[test]
    fn a_heading_inside_the_input_is_not_a_section() {
        let expected = parse(
            "\n## Input\n\n```javascript\n// ## Code\n```\n\n\n## Error\n\n```\nFound 1 error:\n```\n          \n      ",
        );
        assert!(expected.code.is_none());
        assert_eq!(expected.error.as_deref(), Some("Found 1 error:"));
    }
}
