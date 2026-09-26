//! Finding the placeholders in a statement sqlc rewrote.
//!
//! PostgreSQL's `$1` names its parameter, and a generator can pass values by
//! number. MySQL and SQLite bind by position, and sqlc's output for them mixes
//! bare `?` with SQLite's numbered `?N` — which SQLite binds by a rule sqlc
//! did not follow (a bare `?` takes the number after the largest one so far,
//! not the next in order). So for those engines the generator finds every
//! placeholder, decides which parameter each one means, and rewrites them all
//! as bare `?` in bind order. The values array it emits then says the same
//! thing to every driver.

use crate::proto::Parameter;

/// One placeholder in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mark {
    /// `?`.
    Bare,
    /// `?7`.
    Numbered(i32),
    /// `/*SLICE:ids*/?`.
    Slice(String),
    /// `$7`.
    Dollar(i32),
}

/// A placeholder and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub start: usize,
    pub end: usize,
    pub mark: Mark,
}

/// Which quoting rules apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    Postgresql,
    Mysql,
    Sqlite,
}

/// Every placeholder in `text`, skipping string literals, quoted identifiers
/// and comments.
#[must_use]
pub fn scan(text: &str, dialect: Dialect) -> Vec<Found> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'\'' => at = skip_quoted(bytes, at, b'\'', dialect == Dialect::Mysql),
            b'"' => at = skip_quoted(bytes, at, b'"', dialect == Dialect::Mysql),
            b'`' if dialect != Dialect::Postgresql => at = skip_quoted(bytes, at, b'`', false),
            b'[' if dialect == Dialect::Sqlite => {
                at = text[at..].find(']').map_or(bytes.len(), |end| at + end + 1);
            }
            b'-' if bytes.get(at + 1) == Some(&b'-') => {
                at = text[at..]
                    .find('\n')
                    .map_or(bytes.len(), |end| at + end + 1);
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                let end = text[at + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |end| at + 2 + end + 2);
                let comment = &text[at..end];
                if let Some(name) = comment
                    .strip_prefix("/*SLICE:")
                    .and_then(|rest| rest.strip_suffix("*/"))
                    && bytes.get(end) == Some(&b'?')
                {
                    out.push(Found {
                        start: at,
                        end: end + 1,
                        mark: Mark::Slice(name.to_owned()),
                    });
                    at = end + 1;
                } else {
                    at = end;
                }
            }
            b'$' if dialect == Dialect::Postgresql => {
                // `$1`, but not the `$` of a dollar-quoted string (`$$…$$`,
                // `$tag$…$tag$`), which is skipped whole.
                let digits = count_digits(bytes, at + 1);
                if digits > 0 {
                    let number = text[at + 1..at + 1 + digits].parse().unwrap_or(0);
                    out.push(Found {
                        start: at,
                        end: at + 1 + digits,
                        mark: Mark::Dollar(number),
                    });
                    at += 1 + digits;
                } else if let Some(tag_end) = dollar_tag(bytes, at) {
                    let tag = &text[at..=tag_end];
                    at = text[tag_end + 1..]
                        .find(tag)
                        .map_or(bytes.len(), |end| tag_end + 1 + end + tag.len());
                } else {
                    at += 1;
                }
            }
            b'?' if dialect != Dialect::Postgresql => {
                let digits = count_digits(bytes, at + 1);
                let mark = if digits > 0 {
                    Mark::Numbered(text[at + 1..at + 1 + digits].parse().unwrap_or(0))
                } else {
                    Mark::Bare
                };
                out.push(Found {
                    start: at,
                    end: at + 1 + digits,
                    mark,
                });
                at += 1 + digits;
            }
            _ => at += 1,
        }
    }
    out
}

fn count_digits(bytes: &[u8], from: usize) -> usize {
    bytes[from.min(bytes.len())..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count()
}

/// The end of a `$tag$` opening a dollar-quoted string at `at`, if it is one.
fn dollar_tag(bytes: &[u8], at: usize) -> Option<usize> {
    let mut end = at + 1;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    (bytes.get(end) == Some(&b'$')).then_some(end)
}

/// Past a quoted run starting at `at`. A doubled quote is an escaped quote in
/// every dialect; MySQL strings also take a backslash escape.
fn skip_quoted(bytes: &[u8], at: usize, quote: u8, backslash: bool) -> usize {
    let mut i = at + 1;
    while i < bytes.len() {
        if backslash && bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            if bytes.get(i + 1) == Some(&quote) {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

/// The parameter each placeholder of a `?`-engine statement means, as an
/// index into `params` (sqlc's order).
///
/// `?N` means the parameter numbered N and a slice marker the slice parameter
/// of that name. A bare `?` means the next parameter, in sqlc's order, that
/// no placeholder has named yet — which is how sqlc numbers them when it
/// writes the text.
///
/// # Errors
///
/// When a placeholder names no parameter, or a parameter has no placeholder.
pub fn bind_order(found: &[Found], params: &[Parameter]) -> Result<Vec<usize>, String> {
    let mut used = vec![false; params.len()];
    let mut order = Vec::with_capacity(found.len());
    for mark in found {
        let index = match &mark.mark {
            Mark::Numbered(number) | Mark::Dollar(number) => params
                .iter()
                .position(|param| param.number == *number)
                .ok_or_else(|| {
                    uf_infra::into_string(uf_infra::cstr!(
                        "the placeholder ?{number} names no parameter"
                    ))
                })?,
            Mark::Slice(name) => params
                .iter()
                .position(|param| {
                    param
                        .column
                        .as_ref()
                        .is_some_and(|column| column.is_sqlc_slice && column.name == *name)
                })
                .ok_or_else(|| {
                    uf_infra::into_string(uf_infra::cstr!("sqlc.slice('{name}') has no parameter"))
                })?,
            Mark::Bare => used
                .iter()
                .enumerate()
                .position(|(index, used)| {
                    !used
                        && !params[index]
                            .column
                            .as_ref()
                            .is_some_and(|column| column.is_sqlc_slice)
                })
                .ok_or_else(|| "a placeholder has no parameter left to take".to_owned())?,
        };
        used[index] = true;
        order.push(index);
    }
    if let Some(unused) = used.iter().position(|used| !used) {
        let name = params[unused]
            .column
            .as_ref()
            .map_or("", |column| column.name.as_str());
        return Err(uf_infra::cstr!(
            "parameter {} ({name}) appears in no placeholder",
            params[unused].number
        )
        .into_string());
    }
    Ok(order)
}

/// `text` with every placeholder replaced by a bare `?`, split at the slice
/// markers (which are dropped): one more part than there are slices.
#[must_use]
pub fn positional_parts(text: &str, found: &[Found]) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut from = 0;
    for mark in found {
        let last = parts.last_mut().expect("parts starts non-empty");
        last.push_str(&text[from..mark.start]);
        if matches!(mark.mark, Mark::Slice(_)) {
            parts.push(String::new());
        } else {
            last.push('?');
        }
        from = mark.end;
    }
    parts
        .last_mut()
        .expect("parts starts non-empty")
        .push_str(&text[from..]);
    parts
}

/// An `INSERT … VALUES (…)` split for `:copyfrom`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopySplit {
    pub head: String,
    /// The tuple around its placeholders: one more part than placeholders.
    pub tuple: Vec<String>,
    /// For each placeholder in the tuple, the index into `params`.
    pub refs: Vec<usize>,
    pub tail: String,
}

/// Split a `:copyfrom` statement at its `VALUES (…)` tuple.
///
/// # Errors
///
/// When the statement has no single `VALUES` tuple outside quotes, or a
/// placeholder outside it.
pub fn copy_split(text: &str, dialect: Dialect, params: &[Parameter]) -> Result<CopySplit, String> {
    let found = scan(text, dialect);
    let order = if dialect == Dialect::Postgresql {
        found
            .iter()
            .map(|mark| match mark.mark {
                Mark::Dollar(number) => params
                    .iter()
                    .position(|param| param.number == number)
                    .ok_or_else(|| {
                        uf_infra::cstr!("the placeholder ${number} names no parameter")
                            .into_string()
                    }),
                _ => Err("an unexpected placeholder".to_owned()),
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        bind_order(&found, params)?
    };
    let upper = text.to_ascii_uppercase();
    let values = find_keyword(&upper, "VALUES")
        .ok_or_else(|| ":copyfrom needs an INSERT … VALUES (…) statement".to_owned())?;
    let open = text[values..]
        .find('(')
        .map(|offset| values + offset)
        .ok_or_else(|| ":copyfrom needs a parenthesised VALUES tuple".to_owned())?;
    let close = matching_paren(text, open, dialect)
        .ok_or_else(|| ":copyfrom's VALUES tuple is not closed".to_owned())?;
    let mut tuple = vec![String::new()];
    let mut refs = Vec::new();
    let mut from = open;
    for (mark, index) in found.iter().zip(&order) {
        if mark.start < open || mark.end > close {
            return Err(":copyfrom takes parameters only inside its VALUES tuple".to_owned());
        }
        tuple
            .last_mut()
            .expect("non-empty")
            .push_str(&text[from..mark.start]);
        tuple.push(String::new());
        refs.push(*index);
        from = mark.end;
    }
    tuple
        .last_mut()
        .expect("non-empty")
        .push_str(&text[from..=close]);
    Ok(CopySplit {
        head: text[..open].to_owned(),
        tuple,
        refs,
        tail: text[close + 1..].to_owned(),
    })
}

/// The end of the first `keyword` in `upper` that is a whole word. sqlc only
/// accepts a plain `INSERT INTO t (…) VALUES (…)` for `:copyfrom`, so the
/// first `VALUES` is the one.
fn find_keyword(upper: &str, keyword: &str) -> Option<usize> {
    let bytes = upper.as_bytes();
    let mut from = 0;
    while let Some(offset) = upper[from..].find(keyword) {
        let start = from + offset;
        let end = start + keyword.len();
        let before = start == 0 || !is_word(bytes[start - 1]);
        let after = end >= bytes.len() || !is_word(bytes[end]);
        if before && after {
            return Some(end);
        }
        from = end;
    }
    None
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn matching_paren(text: &str, open: usize, dialect: Dialect) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut at = open;
    while at < bytes.len() {
        match bytes[at] {
            b'\'' => {
                at = skip_quoted(bytes, at, b'\'', dialect == Dialect::Mysql);
                continue;
            }
            b'"' => {
                at = skip_quoted(bytes, at, b'"', dialect == Dialect::Mysql);
                continue;
            }
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::Column;

    fn param(number: i32, name: &str, slice: bool) -> Parameter {
        Parameter {
            number,
            column: Some(Column {
                name: name.to_owned(),
                is_sqlc_slice: slice,
                ..Column::default()
            }),
        }
    }

    #[test]
    fn skips_literals_and_comments() {
        let found = scan(
            "SELECT '?', \"?\", `?` -- ?\n FROM t WHERE a = ? /* ? */",
            Dialect::Mysql,
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].mark, Mark::Bare);
    }

    #[test]
    fn reads_slices_and_numbers() {
        let text = "SELECT id FROM people WHERE id IN (/*SLICE:ids*/?) AND name <> ?2";
        let found = scan(text, Dialect::Sqlite);
        assert_eq!(found[0].mark, Mark::Slice("ids".into()));
        assert_eq!(found[1].mark, Mark::Numbered(2));
        let params = [param(1, "ids", true), param(2, "skip", false)];
        assert_eq!(bind_order(&found, &params).unwrap(), [0, 1]);
        assert_eq!(
            positional_parts(text, &found),
            ["SELECT id FROM people WHERE id IN (", ") AND name <> ?"]
        );
    }

    #[test]
    fn binds_sqlites_mixed_placeholders_the_way_sqlc_numbered_them() {
        // `sqlc.narg('min')` became `?2` and the `LIMIT ?` is parameter 1:
        // SQLite would bind that bare `?` as 3.
        let text = "SELECT id FROM people WHERE age > ?2 LIMIT ?";
        let found = scan(text, Dialect::Sqlite);
        let params = [param(2, "min", false), param(1, "limit", false)];
        assert_eq!(bind_order(&found, &params).unwrap(), [0, 1]);
        assert_eq!(
            positional_parts(text, &found),
            ["SELECT id FROM people WHERE age > ? LIMIT ?"]
        );
    }

    #[test]
    fn a_repeated_named_parameter_is_bound_twice() {
        let found = scan("SELECT 1 WHERE a = ?1 OR b = ?1", Dialect::Sqlite);
        assert_eq!(bind_order(&found, &[param(1, "n", false)]).unwrap(), [0, 0]);
    }

    #[test]
    fn postgres_dollar_quotes_are_not_placeholders() {
        let found = scan("SELECT $$ $1 $$, $tag$ $2 $tag$, $1", Dialect::Postgresql);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].mark, Mark::Dollar(1));
    }

    #[test]
    fn splits_copyfrom() {
        let text = "INSERT INTO pets (owner_id, name) VALUES ($1, $2)";
        let split = copy_split(
            text,
            Dialect::Postgresql,
            &[param(1, "owner_id", false), param(2, "name", false)],
        )
        .unwrap();
        assert_eq!(split.head, "INSERT INTO pets (owner_id, name) VALUES ");
        assert_eq!(split.tuple, ["(", ", ", ")"]);
        assert_eq!(split.refs, [0, 1]);
        assert_eq!(split.tail, "");
    }
}
