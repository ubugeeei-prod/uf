//! `app.router.redirects`, `rewrites` and `headers`, refused where they are
//! written.
//!
//! Next.js reads the same three from `next.config.js`, as the return values of
//! async functions, with `path-to-regexp` sources. uf reads them as data — the
//! file is read without being run — and matches a source with the route table's
//! own grammar: a literal segment, `:name` for one segment, and a trailing
//! `:name*` for the rest. `@uniflowed/server`'s `internal/routing.js` is the
//! matcher every host shares; this is the half that refuses what that matcher
//! would not read, at the file, with a sentence per spelling.
//!
//! # Why a refusal rather than a best effort
//!
//! Every spelling here is one a Next.js project already has. A regular
//! expression, a `:slug+`, a parameter inside a segment: each of them, read
//! as a literal, is a rule that matches nothing — so the redirect a site's old
//! links depend on quietly stops happening, and nothing says so until the
//! traffic is gone. The rule this repository holds itself to is that an
//! unsupported configuration is rejected clearly rather than silently changing
//! meaning, and a rule that matches nothing is the quietest change there is.

use camino::Utf8Path;

use crate::{ConfigError, UniflowedConfig};

/// Refuse any rule the hosts would match differently from how it reads.
pub(crate) fn check(path: &Utf8Path, config: &UniflowedConfig) -> Result<(), ConfigError> {
    let router = &config.app.router;
    if let Err(reason) = base_path(&router.base_path) {
        return Err(ConfigError::RouterBasePath {
            path: path.to_path_buf(),
            written: router.base_path.to_string(),
            reason,
        });
    }
    let refuse = |key: &'static str, index: usize, reason: String| ConfigError::RouterRule {
        path: path.to_path_buf(),
        key,
        index,
        reason,
    };

    for (index, rule) in router.redirects.iter().enumerate() {
        let declared =
            source_params(&rule.source).map_err(|reason| refuse("redirects", index, reason))?;
        destination(&rule.destination, &declared, Target::PathOrUrl)
            .map_err(|reason| refuse("redirects", index, reason))?;
        if declared.is_empty() && same_path(&rule.source, &rule.destination) {
            return Err(refuse(
                "redirects",
                index,
                format!(
                    "redirects {:?} to itself, which is a loop a browser follows until it gives up",
                    rule.source.as_str()
                ),
            ));
        }
    }

    for (index, rule) in router.rewrites.iter().enumerate() {
        let declared =
            source_params(&rule.source).map_err(|reason| refuse("rewrites", index, reason))?;
        destination(&rule.destination, &declared, Target::Path)
            .map_err(|reason| refuse("rewrites", index, reason))?;
    }

    for (index, rule) in router.headers.iter().enumerate() {
        source_params(&rule.source).map_err(|reason| refuse("headers", index, reason))?;
        if rule.headers.is_empty() {
            return Err(refuse(
                "headers",
                index,
                String::from("sets no header: `headers` is empty"),
            ));
        }
        for (name, value) in &rule.headers {
            if name.is_empty() || !name.bytes().all(is_token_byte) {
                return Err(refuse(
                    "headers",
                    index,
                    format!("names the header {name:?}, which is not a header name"),
                ));
            }
            if value.contains(['\r', '\n', '\0']) {
                return Err(refuse(
                    "headers",
                    index,
                    format!(
                        "gives `{name}` a value with a line break in it, which would end the \
                         header and start another"
                    ),
                ));
            }
        }
    }

    Ok(())
}

/// Why `basePath` is not one, if it is not.
///
/// A base is literal segments: every host compares it with the start of each
/// request's path, so a parameter, a query or a trailing slash in it would be
/// compared as characters and match nothing a visitor sends.
fn base_path(base: &str) -> Result<(), String> {
    if base.is_empty() {
        return Ok(());
    }
    if base == "/" {
        return Err(String::from(
            "which is the root, and the root is written as no base path at all",
        ));
    }
    if !base.starts_with('/') {
        return Err(String::from("and a base path starts with `/`"));
    }
    if base.ends_with('/') {
        return Err(format!(
            "and a base path has no trailing slash: write {:?}",
            base.trim_end_matches('/')
        ));
    }
    if base.contains(['?', '#']) || base.contains(char::is_whitespace) {
        return Err(String::from(
            "and a base path is a path: no query, no fragment and no whitespace",
        ));
    }
    for segment in base.split('/').skip(1) {
        if segment.is_empty() {
            return Err(String::from("and a base path has no empty segment (`//`)"));
        }
        if segment == "." || segment == ".." {
            return Err(String::from(
                "and `.` and `..` are not segments a base path can have",
            ));
        }
        if segment.contains([':', '*']) {
            return Err(format!(
                "and `{segment}` is a pattern; a base path is literal segments"
            ));
        }
    }
    Ok(())
}

/// Whether a parameter takes one segment or the rest of the path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Takes {
    One,
    Rest,
}

/// What a destination may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// A redirect's: a path of this application, or another origin.
    PathOrUrl,
    /// A rewrite's: a path of this application, and nothing else.
    Path,
}

/// The parameters a source declares, or why it is not a source.
fn source_params(source: &str) -> Result<Vec<(String, Takes)>, String> {
    if !source.starts_with('/') {
        return Err(format!(
            "has the source {source:?}, and a source is a path that starts with `/`"
        ));
    }
    let segments: Vec<&str> = source.split('/').filter(|part| !part.is_empty()).collect();
    // Next.js's modifiers before the query check below, which `:slug?` would
    // otherwise be read as.
    for segment in &segments {
        if let Some(parameter) = segment.strip_prefix(':')
            && let Some(name) = parameter
                .strip_suffix('+')
                .or_else(|| parameter.strip_suffix('?'))
        {
            return Err(format!(
                "has the source {source:?}, and `{segment}` is a Next.js modifier uf does not \
                 read. `:{name}*` takes the rest of the path, including none of it; a page that \
                 needs at least one segment can call `notFound()` without it."
            ));
        }
    }
    if source.contains(['?', '#']) {
        return Err(format!(
            "has the source {source:?}, and a source matches a path: the query and the fragment \
             are not part of one. A rule that depends on the query is a middleware."
        ));
    }
    if source.contains(['(', ')']) {
        return Err(format!(
            "has the source {source:?}, and uf does not read a regular expression in a source. \
             Write the segments, with `:name` for one and a trailing `:name*` for the rest."
        ));
    }
    if source.contains(char::is_whitespace) {
        return Err(format!(
            "has the source {source:?}, which has whitespace in it; write it percent-encoded, \
             the way a request carries it"
        ));
    }

    let mut declared: Vec<(String, Takes)> = Vec::new();
    for (position, segment) in segments.iter().enumerate() {
        let Some(parameter) = segment.strip_prefix(':') else {
            if segment.contains([':', '*']) {
                return Err(format!(
                    "has the source {source:?}, and `{segment}` puts a pattern inside a segment. \
                     A parameter is a whole segment — `:name`, or `:name*` for the rest of the \
                     path — and a page can check the characters around it."
                ));
            }
            continue;
        };
        let (name, takes) = match parameter.strip_suffix('*') {
            Some(name) => (name, Takes::Rest),
            None => (parameter, Takes::One),
        };
        if !is_parameter_name(name) {
            return Err(format!(
                "has the source {source:?}, and `{segment}` does not name a parameter: a name is \
                 letters, digits and `_`, and does not start with a digit"
            ));
        }
        if takes == Takes::Rest && position + 1 != segments.len() {
            return Err(format!(
                "has the source {source:?}, and `:{name}*` takes the rest of the path, so it has \
                 to be the last segment"
            ));
        }
        if declared.iter().any(|(existing, _)| existing == name) {
            return Err(format!(
                "has the source {source:?}, which declares `:{name}` twice"
            ));
        }
        declared.push((name.to_owned(), takes));
    }
    Ok(declared)
}

/// Why a destination is not one, if it is not.
fn destination(
    destination: &str,
    declared: &[(String, Takes)],
    target: Target,
) -> Result<(), String> {
    let absolute = destination.starts_with("//") || authority_end(destination).is_some();
    if destination.is_empty() {
        return Err(String::from("has an empty destination"));
    }
    let path = match (target, absolute) {
        (Target::Path, true) => {
            return Err(format!(
                "has the destination {destination:?}, which is another origin. A rewrite serves \
                 a route of this application: proxying to another server is a route handler \
                 that fetches it, and sending the visitor there is a redirect."
            ));
        }
        (Target::PathOrUrl, true) => {
            let Some(end) = authority_end(destination) else {
                return Err(format!(
                    "has the destination {destination:?}, which names no scheme: write \
                     `https://` in front of the host"
                ));
            };
            if !(destination.starts_with("http://") || destination.starts_with("https://")) {
                return Err(format!(
                    "has the destination {destination:?}, and a redirect sends a browser to a \
                     path of this application or to an `http` or `https` URL"
                ));
            }
            &destination[end..]
        }
        (_, false) => {
            if !destination.starts_with('/') {
                return Err(format!(
                    "has the destination {destination:?}, and a destination is a path that \
                     starts with `/`{}",
                    if target == Target::PathOrUrl {
                        " or an absolute `http(s)` URL"
                    } else {
                        ""
                    }
                ));
            }
            destination
        }
    };
    let path = path.split(['?', '#']).next().unwrap_or_default();
    for segment in path.split('/') {
        let Some(parameter) = segment.strip_prefix(':') else {
            continue;
        };
        let (name, takes) = match parameter.strip_suffix('*') {
            Some(name) => (name, Takes::Rest),
            None => (parameter, Takes::One),
        };
        match declared.iter().find(|(existing, _)| existing == name) {
            None => {
                return Err(format!(
                    "has the destination {destination:?}, which uses `{segment}` — and the \
                     source declares no `:{name}` to fill it with"
                ));
            }
            Some((_, Takes::Rest)) if takes == Takes::One => {
                return Err(format!(
                    "has the destination {destination:?}, and `:{name}` takes the rest of the \
                     path in the source, so write `:{name}*` here"
                ));
            }
            Some((_, Takes::One)) if takes == Takes::Rest => {
                return Err(format!(
                    "has the destination {destination:?}, and `:{name}` takes one segment in \
                     the source, so write `:{name}` here"
                ));
            }
            Some(_) => {}
        }
    }
    Ok(())
}

/// Where the scheme and authority of an absolute URL end, if `value` is one.
fn authority_end(value: &str) -> Option<usize> {
    let scheme = value.find("://")?;
    if scheme == 0 || value[..scheme].contains('/') {
        return None;
    }
    let rest = scheme + 3;
    Some(
        value[rest..]
            .find(['/', '?', '#'])
            .map_or(value.len(), |at| rest + at),
    )
}

/// Whether two paths name the same route, ignoring trailing slashes and a query.
fn same_path(source: &str, destination: &str) -> bool {
    let path = destination.split(['?', '#']).next().unwrap_or_default();
    let trimmed = |value: &str| value.trim_end_matches('/').to_owned();
    destination.split_once('?').is_none() && trimmed(source) == trimmed(path)
}

fn is_parameter_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// A byte an HTTP field name may hold: RFC 9110's `tchar`.
const fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

#[cfg(test)]
mod tests;
