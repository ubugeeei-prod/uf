//! `sitemap.xml` and `robots.txt`: what a build can tell a crawler and mean.
//!
//! A sitemap is a list of URLs, and `uf build` finishes holding exactly that:
//! the prerender reports every document it wrote, and the route table says
//! which route each one belongs to. So the file is nearly free. What is not
//! free is being *right* — these two are read by machines that do not forgive,
//! and a `<loc>` naming a page that 404s, or a page nobody may see, is worse
//! for a site than no sitemap at all. The rules below are therefore all
//! subtractive, and each one is a fact the build actually has.
//!
//! # Which URLs go in
//!
//! **Documents the prerender wrote, and only those.** Not the route table: a
//! route is a pattern, and `/posts/:slug` is not a URL. A parameterised route
//! is in the sitemap exactly when its `generateStaticParams` enumerated it,
//! because that is the only circumstance under which anyone — this build
//! included — knows what its URLs are. One with no `generateStaticParams`
//! prerenders nothing and so appears nowhere here, which is the right answer
//! arrived at by construction rather than by a special case.
//!
//! Minus two subtractions:
//!
//! * **anything the prerender did not answer `200` for.** `404.html` is
//!   written from the router-root not-found boundary and reported as `/404`;
//!   it is a document, it is served, and it is not a page. Submitting it would
//!   be asking a crawler to index the error page.
//! * **anything behind a `_uf.middleware.js`.** A guard is a statement that
//!   this route is not for everyone, and a sitemap is a submission to search
//!   engines — the two cannot both be honoured. [`super::guards`] already
//!   computes which prerendered documents sit under a guard, for its own
//!   warning, and this is the second reader of that answer.
//!
//! # What is deliberately not in a `<url>`
//!
//! `lastmod`, `changefreq` and `priority` are all optional and all absent.
//!
//! `lastmod` is a claim about when the *content* changed, and this build has
//! no fact that supports one. A source file's modification time is the date of
//! the checkout on any CI machine; the build's own clock is the same instant
//! for every URL, which tells a crawler that the entire site changed every
//! time anyone deployed, and a `lastmod` that behaves like that is one Google
//! documents it will start ignoring. `changefreq` and `priority` are ignored
//! outright — priority in particular is only ever relative *within* one site,
//! so a generator that emitted the same number for every page would be
//! emitting no information at all.
//!
//! A list of `<loc>`s is smaller than the sitemaps most frameworks generate,
//! and it is the whole of what this build knows to be true.
//!
//! # Why `robots.txt` is not always written
//!
//! A `robots.txt` that disallows nothing is exactly equivalent to having no
//! `robots.txt`, so writing one would be adding a request to every crawl for
//! no answer. What makes it worth writing is the `Sitemap:` line — the only
//! way a crawler that was not handed the sitemap finds it — or a rule the
//! project asked for. With neither, no file.
//!
//! Guarded routes are excluded from the sitemap and are *not* added to
//! `robots.txt`. `Disallow: /admin` publishes `/admin` to everyone who fetches
//! the file, and a path a project did not want advertised is not made safer by
//! being written down in the one file every crawler and every scanner reads
//! first. Leaving it out of the sitemap is silence; naming it is the opposite.

use std::fmt::Write as _;
use std::fs;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_config::{RobotsConfig, SiteConfig};

use super::Prerendered;
use super::guards::UnguardedPage;

/// How many URLs one sitemap file may name, per the sitemaps.org protocol.
///
/// A file over the limit is rejected whole rather than truncated by the
/// consumer, so a build that produced one would have shipped nothing while
/// appearing to ship everything. Past this a sitemap index is required, which
/// uf does not write; the build says so rather than emitting a file that will
/// be thrown away.
const MAX_SITEMAP_URLS: usize = 50_000;

/// The site's origin, parsed once and known to be usable in a `<loc>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SiteUrl {
    /// Scheme, authority and any base path, with no trailing slash.
    base: String,
}

impl SiteUrl {
    /// Read `site.url`, or say why it cannot be one.
    ///
    /// Strict on purpose, and strict *here* rather than in `uf_config`: every
    /// command loads the configuration and only this one writes a URL into a
    /// file, so a typo should fail the build that would have published it and
    /// nothing else.
    pub(crate) fn parse(raw: &str) -> Result<Self> {
        let value = raw.trim();
        let rest = match value.split_once("://") {
            Some(("http" | "https", rest)) => rest,
            _ => bail!(
                "`site.url` must be an absolute http(s) URL, and {raw:?} is not; \
                 a sitemap's <loc> has to name the host the site is served from"
            ),
        };
        if rest.is_empty() || rest.starts_with('/') {
            bail!("`site.url` names no host: {raw:?}");
        }
        if value.contains(['?', '#']) {
            bail!("`site.url` must have no query and no fragment: {raw:?}");
        }
        if value.contains(char::is_whitespace) {
            bail!("`site.url` must have no whitespace in it: {raw:?}");
        }
        Ok(Self {
            base: value.trim_end_matches('/').to_owned(),
        })
    }

    /// The absolute URL of a path this build served, ready for a `<loc>`.
    ///
    /// The path arrives from the prerender already percent-encoded where a
    /// `generateStaticParams` value needed it, so [`encode_path`] preserves
    /// what is there rather than encoding the `%` again.
    pub(crate) fn join(&self, path: &str) -> String {
        let encoded = encode_path(path);
        if encoded == "/" {
            // The site root is `https://example.com/`, not
            // `https://example.com`: a `<loc>` is a URL and a URL with an
            // authority has a path.
            format!("{}/", self.base)
        } else {
            format!("{}{encoded}", self.base)
        }
    }
}

/// Which prerendered documents may be named in a sitemap, sorted and unique.
///
/// The two subtractions argued in the module documentation, in one pass.
pub(crate) fn indexable(pages: &[Prerendered], guarded: &[UnguardedPage]) -> Vec<String> {
    let mut urls: Vec<String> = pages
        .iter()
        .filter(|page| page.status == 200)
        .filter(|page| !guarded.iter().any(|hidden| hidden.url == page.url))
        .map(|page| page.url.clone())
        .collect();
    urls.sort_unstable();
    urls.dedup();
    urls
}

/// The `sitemap.xml` for these URLs, or the reason there cannot be one.
///
/// Sorted input is the caller's job and [`indexable`] does it: two builds of
/// the same sources must produce the same bytes, and the prerender's order is
/// whatever order the routes were walked in.
pub(crate) fn sitemap(site: &SiteUrl, urls: &[String]) -> Result<String> {
    if urls.len() > MAX_SITEMAP_URLS {
        bail!(
            "this build prerendered {} pages and one sitemap may name {MAX_SITEMAP_URLS}; \
             a site this size needs a sitemap index, which uf does not write yet — \
             set `site.sitemap: false` to keep the build",
            urls.len()
        );
    }

    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for url in urls {
        // `write!` into a String cannot fail; the `_ =` is the accepted way of
        // saying so without an `unwrap` that would read as a real fallibility.
        _ = writeln!(
            out,
            "  <url>\n    <loc>{}</loc>\n  </url>",
            escape_xml(&site.join(url))
        );
    }
    out.push_str("</urlset>\n");
    Ok(out)
}

/// The `robots.txt` for this configuration, or `None` if it would say nothing.
///
/// `sitemap` is the URL of the sitemap this build wrote, when it wrote one.
pub(crate) fn robots(
    site: &SiteUrl,
    config: &RobotsConfig,
    sitemap_written: bool,
) -> Result<Option<String>> {
    if !config.enabled {
        return Ok(None);
    }
    for path in config.allow.iter().chain(config.disallow.iter()) {
        if !path.starts_with('/') {
            bail!(
                "`site.robots` paths are matched against a request path and must start with `/`, \
                 and {path:?} does not"
            );
        }
        if path.contains(['\n', '\r']) {
            bail!("a `site.robots` path may not contain a line break: {path:?}");
        }
    }
    // The whole of the "is this worth writing" decision, in one condition.
    if !sitemap_written && config.allow.is_empty() && config.disallow.is_empty() {
        return Ok(None);
    }

    let mut out = String::from("User-agent: *\n");
    for path in &config.allow {
        _ = writeln!(out, "Allow: {path}");
    }
    for path in &config.disallow {
        _ = writeln!(out, "Disallow: {path}");
    }
    // A group needs at least one rule line to be a group. An empty `Disallow:`
    // is the protocol's way of spelling "nothing is disallowed", and it is the
    // line that makes a sitemap-only file well-formed rather than a bare
    // `Sitemap:` some validators reject.
    if config.allow.is_empty() && config.disallow.is_empty() {
        out.push_str("Disallow:\n");
    }
    if sitemap_written {
        _ = write!(out, "\nSitemap: {}\n", site.join("/sitemap.xml"));
    }
    Ok(Some(out))
}

/// Percent-encode what a URL path may not carry literally, and nothing else.
///
/// Bytes already written as a `%XX` triplet are passed through: the prerender
/// runs `encodeURIComponent` over every `generateStaticParams` value, so a
/// path can arrive encoded, and encoding the `%` a second time would produce a
/// `<loc>` that resolves to a different page than the one that was built.
fn encode_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = String::with_capacity(path.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%'
            && bytes
                .get(index + 1..index + 3)
                .is_some_and(|pair| pair.iter().all(u8::is_ascii_hexdigit))
        {
            out.push('%');
            out.push(char::from(bytes[index + 1]));
            out.push(char::from(bytes[index + 2]));
            index += 3;
            continue;
        }
        // Unreserved, plus the sub-delimiters and the two extra characters
        // RFC 3986 allows in a path segment. `%` is not among them, so a `%`
        // that did not begin a triplet above is encoded here.
        if byte.is_ascii_alphanumeric() || b"-._~/:@!$&'()*+,;=".contains(&byte) {
            out.push(char::from(byte));
        } else {
            _ = write!(out, "%{byte:02X}");
        }
        index += 1;
    }
    out
}

/// Escape the five characters the sitemap protocol requires escaped.
fn escape_xml(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&apos;"),
            '"' => out.push_str("&quot;"),
            '>' => out.push_str("&gt;"),
            '<' => out.push_str("&lt;"),
            _ => out.push(character),
        }
    }
    out
}

/// What [`write`] did.
#[derive(Debug, Default)]
pub(crate) struct Written {
    /// Files this build wrote into the output directory.
    pub(crate) files: Vec<Utf8PathBuf>,
    /// Files the project already had there, which this build left alone.
    pub(crate) kept: Vec<Utf8PathBuf>,
}

/// Write whatever this configuration and this build can honestly produce.
///
/// Into the output directory, before the size report measures it: these two
/// are fetched by crawlers over the same connection as everything else, so
/// they are shipped assets and the report should say so.
///
/// A file the project already put there is never overwritten. `public/` is
/// copied into the output directory by Vite, so a hand-written
/// `public/robots.txt` arrives here before this runs — and it is the more
/// specific statement of intent by a distance. The build says which files it
/// left alone rather than silently deferring.
pub(crate) fn write(
    out_dir: &Utf8Path,
    config: &SiteConfig,
    pages: &[Prerendered],
    guarded: &[UnguardedPage],
) -> Result<Written> {
    // The switch. Without a site URL there is no `<loc>` to write and no
    // `Sitemap:` line to point anywhere, so there is nothing to do — which is
    // also what every project that has not asked for any of this gets.
    let Some(raw) = config.url.as_deref() else {
        return Ok(Written::default());
    };
    let site = SiteUrl::parse(raw)?;
    let mut written = Written::default();

    let sitemap_path = out_dir.join("sitemap.xml");
    let mut sitemap_exists = sitemap_path.exists();
    if config.sitemap && !sitemap_exists {
        let urls = indexable(pages, guarded);
        // An empty `<urlset>` is well-formed and says nothing. An application
        // that prerendered no document — every route parameterised, or every
        // one behind a guard — has no sitemap to write, and submitting an
        // empty one is not the same as having one.
        if !urls.is_empty() {
            fs::write(&sitemap_path, sitemap(&site, &urls)?)
                .with_context(|| format!("failed to write {sitemap_path}"))?;
            written.files.push(sitemap_path);
            sitemap_exists = true;
        }
    } else if sitemap_exists && config.sitemap {
        written.kept.push(sitemap_path);
    }

    let robots_path = out_dir.join("robots.txt");
    if robots_path.exists() {
        if config.robots.enabled {
            written.kept.push(robots_path);
        }
    } else if let Some(text) = robots(&site, &config.robots, sitemap_exists)? {
        fs::write(&robots_path, text).with_context(|| format!("failed to write {robots_path}"))?;
        written.files.push(robots_path);
    }

    Ok(written)
}

#[cfg(test)]
mod tests;
