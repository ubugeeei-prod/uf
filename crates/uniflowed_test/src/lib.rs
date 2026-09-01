use thiserror::Error;
use uniflowed_infra::LineIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestKind {
    Describe,
    Test,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCase {
    pub file: String,
    pub name: String,
    pub kind: TestKind,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestPlan {
    pub cases: Vec<TestCase>,
}

impl TestPlan {
    pub fn runnable_count(&self) -> usize {
        self.cases
            .iter()
            .filter(|case| case.kind == TestKind::Test)
            .count()
    }
}

#[derive(Debug, Error)]
pub enum TestError {
    #[error("native JavaScript execution backend is not enabled yet")]
    RuntimeUnavailable,
}

pub fn discover_tests(file: &str, source: &str) -> TestPlan {
    let line_index = LineIndex::new(source);
    let code_mask = code_byte_mask(source);
    let mut cases = Vec::new();

    for (call, kind) in [
        ("describe", TestKind::Describe),
        ("it", TestKind::Test),
        ("test", TestKind::Test),
    ] {
        let mut search_start = 0;
        while let Some(relative) = source[search_start..].find(call) {
            let offset = search_start + relative;
            search_start = offset + call.len();

            if !code_mask.get(offset).copied().unwrap_or(false) || !is_call_at(source, offset, call)
            {
                continue;
            }

            let Some(name) = extract_first_string_arg(&source[search_start..]) else {
                continue;
            };
            let position = line_index.line_col(offset);
            cases.push(TestCase {
                file: file.to_string(),
                name,
                kind,
                line: position.line,
                column: position.column,
            });
        }
    }

    cases.sort_by(|a, b| a.line.cmp(&b.line).then(a.column.cmp(&b.column)));
    TestPlan { cases }
}

fn code_byte_mask(source: &str) -> Vec<bool> {
    let bytes = source.as_bytes();
    let mut mask = vec![false; bytes.len() + 1];
    let mut i = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;

    while i < bytes.len() {
        let byte = bytes[i];

        if line_comment {
            if byte == b'\n' {
                line_comment = false;
                mask[i] = true;
            }
            i += 1;
            continue;
        }

        if block_comment {
            if byte == b'*' && bytes.get(i + 1) == Some(&b'/') {
                i += 2;
                block_comment = false;
            } else {
                i += 1;
            }
            continue;
        }

        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            i += 1;
            continue;
        }

        if byte == b'/' && bytes.get(i + 1) == Some(&b'/') {
            line_comment = true;
            i += 2;
            continue;
        }

        if byte == b'/' && bytes.get(i + 1) == Some(&b'*') {
            block_comment = true;
            i += 2;
            continue;
        }

        if matches!(byte, b'\'' | b'"' | b'`') {
            quote = Some(byte);
            i += 1;
            continue;
        }

        mask[i] = true;
        i += 1;
    }

    mask
}

pub fn merge_plans(plans: impl IntoIterator<Item = TestPlan>) -> TestPlan {
    let mut cases = plans
        .into_iter()
        .flat_map(|plan| plan.cases)
        .collect::<Vec<_>>();
    cases.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
    });
    TestPlan { cases }
}

fn is_call_at(source: &str, offset: usize, ident: &str) -> bool {
    let before = source[..offset].chars().next_back();
    if before.is_some_and(is_identifier_char) {
        return false;
    }

    let after_ident = offset + ident.len();
    let tail = &source[after_ident..];
    let Some(next) = tail.chars().find(|ch| !ch.is_whitespace()) else {
        return false;
    };

    next == '('
}

fn extract_first_string_arg(tail_after_ident: &str) -> Option<String> {
    let open = tail_after_ident.find('(')?;
    let tail = tail_after_ident[open + 1..].trim_start();
    let mut chars = tail.chars();
    let quote = chars.next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }

    let mut name = String::new();
    let mut escaped = false;
    for ch in chars {
        if escaped {
            name.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return Some(name);
        }
        name.push(ch);
    }

    None
}

fn is_identifier_char(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_describe_it_and_test_calls() {
        let source = r#"
            import { describe, expect, it, test } from '@uniflowed/testing';

            describe('math', () => {
              it('adds values', () => {});
              test("subtracts values", () => {});
            });
        "#;

        let plan = discover_tests("src/math.test.js", source);

        assert_eq!(plan.cases.len(), 3);
        assert_eq!(plan.runnable_count(), 2);
        assert_eq!(plan.cases[0].kind, TestKind::Describe);
        assert_eq!(plan.cases[1].name, "adds values");
        assert_eq!(plan.cases[2].name, "subtracts values");
    }

    #[test]
    fn ignores_identifier_substrings() {
        let source = r#"
            const within = 'not a test';
            const title = "it('also not a test')";
            it('real test', () => {});
        "#;

        let plan = discover_tests("src/a.test.js", source);

        similar_asserts::assert_eq!(
            plan.cases
                .iter()
                .map(|case| case.name.as_str())
                .collect::<Vec<_>>(),
            vec!["real test"]
        );
    }

    #[test]
    fn merges_plans_in_file_order() {
        let a = discover_tests("b.test.js", "it('b', () => {})");
        let b = discover_tests("a.test.js", "it('a', () => {})");

        let merged = merge_plans([a, b]);

        assert_eq!(merged.cases[0].file, "a.test.js");
        assert_eq!(merged.cases[1].file, "b.test.js");
    }
}
