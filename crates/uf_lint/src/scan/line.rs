//! Classifying one physical line into the part of it that is really code.
//!
//! Comments and string literals do not respect line boundaries, so the scanner
//! threads a [`Carry`] from each line into the next. Everything a rule later sees
//! as "the code on this line" is decided here.

/// State carried from one line into the next by the comment scanner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Carry {
    /// Ordinary code.
    Code,
    /// Inside a `/* ... */` comment.
    BlockComment,
    /// Inside a backtick template literal.
    Template,
}

/// Result of classifying one physical line.
pub(crate) struct ScannedLine {
    pub(crate) code_start: usize,
    pub(crate) code_end: usize,
    pub(crate) carry: Carry,
    /// Net `{` minus `}` outside comments and string literals.
    pub(crate) brace_delta: i32,
}

/// Classify one line into `[code_start, code_end)` plus the state the next line
/// starts in, counting braces that are really braces on the way.
///
/// Known, deliberate simplification: when a `/* ... */` comment opens *and*
/// closes on the same line, the code after the closing `*/` is dropped rather
/// than stitched back together. That costs a few diagnostics on lines like
/// `const a = /* why */ any;` and never invents one, which is the right way for a
/// linter to be wrong.
pub(crate) fn scan_line(text: &str, carry: Carry) -> ScannedLine {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut code_start = 0usize;

    match carry {
        Carry::BlockComment => match find_pair(bytes, 0, b'*', b'/') {
            Some(pos) => {
                i = pos + 2;
                code_start = i;
            }
            None => {
                return ScannedLine {
                    code_start: 0,
                    code_end: 0,
                    carry: Carry::BlockComment,
                    brace_delta: 0,
                };
            }
        },
        Carry::Template => match find_unescaped(bytes, 0, b'`') {
            Some(pos) => i = pos + 1,
            None => {
                return ScannedLine {
                    code_start: 0,
                    code_end: len,
                    carry: Carry::Template,
                    brace_delta: 0,
                };
            }
        },
        Carry::Code => {}
    }

    let mut carry = Carry::Code;
    let mut code_end = len;
    let mut brace_delta = 0i32;
    while i < len {
        match bytes[i] {
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                code_end = i;
                break;
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                code_end = i;
                if find_pair(bytes, i + 2, b'*', b'/').is_none() {
                    carry = Carry::BlockComment;
                }
                break;
            }
            quote @ (b'\'' | b'"') => {
                i = match find_unescaped(bytes, i + 1, quote) {
                    Some(pos) => pos + 1,
                    None => len,
                };
            }
            b'`' => match find_unescaped(bytes, i + 1, b'`') {
                Some(pos) => i = pos + 1,
                None => {
                    carry = Carry::Template;
                    i = len;
                }
            },
            b'{' => {
                brace_delta += 1;
                i += 1;
            }
            b'}' => {
                brace_delta -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }

    ScannedLine {
        code_start,
        code_end: code_end.max(code_start),
        carry,
        brace_delta,
    }
}

/// First `needle` at or after `from` that is not backslash escaped.
fn find_unescaped(bytes: &[u8], from: usize, needle: u8) -> Option<usize> {
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            byte if byte == needle => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// First occurrence of the two-byte sequence `first`,`second` at or after `from`.
fn find_pair(bytes: &[u8], from: usize, first: u8, second: u8) -> Option<usize> {
    if from >= bytes.len() {
        return None;
    }
    uf_infra::memchr_iter(first, &bytes[from..])
        .map(|offset| from + offset)
        .find(|&pos| bytes.get(pos + 1) == Some(&second))
}
