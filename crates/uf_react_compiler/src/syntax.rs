//! Token-level questions the walk asks, kept out of the walk itself.
//!
//! Each function here answers exactly one thing about a position in the token
//! vector, and each one is bounded: none of them calls itself, and the two that
//! scan forward stop at the end of the construct they were pointed at.

use compact_str::CompactString;
use uf_infra::InlineVec;
use uf_rsc::{Token, TokenKind, matching_close, matching_open};

use crate::bindings::is_reference;

/// Parameter names of one function. Four covers almost every component.
pub type ParamList = InlineVec<CompactString, 4>;

/// Names a hook call was handed. Four covers almost every call.
pub type ArgumentList = InlineVec<CompactString, 4>;

/// Whether the `=` at `index` is an assignment rather than a comparison.
///
/// `==`, `===`, `!=`, `<=` and `>=` are comparisons; `=`, `+=`, `|=` and the
/// rest are writes. `=>` never reaches here because the lexer makes it one
/// token, which is the whole reason the check can be this short.
pub fn is_assignment(tokens: &[Token], index: usize) -> bool {
    if tokens
        .get(index + 1)
        .is_some_and(|token| token.is_punct(b'='))
    {
        return false;
    }
    !index
        .checked_sub(1)
        .and_then(|at| tokens.get(at))
        .is_some_and(|token| matches!(token.kind, TokenKind::Punct(b'=' | b'!' | b'<' | b'>')))
}

/// The root identifier of the member chain ending at `end`.
///
/// `props.style.color` and `props.items[0]` both resolve to `props`; anything
/// rooted in a call result (`load().x`) resolves to nothing, because there is
/// no binding to have an opinion about.
pub fn member_root(tokens: &[Token], end: usize) -> Option<usize> {
    let mut at = end;
    loop {
        let token = tokens.get(at)?;
        if token.kind == TokenKind::Ident {
            match at.checked_sub(1).and_then(|previous| tokens.get(previous)) {
                Some(previous) if previous.is_punct(b'.') => at = at.checked_sub(2)?,
                _ => return Some(at),
            }
        } else if token.is_punct(b']') {
            at = matching_open(tokens, at, b'[', b']')?.checked_sub(1)?;
        } else {
            return None;
        }
    }
}

/// Index of the `(` that opens a declaration's parameter list.
///
/// Scans past a Flow type-parameter list, and gives up at the body or the end
/// of the statement so a malformed declaration cannot walk the rest of the file.
pub fn parameter_list(tokens: &[Token], from: usize) -> Option<usize> {
    /// Longest type-parameter list uf will scan past, in tokens.
    const BUDGET: usize = 64;

    let limit = (from + BUDGET).min(tokens.len());
    for (at, token) in tokens.iter().enumerate().take(limit).skip(from) {
        if token.is_punct(b'(') {
            return Some(at);
        }
        if token.is_punct(b'{') || token.is_punct(b';') {
            return None;
        }
    }
    None
}

/// The parameter names declared in the list opening at `open`.
///
/// A name is a parameter when it follows `(`, `,`, `{`, `[` or a spread, and is
/// not inside a type annotation — which is what tells `flag` apart from
/// `boolean` in `component Page(flag: boolean)`.
pub fn parameters(source: &str, tokens: &[Token], open: usize) -> ParamList {
    let mut names = ParamList::new();
    let Some(close) = matching_close(tokens, open, b'(', b')') else {
        return names;
    };

    let mut in_type = false;
    let mut depth = 0i32;
    for at in open + 1..close {
        let token = &tokens[at];
        match token.kind {
            TokenKind::Punct(b'(' | b'[' | b'{') => depth += 1,
            TokenKind::Punct(b')' | b']' | b'}') => depth -= 1,
            TokenKind::Punct(b':') => in_type = true,
            TokenKind::Punct(b',') if depth <= 0 => in_type = false,
            TokenKind::Ident if !in_type && follows_name_position(tokens, at) => {
                names.push(CompactString::new(token.text(source)));
            }
            _ => {}
        }
    }
    names
}

/// Whether the identifier at `at` sits where a binding name goes.
fn follows_name_position(tokens: &[Token], at: usize) -> bool {
    let Some(previous) = at.checked_sub(1).and_then(|index| tokens.get(index)) else {
        return false;
    };
    if previous.is_punct(b'(') || previous.is_punct(b',') || previous.is_punct(b'{') {
        return true;
    }
    // `...rest` lexes as three `.` tokens followed by the name.
    previous.is_punct(b'.')
        && at
            .checked_sub(2)
            .and_then(|index| tokens.get(index))
            .is_some_and(|token| token.is_punct(b'.'))
}

/// The names a call's argument list refers to.
///
/// Only bare references count: a property name (`config.width`), a callee
/// (`compute(...)`) and anything inside a callback's body are all skipped, so
/// what comes back is the set of values the call was actually handed —
/// including the ones in a dependency array, which is where they usually are.
pub fn argument_names(source: &str, tokens: &[Token], open: usize) -> ArgumentList {
    let mut names = ArgumentList::new();
    let Some(close) = matching_close(tokens, open, b'(', b')') else {
        return names;
    };

    let mut braces = 0i32;
    for at in open + 1..close {
        let token = &tokens[at];
        match token.kind {
            TokenKind::Punct(b'{') => braces += 1,
            TokenKind::Punct(b'}') => braces -= 1,
            TokenKind::Ident if braces == 0 => {
                let name = token.text(source);
                let is_property = at
                    .checked_sub(1)
                    .and_then(|index| tokens.get(index))
                    .is_some_and(|previous| previous.is_punct(b'.'));
                let is_callee = tokens
                    .get(at + 1)
                    .is_some_and(|next| next.is_punct(b'(') || next.is_punct(b'.'));
                if !is_property
                    && !is_callee
                    && is_reference(name)
                    && !names.iter().any(|held| held == name)
                {
                    names.push(CompactString::new(name));
                }
            }
            _ => {}
        }
    }
    names
}

/// Whether the value span after an `=` is a bare reference chain.
///
/// `const items = props.items` aliases the prop and may not be written to;
/// `const items = [...props.items]` builds a new array and may be. Telling the
/// two apart is the difference between a validator that is useful and one that
/// cries wolf on every copy, and a chain of `.` and `[` is exactly the shape
/// that aliases rather than constructs.
pub fn alias_root(tokens: &[Token], equals: usize, end: usize) -> Option<usize> {
    let mut at = equals + 1;
    let mut last = None;
    while at < end {
        let token = &tokens[at];
        match token.kind {
            TokenKind::Ident => last = Some(at),
            TokenKind::Punct(b'.') => {}
            TokenKind::Punct(b'[') => {
                let close = matching_close(tokens, at, b'[', b']')?;
                // Only a constant subscript keeps this an alias.
                if close != at + 2 || tokens[at + 1].kind == TokenKind::Ident {
                    return None;
                }
                last = Some(close);
                at = close;
            }
            _ => return None,
        }
        at += 1;
    }
    member_root(tokens, last?)
}

/// Index one past the end of the statement starting at `from`.
///
/// Stops at the first `;` or line break outside any nested group, and at the
/// end of the enclosing block.
pub fn statement_end(tokens: &[Token], from: usize) -> usize {
    let mut depth = 0i32;
    let mut at = from;
    while at < tokens.len() {
        let token = &tokens[at];
        match token.kind {
            TokenKind::Punct(b'(' | b'[' | b'{') => depth += 1,
            TokenKind::Punct(b')' | b']' | b'}') => {
                depth -= 1;
                if depth < 0 {
                    return at;
                }
            }
            TokenKind::Punct(b';') if depth == 0 => return at,
            _ if depth == 0 && at > from && token.newline_before => return at,
            _ => {}
        }
        at += 1;
    }
    tokens.len()
}

/// Index one past the end of an arrow's concise body starting at `from`.
///
/// A concise body has no closing brace, so its extent is whatever ends the
/// expression: a `,` or `;` at the top level, the closer of the group it sits
/// in, or the start of the next statement in a module written without
/// semicolons.
pub fn concise_body_end(source: &str, tokens: &[Token], from: usize) -> usize {
    let mut depth = 0i32;
    let mut at = from;
    while at < tokens.len() {
        let token = &tokens[at];
        match token.kind {
            TokenKind::Punct(b'(' | b'[' | b'{') => depth += 1,
            TokenKind::Punct(b')' | b']' | b'}') => {
                if depth == 0 {
                    return at;
                }
                depth -= 1;
            }
            TokenKind::Punct(b',' | b';') if depth == 0 => return at,
            TokenKind::Ident
                if depth == 0
                    && at > from
                    && token.newline_before
                    && STATEMENT_KEYWORDS
                        .binary_search(&token.text(source))
                        .is_ok() =>
            {
                return at;
            }
            _ => {}
        }
        at += 1;
    }
    tokens.len()
}

/// Words that begin a statement, and so end a concise body that ran onto the
/// next line without a semicolon. Sorted for binary search.
const STATEMENT_KEYWORDS: &[&str] = &[
    "class",
    "component",
    "const",
    "declare",
    "export",
    "for",
    "function",
    "hook",
    "if",
    "import",
    "let",
    "return",
    "switch",
    "throw",
    "try",
    "var",
    "while",
];

/// Whether the `=` at `index` is preceded by a compound-assignment operator.
///
/// `total += 1` writes to `total`, and the operator sits between the target and
/// the `=`, so the target's chain ends one token further back.
pub fn compound_assignment(tokens: &[Token], index: usize) -> bool {
    index
        .checked_sub(1)
        .and_then(|at| tokens.get(at))
        .is_some_and(|token| {
            matches!(
                token.kind,
                TokenKind::Punct(b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^')
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_statement_keyword_table_is_sorted_for_binary_search() {
        assert!(STATEMENT_KEYWORDS.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
