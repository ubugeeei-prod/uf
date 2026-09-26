//! Prerendered documents served under a policy that names a per-request nonce.
//!
//! A `{uf.nonce}` in an `app.router.headers` value is the short way to set a
//! strict policy: the rule names the token, every front door substitutes this
//! request's nonce into it, and uf writes the same value onto every `<script>`
//! it renders. Header and markup are then one value read twice, which is the
//! whole point of the feature.
//!
//! A prerendered route is the case that cannot hold up its end. Its document
//! was written by a build, not by a request, so there was no nonce to mint and
//! none is in the file — not a stale one, **none**. The header still carries a
//! freshly minted nonce, because `{uf.nonce}` applies to every response a front
//! door answers and no door can tell a file from a render. The browser then
//! refuses uf's own client entry, by name, and the page never hydrates:
//!
//! ```text
//! Loading the script '/assets/client-DXtoNFND.js' violates the following
//! Content Security Policy directive: "script-src 'nonce-U4pihJdcCsDJ4PAyFRoy6A=='"
//! ```
//!
//! A server-rendered route on the same deployment is fine, so the failure is
//! per-route and invisible until somebody loads the wrong page. It does not
//! fail the build, the tests, or CI. That combination — correct-looking,
//! unobserved, and only in production — is what this check exists to say out
//! loud. See ubugeeei-prod/uf#1171, and #1169 for the browser evidence.
//!
//! # Why this reports instead of refusing
//!
//! The level is [`REFUSES_THE_BUILD`], one constant read in one place, so the
//! decision below is a line rather than a refactor.
//!
//! It reports today because that is what the repository owner asked for when
//! they first described this in #1169, and because a build can produce
//! something this check cannot see the deployment of: a project may serve
//! `dist/` from a host that never applies `app.router.headers` at all, in which
//! case the rule describes one deployment and the documents are for another.
//! That is the same missing fact [`super::guards`] argues about at length, and
//! the same answer.
//!
//! What is *not* the same is the strength of the claim. A guard that does not
//! run is a risk that depends on where `dist/` is served from; this is a page
//! that does not work anywhere the rule is applied, and the reason is
//! categorical rather than probabilistic — a value minted per request cannot
//! match a file that contains no value at all, so there is no deployment that
//! honours both halves and has a working page. If that argument wins, this
//! constant is where to act on it.
//!
//! # Why it is driven from the pages rather than the route table
//!
//! The same reason [`super::guards`] is: the route table is what *could* be
//! prerendered and the pages are what shipped, including every document a
//! `generateStaticParams` produced. It costs this check the house preference
//! for refusing before the bundle — `vite.pages` does not exist until Vite has
//! run — and buys a report that names files which are really on disk.

use uf_config::HeaderRule;

use super::Prerendered;
use crate::support::plural;

/// The token an `app.router.headers` value writes where the nonce goes.
///
/// `packages/server/internal/routing.js`'s `NONCE_TOKEN` is the source of truth
/// and this is a copy of it, the way `uf_router::Route::specificity` copies the
/// router's own numbers. The two have to spell it the same, or this check
/// reports a policy the server does not write — or misses the one it does.
const NONCE_TOKEN: &str = "{uf.nonce}";

/// How many rows the message names before the rest are counted.
const SHOWN: usize = 10;

/// Whether this combination refuses the build or is reported in its summary.
///
/// **This is the level, and it is deliberately one constant read in one
/// place** — `build()`'s call to [`message`] — so that moving between a refusal
/// and a report is this line and nothing else. Every other part of the check is
/// the same either way: the same join, the same rows, the same sentence.
///
/// `false` reports. The argument for each is in this module's header; the short
/// version is that a warning is missable and this failure is invisible until
/// production, while a refusal stops a build a project may have meant, and only
/// the person deploying knows which. Set it to `true` to refuse.
pub(crate) const REFUSES_THE_BUILD: bool = false;

/// One prerendered document, and the nonce rule written over it.
///
/// Ordered by URL first, so two builds of one tree produce rows in an order a
/// person can diff.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NoncedPage {
    /// The URL the prerender wrote a file for.
    pub(crate) url: String,
    /// The file it wrote, relative to the project root.
    pub(crate) file: String,
    /// The `source` of the rule that covers it.
    pub(crate) source: String,
    /// The header whose value names the token.
    pub(crate) header: String,
}

/// Every prerendered document covered by a rule whose value names the nonce.
///
/// The join the build already has both sides of: the documents Vite reported
/// writing, and `app.router.headers` as `uf_config` parsed it.
///
/// Every matching rule is reported rather than the first, because `headersFor`
/// applies every matching rule in order rather than stopping at one. A document
/// can be covered twice, and a report naming one rule would send somebody to
/// scope it and leave the other in place.
pub(crate) fn nonced_pages(pages: &[Prerendered], headers: &[HeaderRule]) -> Vec<NoncedPage> {
    let mut found = Vec::new();
    for rule in headers {
        // A rule that names no nonce costs one `contains` per value, so a
        // project that has never heard of this pays nothing — the same bargain
        // `withNonce` strikes at request time.
        let names_the_token: Vec<&str> = rule
            .headers
            .iter()
            .filter(|(_, value)| value.contains(NONCE_TOKEN))
            .map(|(name, _)| name.as_str())
            .collect();
        if names_the_token.is_empty() {
            continue;
        }
        for page in pages {
            if !source_matches(&rule.source, &page.url) {
                continue;
            }
            for header in &names_the_token {
                found.push(NoncedPage {
                    url: page.url.clone(),
                    file: page.file.clone(),
                    source: rule.source.to_string(),
                    header: (*header).to_owned(),
                });
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// Whether a rule's `source` covers `url`.
///
/// A port of `matchSegments` in `packages/server/internal/routing.js`, which is
/// the function that decides this for a real request.
///
/// It is **not** `uf_router::Route::matches_url`, and the difference is the
/// whole check: a route's catch-all requires at least one segment to consume,
/// so `/docs` is not `/docs/[...slug]`, while a rule's catch-all takes the rest
/// of the path *including none of it*. `source: "/:path*"` is the canonical way
/// to write a policy over a whole site, and under the route's reading it would
/// not cover `/` — the exact document #1171 was reported against. Matching a
/// rule with the route's rule would have left this check silent on the bug it
/// exists to find.
fn source_matches(source: &str, url: &str) -> bool {
    let path = url.split('?').next().unwrap_or(url);
    let mut given = path.split('/').filter(|part| !part.is_empty());
    for segment in source.split('/').filter(|part| !part.is_empty()) {
        // `uf_config` has already refused a `:name*` that is not last, so the
        // rest of the path — including none of it — belongs to this one.
        if segment.starts_with(':') && segment.ends_with('*') {
            return true;
        }
        let Some(part) = given.next() else {
            return false;
        };
        if !segment.starts_with(':') && segment != part {
            return false;
        }
    }
    given.next().is_none()
}

/// What the build says about them, as a refusal or as a warning.
///
/// One sentence for both levels: which documents, which rule covers each one,
/// and the two ways out that `docs/security.md` already names. The rule is
/// quoted by its `source` and the file it is written in, because that is what a
/// reader searches for.
pub(crate) fn message(findings: &[NoncedPage], config_file: &str) -> String {
    let mut rows: Vec<String> = findings
        .iter()
        .take(SHOWN)
        .map(|page| {
            uf_infra::into_string(uf_infra::cstr!(
                "  {} ({}) — `{}` from `{}` in {}",
                page.url,
                page.file,
                page.header,
                page.source,
                config_file
            ))
        })
        .collect();
    if findings.len() > SHOWN {
        rows.push(uf_infra::into_string(uf_infra::cstr!(
            "  … and {} more",
            findings.len() - SHOWN
        )));
    }
    uf_infra::into_string(uf_infra::cstr!(
        "{} written without a nonce and served under an `app.router.headers` rule whose value \
         names `{NONCE_TOKEN}`\n{}\n\n\
         A nonce is minted per request and a prerendered document has no request, so the file \
         carries no nonce at all — not a stale one, none — and the policy refuses uf's own client \
         entry: the page loads and never hydrates. A server-rendered route under the same rule is \
         fine, which is why nothing before production says so. Scope the rule's `source` to the \
         routes a server renders, or serve the prerendered ones under a hash policy rather than a \
         nonce.",
        plural(findings.len(), "prerendered document"),
        rows.join("\n"),
    ))
}

#[cfg(test)]
mod tests;
