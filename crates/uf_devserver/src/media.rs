//! The closed extension-to-media-type table.
//!
//! # Threat model
//!
//! A `Content-Type` derived from attacker-influenced text is an XSS primitive:
//! upload or name a file so the browser is told to treat it as HTML, and the
//! page runs in the dev server's origin. So the mapping is a fixed table of
//! `&'static str` pairs, an unknown extension falls through to
//! `application/octet-stream` rather than being guessed, and every response
//! carries `X-Content-Type-Options: nosniff` so the browser does not guess
//! either.
//!
//! The extension is read from the *canonical* path — the file that was actually
//! opened — not from the request, for the same reason every other decision in
//! this crate is made on the canonical path.

/// A media type this server is willing to name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MediaType {
    /// Anything not on the table.
    #[default]
    Binary,
    /// A known type, held as its full header value.
    Known(&'static str),
}

impl MediaType {
    /// The `Content-Type` header value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Binary => "application/octet-stream",
            Self::Known(value) => value,
        }
    }

    /// Look up the media type for a file extension, case-insensitively.
    pub fn for_extension(extension: Option<&str>) -> Self {
        let Some(extension) = extension else {
            return Self::Binary;
        };
        if extension.len() > MAX_EXTENSION_BYTES {
            return Self::Binary;
        }
        MEDIA_TYPES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(extension))
            .map_or(Self::Binary, |(_, value)| Self::Known(value))
    }
}

/// Longest extension worth a table scan.
const MAX_EXTENSION_BYTES: usize = 16;

/// Extensions the dev server will name a type for. Sorted for readability; the
/// lookup is a linear scan because the table is small and the comparison is
/// case-insensitive.
const MEDIA_TYPES: &[(&str, &str)] = &[
    ("avif", "image/avif"),
    ("css", "text/css; charset=utf-8"),
    ("gif", "image/gif"),
    ("htm", "text/html; charset=utf-8"),
    ("html", "text/html; charset=utf-8"),
    ("ico", "image/x-icon"),
    ("jpeg", "image/jpeg"),
    ("jpg", "image/jpeg"),
    ("js", "text/javascript; charset=utf-8"),
    ("json", "application/json"),
    ("map", "application/json"),
    ("mjs", "text/javascript; charset=utf-8"),
    ("png", "image/png"),
    ("svg", "image/svg+xml"),
    ("txt", "text/plain; charset=utf-8"),
    ("wasm", "application/wasm"),
    ("webp", "image/webp"),
    ("woff", "font/woff"),
    ("woff2", "font/woff2"),
];
