#!/bin/sh
# Every link in the built site resolves to something the same build wrote.
#
# The manual has hundreds of internal links and a growing number of
# `/guide/…` cross-references, and a page that moves takes every link to its
# old name with it. `/guide/ci` was added and nothing anywhere would have said
# a word if the three pages pointing at it had said `/guide/pipeline`: the
# build prerenders each page independently, so a dead `<a href>` is a document
# that renders perfectly and 404s when somebody clicks it.
#
# This reads what `uf build#docs` wrote rather than the sources, and that is
# the whole point of it. Three defects only exist in the output:
#
#   * an anchor. `/reference/config#site` is correct only if the heading the
#     markdown pipeline rendered has that id, which is a property of the
#     renderer's slugger and not of the source text.
#   * a moved page. The build writes `sitemap.xml` from the documents it
#     prerendered, so a `<loc>` for a route nothing serves — or a page that
#     was prerendered and is in no `<loc>` — is a disagreement between the
#     build and itself.
#   * a link that no source file contains. The layout, the sidebar and the
#     "next page" footer put links on every page; those are code, and what
#     they produce is markup.
#
# ## What this is not
#
# `tests/library/docs-nav.test.js` checks the *source* list in
# `docs/app/_design/nav.js`: that it lists each page once, that "next page"
# walks every page and stops, and that each entry names a page on disk. It is
# about one file, before anything is rendered, and it caught a duplicate
# `/guide/state` that made the reading order a cycle (#586).
#
# This check never reads that list. It reads markup, and it cannot tell a
# sidebar link from a prose one. The two overlap on exactly one question — does
# `/guide/x` exist — and they answer it about different artifacts: the nav test
# about `docs/app/guide/x/_uf.page.mdx`, this about whether the build wrote
# something at `/guide/x`. A page can exist and not be built (it is not in the
# route table), and a route can be served and have no page (an asset, a
# redirect). Neither check subsumes the other, and neither is worth deleting
# for the other.
#
# ## The network
#
# By default nothing here touches it. Internal links, anchors and assets are
# decided by reading the output directory, so this check has exactly one
# answer for a given `dist/` and is safe to block a pull request on.
#
# External links are `--external`, which is a separate task
# (`uf run docs:links:external`) that CI runs on a schedule rather than on a
# pull request. A link checker that fails because a documentation host was
# slow teaches people to ignore failures, and the failure it teaches them to
# ignore is the one that matters. Even under `--external` only a definite,
# reproducible answer fails: `404`, `410` and `451` are dead, and a timeout, a
# connection error, a `429` or a `5xx` is reported as unverified and exits 0,
# because none of those is a statement about the link.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

# What `docs/uf.config.js` builds into, and the sources it built from. Both are
# arguments so that the check can be pointed at a planted site, which is how
# `test-links-resolve.sh` exercises it.
site="docs/dist/docs"
source_root="docs/app"
external=0

usage() {
  cat <<'USAGE'
usage: links-resolve.sh [--site DIR] [--source DIR] [--external]

  --site DIR     the built site to read (default: docs/dist/docs)
  --source DIR   the route sources, used to name the file a bad link is
                 written in (default: docs/app)
  --external     also resolve http(s) links over the network. Off by default:
                 a check that fails on somebody else's slow host teaches
                 people to ignore failures.
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --external) external=1 ;;
    --site) shift; site="${1:-}" ;;
    --site=*) site="${1#--site=}" ;;
    --source) shift; source_root="${1:-}" ;;
    --source=*) source_root="${1#--source=}" ;;
    -h | --help) usage; exit 0 ;;
    *)
      printf 'links-resolve: unknown argument `%s`\n\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if [ ! -d "$site" ]; then
  printf 'links-resolve: `%s` does not exist.\n' "$site" >&2
  printf 'This reads what `uf build` wrote, not the sources. Run `uf run docs:build`\n' >&2
  printf 'first, or pass `--site DIR` for a site built somewhere else.\n' >&2
  exit 1
fi

# `node -` rather than awk: this resolves relative URLs, decodes percent
# escapes and reads markup, and every one of those is a place where a
# hand-rolled parser is wrong in a way nobody notices. The `Docs build` job has
# node because the site is built with Vite.
UF_LINKS_SITE="$site" \
UF_LINKS_SOURCE="$source_root" \
UF_LINKS_EXTERNAL="$external" \
node - <<'NODE'
"use strict";

const fs = require("node:fs");
const path = require("node:path");

const site = process.env.UF_LINKS_SITE;
const sourceRoot = process.env.UF_LINKS_SOURCE;
const external = process.env.UF_LINKS_EXTERNAL === "1";

const findings = [];
const notes = [];

/** Every file under `dir`, as site-absolute URL paths (`/guide/ci/index.html`). */
function walk(dir, into) {
  for (const item of fs.readdirSync(dir, { withFileTypes: true })) {
    const child = path.join(dir, item.name);
    if (item.isDirectory()) {
      walk(child, into);
    } else if (item.isFile()) {
      into.push("/" + path.relative(site, child).split(path.sep).join("/"));
    }
  }
  return into;
}

const files = walk(site, []).sort();

// What a request for a path would be answered with, which is the only
// definition of "this link works" a static host has. A directory holding an
// `index.html` answers both spellings of its route; every file answers its own
// path.
const served = new Map();
for (const file of files) {
  served.set(file, file);
  if (file === "/index.html") {
    served.set("/", file);
  } else if (file.endsWith("/index.html")) {
    const withSlash = file.slice(0, -"index.html".length);
    served.set(withSlash, file);
    served.set(withSlash.slice(0, -1), file);
  }
}

const documents = files.filter((file) => file.endsWith(".html"));
const markup = new Map(
  documents.map((file) => [file, fs.readFileSync(path.join(site, file.slice(1)), "utf8")]),
);

// The ids a document actually rendered. An anchor is checked against these and
// not against the heading text, because the id is the slugger's output and the
// slugger is the thing that can change under the link.
const anchors = new Map();
for (const [file, body] of markup) {
  const ids = new Set();
  for (const match of body.matchAll(/\s(?:id|name)\s*=\s*(?:"([^"]*)"|'([^']*)')/g)) {
    ids.add(match[1] ?? match[2]);
  }
  anchors.set(file, ids);
}

function decodeEntities(text) {
  return text
    .replace(/&#x27;/gi, "'")
    .replace(/&#39;/g, "'")
    .replace(/&quot;/gi, '"')
    .replace(/&lt;/gi, "<")
    .replace(/&gt;/gi, ">")
    .replace(/&amp;/gi, "&");
}

// Every `href` and `src` in the markup, with the line it is on. Attributes
// rather than a parse tree: the output is generated, its quoting is uniform,
// and a tolerant scan over it has no opinion about which element the attribute
// belongs to — which is what we want, because a stylesheet's `href` and a
// prose link's are the same question.
const links = [];
for (const [file, body] of markup) {
  body.split("\n").forEach((line, index) => {
    for (const match of line.matchAll(/\s(?:href|src)\s*=\s*(?:"([^"]*)"|'([^']*)')/g)) {
      links.push({ file, line: index + 1, href: decodeEntities(match[1] ?? match[2]).trim() });
    }
  });
}

/** The source file behind a built document, when there is one. */
function pageSource(file) {
  const route = file === "/index.html" ? "" : file.replace(/\/index\.html$/, "").slice(1);
  for (const name of ["_uf.page.mdx", "_uf.page.js"]) {
    const candidate = path.join(sourceRoot, route, name);
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }
  return null;
}

// The modules that put a link on every page: the layout, the not-found page,
// and the design system the sidebar and the "next page" footer are built from.
// A dead link in one of these is reported against every document that carries
// it, and naming the module is the difference between one fix and thirty
// identical ones.
const sharedSources = [];
for (const name of ["_uf.layout.js", "_uf.not-found.js", "_uf.page.js"]) {
  const candidate = path.join(sourceRoot, name);
  if (fs.existsSync(candidate)) {
    sharedSources.push(candidate);
  }
}
const designDir = path.join(sourceRoot, "_design");
if (fs.existsSync(designDir)) {
  for (const name of fs.readdirSync(designDir).sort()) {
    if (name.endsWith(".js")) {
      sharedSources.push(path.join(designDir, name));
    }
  }
}

/**
 * Is this route behind a `_uf.middleware.js`?
 *
 * `uf build` leaves a guarded page out of `sitemap.xml` deliberately — a guard
 * says the route is not for everyone and a sitemap is a submission to search
 * engines, and the two cannot both be honoured. The check below asks the same
 * question of the same sources, so that adding a guard does not turn this
 * check red for the guard doing its job.
 */
function guarded(route) {
  const segments = route.split("/").filter((segment) => segment !== "");
  for (let depth = segments.length; depth >= 0; depth -= 1) {
    const dir = path.join(sourceRoot, ...segments.slice(0, depth));
    if (fs.existsSync(path.join(dir, "_uf.middleware.js"))) {
      return true;
    }
  }
  return false;
}

const sourceCache = new Map();
function sourceLines(file) {
  if (!sourceCache.has(file)) {
    sourceCache.set(file, fs.readFileSync(file, "utf8").split("\n"));
  }
  return sourceCache.get(file);
}

/**
 * Where a link is written, as `file:line`, best effort.
 *
 * The built document is the truth and is always reported; this adds the place
 * a person edits. A link that appears in no source at all is a link the layout
 * computed — from `nav.js`'s list, say — and saying "in the markup only" is
 * more honest than guessing.
 */
function writtenAt(link) {
  const candidates = [];
  const page = pageSource(link.file);
  if (page != null) {
    candidates.push(page);
  }
  candidates.push(...sharedSources);
  const places = [];
  for (const candidate of candidates) {
    sourceLines(candidate).forEach((line, index) => {
      if (line.includes(link.href)) {
        places.push(`${candidate}:${index + 1}`);
      }
    });
    if (places.length > 0) {
      break;
    }
  }
  return places;
}

function report(kind, link, detail) {
  findings.push({ kind, link, detail, places: writtenAt(link) });
}

const externalTargets = new Map();
let internal = 0;
let anchorsChecked = 0;

for (const link of links) {
  const href = link.href;
  if (href === "" || href === "#") {
    continue;
  }
  // A scheme uf does not serve is not uf's to resolve. `//host/path` is a
  // URL whose scheme the page inherits, so it is external too.
  if (href.startsWith("//")) {
    const url = "https:" + href;
    externalTargets.set(url, [...(externalTargets.get(url) ?? []), link]);
    continue;
  }
  if (/^[a-z][a-z0-9+.-]*:/i.test(href)) {
    if (/^https?:/i.test(href)) {
      externalTargets.set(href, [...(externalTargets.get(href) ?? []), link]);
    }
    continue;
  }

  const hash = href.indexOf("#");
  const fragment = hash === -1 ? "" : href.slice(hash + 1);
  let target = hash === -1 ? href : href.slice(0, hash);
  const query = target.indexOf("?");
  if (query !== -1) {
    target = target.slice(0, query);
  }

  let document = link.file;
  if (target !== "") {
    internal += 1;
    const absolute = target.startsWith("/")
      ? path.posix.normalize(target)
      : path.posix.normalize(path.posix.join(path.posix.dirname(link.file), target));
    let answered = served.get(absolute);
    if (answered == null) {
      let decoded = absolute;
      try {
        decoded = decodeURIComponent(absolute);
      } catch {
        // A malformed escape is not a route either; fall through with the raw
        // text so the message quotes what is written.
      }
      answered = served.get(decoded);
    }
    if (answered == null) {
      report("no route", link, `nothing the build wrote answers \`${absolute}\``);
      continue;
    }
    document = answered;
  }

  if (fragment === "") {
    continue;
  }
  const ids = anchors.get(document);
  if (ids == null) {
    // The target is a file rather than a document — a PDF, an image. It has
    // no ids and the fragment means whatever its viewer says it means.
    continue;
  }
  let name = fragment;
  try {
    name = decodeURIComponent(fragment);
  } catch {
    // Percent escapes that do not decode are not an id either.
  }
  anchorsChecked += 1;
  if (!ids.has(name) && !ids.has(fragment)) {
    report("no anchor", link, `\`${document}\` renders no id \`${name}\``);
  }
}

// The sitemap the build wrote, against the documents the same build wrote.
// This is where a moved page shows: `sitemap.xml` is generated from what was
// prerendered, so a `<loc>` nothing answers means the two halves of one build
// disagree, and a prerendered page with no `<loc>` is a page a crawler is
// never handed.
const sitemapFile = path.join(site, "sitemap.xml");
if (fs.existsSync(sitemapFile)) {
  const xml = fs.readFileSync(sitemapFile, "utf8");
  const listed = new Set();
  const locations = [...xml.matchAll(/<loc>([^<]*)<\/loc>/g)].map((match) =>
    decodeEntities(match[1].trim()),
  );
  const sitemapLink = { file: "/sitemap.xml", line: 1, href: "" };
  for (const location of locations) {
    let route;
    try {
      route = new URL(location).pathname;
    } catch {
      findings.push({
        kind: "sitemap",
        link: sitemapLink,
        detail: `\`${location}\` is not an absolute URL, and a <loc> has to be one`,
        places: [],
      });
      continue;
    }
    const canonical = route.length > 1 ? route.replace(/\/$/, "") : route;
    listed.add(canonical);
    if (!served.has(route)) {
      findings.push({
        kind: "sitemap",
        link: sitemapLink,
        detail: `<loc> ${location} — nothing the build wrote answers \`${route}\``,
        places: [],
      });
    }
  }
  for (const document of documents) {
    if (document !== "/index.html" && !document.endsWith("/index.html")) {
      // Only prerendered routes are sitemap material. `404.html` is served for
      // a path that has no route, and a crawler handed it as a URL would index
      // the error page.
      continue;
    }
    const route = document === "/index.html" ? "/" : document.slice(0, -"/index.html".length);
    if (guarded(route)) {
      // The build leaves a guarded page out of the sitemap on purpose: a
      // `_uf.middleware.js` says this route is not for everyone, and a sitemap
      // is a submission to search engines. This is the same rule, so that a
      // guard added later does not fail this check for doing its job.
      continue;
    }
    if (!listed.has(route)) {
      findings.push({
        kind: "sitemap",
        link: { file: document, line: 1, href: "" },
        detail: `prerendered and in no <loc> — a crawler handed sitemap.xml never reaches it`,
        places: [],
      });
    }
  }
} else {
  notes.push("no sitemap.xml in the output, so nothing was checked against it");
}

console.log(`links-resolve: ${site}`);
console.log(
  `  ${documents.length} documents, ${links.length} links, ` +
    `${internal} internal, ${anchorsChecked} anchors, ${externalTargets.size} external`,
);
for (const note of notes) {
  console.log(`  note       ${note}`);
}

async function resolveExternal() {
  const targets = [...externalTargets.keys()].sort();
  if (targets.length === 0) {
    return;
  }
  console.log(`  resolving  ${targets.length} external links over the network`);
  let dead = 0;
  let unverified = 0;

  // A `HEAD` first because it costs the host nothing, and a `GET` after it
  // because a surprising number of hosts answer `HEAD` with 403 or 405 and
  // the request method is not what is being checked.
  const visit = async (url) => {
    for (const method of ["HEAD", "GET"]) {
      try {
        const response = await fetch(url, {
          method,
          redirect: "follow",
          signal: AbortSignal.timeout(20000),
          headers: { "user-agent": "uf-links-resolve (+https://github.com/ubugeeei-prod/uf)" },
        });
        // Only the status is wanted, and an unread body holds its socket open
        // — enough of them and the process does not exit at all.
        await response.body?.cancel().catch(() => {});
        if (response.status === 404 || response.status === 410 || response.status === 451) {
          return { state: "dead", detail: `HTTP ${response.status}` };
        }
        if (response.ok || response.status < 400) {
          return { state: "ok", detail: `HTTP ${response.status}` };
        }
        if (method === "GET") {
          return { state: "unverified", detail: `HTTP ${response.status}` };
        }
      } catch (error) {
        if (method === "GET") {
          return { state: "unverified", detail: String(error.message ?? error) };
        }
      }
    }
    return { state: "unverified", detail: "no answer" };
  };

  // Eight at a time. A crawl that opens every link at once is a crawl that
  // gets rate limited, and a rate limit is exactly the answer this pass has
  // to treat as no answer at all.
  const queue = targets.slice();
  const workers = Array.from({ length: Math.min(8, queue.length) }, async () => {
    for (let url = queue.shift(); url != null; url = queue.shift()) {
      const result = await visit(url);
      const where = externalTargets.get(url)[0];
      if (result.state === "dead") {
        dead += 1;
        console.log(`  dead       ${url}  (${result.detail})`);
        console.log(`             in ${where.file}:${where.line}`);
        findings.push({
          kind: "dead link",
          link: where,
          detail: `${url} — ${result.detail}`,
          places: writtenAt(where),
        });
      } else if (result.state === "unverified") {
        unverified += 1;
        console.log(`  unverified ${url}  (${result.detail})`);
      }
    }
  });
  await Promise.all(workers);
  console.log(`  ${targets.length - dead - unverified} reachable, ${unverified} unverified`);
  if (unverified > 0) {
    console.log(
      "  unverified is not a failure: a timeout, a 429 and a 5xx are statements\n" +
        "  about a host, not about a link.",
    );
  }
}

function finish() {
  if (findings.length === 0) {
    console.log("links-resolve: ok");
    return;
  }
  console.error("");
  for (const finding of findings) {
    const href = finding.link.href === "" ? "" : `  ${finding.link.href}`;
    console.error(`  ${finding.kind.padEnd(10)}${href}`);
    console.error(`             ${finding.detail}`);
    for (const place of finding.places) {
      console.error(`             written at ${place}`);
    }
    console.error(`             in ${finding.link.file}:${finding.link.line}`);
  }
  console.error(
    `\n${findings.length} link(s) in the built site go nowhere. Each is a page that\n` +
      "renders perfectly and 404s when a reader clicks it. Fix the link, or the\n" +
      "route, and run `uf run docs:build && uf run docs:links` to see it again.",
  );
  process.exitCode = 1;
}

if (external) {
  resolveExternal().then(finish);
} else {
  finish();
}
NODE
