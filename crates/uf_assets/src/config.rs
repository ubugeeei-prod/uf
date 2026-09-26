//! What a project declares about its images and fonts.
//!
//! These live here rather than in `uf_config` for the reason `BundleBudgets`
//! lives in `uf_bundle`: the crate that acts on a setting owns its shape, and
//! `uf_config` re-exports it. A setting whose only definition is in the config
//! crate is a setting the crate that reads it has to agree with by hand.
//!
//! Every key here is uf's own. None of them is a second name for something
//! Vite already names, which is the line `docs/red-lines.md` draws around
//! re-declaring an upstream tool's schema — Vite has no image pipeline to
//! re-declare.

use serde::{Deserialize, Serialize};

/// The widths uf emits when a project does not say.
///
/// Two device-pixel-ratio ladders rather than one: 640/828/1200/1920 is the
/// common CSS-pixel set at 1x, and 750/1080/1440 covers the same layouts on the
/// phones that report 2x and 3x. A project with one fixed-width column should
/// replace the list rather than keep paying for the ones it never serves.
pub const DEFAULT_WIDTHS: &[u32] = &[640, 750, 828, 1080, 1200, 1440, 1920];

/// The JPEG quality uf encodes at when a project does not say.
///
/// 75 is where the artefacts stop being visible on a photograph at the sizes
/// above while the file is a fraction of a visually lossless encode.
pub const DEFAULT_QUALITY: u8 = 75;

/// What a project declares about its images.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct ImagesConfig {
    /// Whether imported images go through the pipeline at all.
    pub enabled: bool,
    /// The widths a layout asks for.
    pub widths: Vec<u32>,
    /// JPEG quality, 1–100.
    pub quality: u8,
    /// Whether to generate the blur placeholder.
    pub placeholder: bool,
    /// The remote images the request-time endpoint may fetch.
    ///
    /// Empty by default, and empty means there is no endpoint at all: no
    /// server uf starts or links answers `/__uf/image`. A remote image is
    /// fetched only when its URL matches one of these, and so is every
    /// redirect on the way to it. See `@uniflowed/server/image`.
    pub remote_patterns: Vec<RemotePattern>,
    /// The qualities the endpoint accepts, besides [`quality`](Self::quality).
    ///
    /// A bound rather than a range: every quality a query string may name is
    /// one more encode of every remote image a stranger can ask for, so the
    /// endpoint answers only the ones listed here and the project's own.
    pub qualities: Vec<u8>,
    /// Let the endpoint fetch from loopback, private and link-local addresses.
    ///
    /// Off, and meant to stay off anywhere but a test or a machine nobody else
    /// can reach. An image proxy that can be pointed at `127.0.0.1` or
    /// `169.254.169.254` is a way into the network the server sits on, which
    /// is why the default refuses them after resolving the name rather than
    /// by reading it.
    pub dangerously_allow_private_addresses: bool,
    /// A module exporting `createImageTransformer`, for a deploy target that
    /// has no encoder of its own.
    ///
    /// `uf start` and `uf preview` encode with `uf` itself, and
    /// `--adapter edge` with Cloudflare's image binding. A directory
    /// `--adapter node`, `bun`, `deno`, `container` or `serverless` writes
    /// carries neither, so a project that wants the endpoint there names the
    /// encoder it has — a module specifier resolved from the project, the same
    /// shape as `rendering.cache.store`.
    pub transformer: Option<String>,
}

impl Default for ImagesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            widths: DEFAULT_WIDTHS.to_vec(),
            quality: DEFAULT_QUALITY,
            placeholder: true,
            remote_patterns: Vec::new(),
            qualities: Vec::new(),
            dangerously_allow_private_addresses: false,
            transformer: None,
        }
    }
}

/// One entry in the endpoint's allow-list.
///
/// Next.js's `images.remotePatterns`, with the same four keys and the same
/// wildcard grammar, because a project moving across should not have to
/// translate its list. `@uniflowed/server`'s `internal/remote-patterns.js` is
/// the matcher every host runs; [`check_remote_pattern`] refuses at the config
/// file what that matcher would not read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct RemotePattern {
    /// `"https"` or `"http"`. Absent means `"https"` only.
    pub protocol: Option<String>,
    /// The host: a name, or a name under a leading `*.` (exactly one more
    /// label) or `**.` (any number).
    pub hostname: String,
    /// The port, as digits. Absent means the protocol's default port and no
    /// other.
    pub port: Option<String>,
    /// A path the image must be under: `*` is one segment and `**` any number.
    /// Absent means any path.
    pub pathname: Option<String>,
}

/// Why a remote pattern cannot be matched the way it reads, or `Ok`.
///
/// The refusals are the spellings that would widen the list past what the
/// author meant — a bare `*` host, a wildcard in the middle of a name — and
/// the ones that would match nothing at all, which is the quieter failure of
/// an allow-list and the reason this is a refusal rather than a best effort.
///
/// # Errors
///
/// A sentence naming what is wrong with the pattern.
pub fn check_remote_pattern(pattern: &RemotePattern) -> Result<(), String> {
    if let Some(protocol) = pattern.protocol.as_deref()
        && protocol != "https"
        && protocol != "http"
    {
        return Err(uf_infra::into_string(uf_infra::cstr!(
            "protocol {protocol:?} is not `\"https\"` or `\"http\"`; the endpoint fetches nothing else"
        )));
    }
    let hostname = pattern.hostname.as_str();
    if hostname.is_empty() {
        return Err(String::from("has no `hostname`"));
    }
    let named = hostname
        .strip_prefix("**.")
        .or_else(|| hostname.strip_prefix("*."))
        .unwrap_or(hostname);
    if named.is_empty() || named.contains('*') {
        return Err(uf_infra::into_string(uf_infra::cstr!(
            "hostname {hostname:?} puts a wildcard somewhere other than a leading `*.` or `**.`, \
             or is nothing but one; an allow-list that admits every host is not an allow-list"
        )));
    }
    if !named.split('.').all(|label| {
        !label.is_empty()
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    }) {
        return Err(uf_infra::into_string(uf_infra::cstr!(
            "hostname {hostname:?} is not a host name: labels of letters, digits and `-` \
             separated by dots, lowercase as a URL parser writes them"
        )));
    }
    if named.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(uf_infra::into_string(uf_infra::cstr!(
            "hostname {hostname:?} has capitals, and a URL's host never does once parsed, \
             so it would match nothing"
        )));
    }
    if let Some(port) = pattern.port.as_deref()
        && (port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(uf_infra::into_string(uf_infra::cstr!(
            "port {port:?} is not a port number"
        )));
    }
    if let Some(pathname) = pattern.pathname.as_deref() {
        if !pathname.starts_with('/') {
            return Err(uf_infra::into_string(uf_infra::cstr!(
                "pathname {pathname:?} does not start with `/`, and every URL's path does"
            )));
        }
        if pathname
            .split('/')
            .any(|segment| segment.contains('*') && segment != "*" && segment != "**")
        {
            return Err(uf_infra::into_string(uf_infra::cstr!(
                "pathname {pathname:?} puts a wildcard inside a segment; a wildcard is a whole \
                 segment, `*` for one and `**` for any number"
            )));
        }
    }
    Ok(())
}

/// What a project declares about its fonts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct FontsConfig {
    /// Whether imported fonts are self-hosted and declared.
    pub enabled: bool,
    /// The `font-display` every generated `@font-face` carries.
    ///
    /// `swap` by default: text the reader can read immediately, with the
    /// reflow that usually costs removed by the metric-matched fallback rather
    /// than by making them wait for it.
    pub display: String,
    /// The local face a metric-matched fallback is scaled from.
    ///
    /// One of `crates/uf_assets::font::LOCAL_FACES`. A project whose font is a
    /// serif should say so here, because scaling Arial to match a serif matches
    /// the line box and not the texture.
    pub fallback: String,
    /// How imported fonts are cut down: `"none"` or `"ranges"`.
    ///
    /// `"none"` by default, and the default is the argued position rather than
    /// caution. Subsetting is lossy — a glyph that was removed is a character
    /// the page can no longer draw — and a build cannot see the text a server
    /// will render, a user will type, or a translation will introduce. A
    /// toolchain that silently cut fonts down to the strings in the source
    /// would produce pages that render correctly for the developer and in
    /// tofu for somebody with a diacritic in their name.
    ///
    /// `"ranges"` is the mode that is safe to turn on: it splits the font's
    /// own coverage into script buckets with exact `unicode-range` values,
    /// loses nothing, and defers the part of the download a page does not
    /// need. See `crates/uf_assets/src/subset.rs`.
    pub subset: String,
    /// Whether an imported font's primary face is preloaded.
    ///
    /// On by default and worth being able to turn off. A preload is right for
    /// the face that paints the first screen and wrong for one that does not:
    /// it competes with the document and the stylesheet for the same
    /// connection, and a page with four preloaded faces has moved its own CSS
    /// down the queue. With `subset: "ranges"` exactly one bucket is
    /// preloaded whatever this says, because preloading all of them is the one
    /// thing that would undo the split.
    pub preload: bool,
}

impl Default for FontsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            display: String::from("swap"),
            fallback: String::from("Arial"),
            subset: String::from("none"),
            preload: true,
        }
    }
}

/// What a project declares about its icons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct IconsConfig {
    /// Whether `uf:icon/…` imports resolve at all.
    pub enabled: bool,
    /// Where the SVG files live, relative to the project root.
    ///
    /// A directory rather than an extension claim. `docs/red-lines.md` leaves
    /// `.svg` to Vite and to `vite-plugin-svgr`, and taking it would make a
    /// documented Vite feature unreachable; a named directory reached through
    /// uf's own `uf:` namespace takes nothing away.
    pub dir: String,
}

impl Default for IconsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dir: String::from("icons"),
        }
    }
}

/// What a project declares about its Open Graph cards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct OgConfig {
    /// Whether `*.og.json` imports are drawn.
    pub enabled: bool,
    /// A font every template inherits when it names none.
    ///
    /// Relative to the project root. uf embeds no typeface of its own, so a
    /// project that draws cards has to point at one; putting it here means the
    /// templates themselves stay text and colour.
    pub font: Option<String>,
}

impl Default for OgConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            font: None,
        }
    }
}
