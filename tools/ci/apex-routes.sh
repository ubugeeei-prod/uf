#!/bin/sh
# What `uniflowed.dev` answers, checked by asking it.
#
# The apex worker used to be nine lines: every path 308'd to the documentation
# host, so there was one behaviour and nothing to get wrong. Now `/` is a page,
# `/robots.txt` is a file, `/brand/*` comes out of the repository's `brand/`
# directory, and everything else still redirects — and "everything else still
# redirects" is a promise about paths nobody in this repository can enumerate.
# Links to `uniflowed.dev/guide`, `uniflowed.dev/og.png` and
# `uniflowed.dev/sitemap.xml` are in issues, in other people's blog posts, and
# in the address bars of anyone who bookmarked one. A landing page that quietly
# turned those into 404s would look perfect on the screen of the person who
# added it.
#
# So this is the check, and — the same argument `tools/ci/docs-csp.sh` makes for
# response headers — it is deliberately not a grep. It imports
# `infra/cloudflare/workers/root.js`, calls its `fetch` with an `ASSETS` binding
# over `brand/`, and reads the answers. `security-scan.sh` said it first:
# *"asserting their behaviour by reading their source would be asserting the
# text of a file and calling it a server."*
#
# Five things it will not let go quiet.
#
# ## 1. The apex is a page
#
# `/` must answer `200 text/html` with the install command in it, a `<title>`,
# a `lang`, a viewport, a skip link and the element it skips to. The old worker
# failed every one of those, which is the point: this check is red on the
# commit before the one that added the page.
#
# ## 2. Every path that resolved still resolves
#
# A table of the paths that are actually written down somewhere — `/guide`,
# `/og.png`, `/brand/uf.png`, `/sitemap.xml`, a reference page, a query string,
# and a path nobody has ever used — and each must come back either as something
# the apex serves itself or as a 308 to the *same* path on the documentation
# host. A redirect that drops the query string or rewrites the path is a link
# that resolves to the wrong page, which is worse than one that 404s.
#
# The fallthrough is also checked with no `ASSETS` binding at all, because
# compatibility must not depend on a binding being configured: a worker deployed
# without one should still redirect `/brand/uf.png` rather than 404 it.
#
# ## 3. `www` reaches the apex, and stops
#
# One hop to the canonical origin, never to another `www` URL. A redirect loop
# on the front door is a site that is down.
#
# ## 4. The page loads only what it can serve
#
# Every `src` and `href` in the markup that is a *load* rather than a navigation
# must be same-origin, and must actually be answered `200` by this same worker.
# `img-src 'self'` is the whole policy for them, so an off-origin one is a
# broken image on a page whose own header forbids it — and a same-origin one
# naming a file that is not in `brand/` is a broken image with no error anywhere.
#
# ## 5. The policy is a policy about *this* page
#
# `style-src` names a sha256, and the digest is recomputed here from the
# `<style>` the page actually returned. A hash that has gone stale is a
# completely unstyled front door, which is why the worker derives it at runtime
# rather than carrying a literal — this asserts that it still does. The page
# must contain no `<script>` at all and the policy must say `script-src 'none'`,
# which together are the reason there is no inline-script exemption here of the
# kind the documentation site needs.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

worker="infra/cloudflare/workers/root.js"
brand="brand"

usage() {
  cat <<'USAGE'
usage: apex-routes.sh [--worker FILE] [--brand DIR]

  --worker FILE   the worker whose handler to drive
                  (default: infra/cloudflare/workers/root.js)
  --brand DIR     the directory its `ASSETS` binding answers from
                  (default: brand)
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --worker) shift; worker="${1:-}" ;;
    --worker=*) worker="${1#--worker=}" ;;
    --brand) shift; brand="${1:-}" ;;
    --brand=*) brand="${1#--brand=}" ;;
    -h | --help) usage; exit 0 ;;
    *)
      printf 'apex-routes: unknown argument `%s`\n\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if [ ! -f "$worker" ]; then
  printf 'apex-routes: no worker at `%s`\n' "$worker" >&2
  exit 1
fi

if [ ! -d "$brand" ]; then
  printf 'apex-routes: `%s` does not exist, and it is what the `ASSETS` binding\n' "$brand" >&2
  printf 'answers from. Pass `--brand DIR` for a directory somewhere else.\n' >&2
  exit 1
fi

UF_APEX_WORKER="$worker" UF_APEX_BRAND="$brand" node - <<'NODE'
"use strict";

const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { pathToFileURL } = require("node:url");

const workerFile = process.env.UF_APEX_WORKER;
const brand = process.env.UF_APEX_BRAND;

const APEX = "https://uniflowed.dev";
const WWW = "https://www.uniflowed.dev";
const DOCS = "https://docs.uniflowed.dev";

const findings = [];
function finding(where, message) {
  findings.push({ where, message });
}

/**
 * The `ASSETS` binding, over `brand/`.
 *
 * One line of Cloudflare's documentation is the whole contract: it answers a
 * `Request` with a `Response`, and `404` where there is no such asset. No
 * `html_handling` and no `not_found_handling` here, because the worker's
 * config asks for neither — it strips its own prefix and asks for one file.
 */
const TYPES = {
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".md": "text/markdown; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
};

function assetsBinding(root) {
  return {
    fetch: async (request) => {
      const { pathname } = new URL(request.url);
      const file = path.join(root, pathname);
      // A path that climbs out of the directory is not an asset, and the
      // platform would not answer it either.
      if (!path.resolve(file).startsWith(path.resolve(root) + path.sep)) {
        return new Response("not found", { status: 404 });
      }
      if (!fs.existsSync(file) || !fs.statSync(file).isFile()) {
        return new Response("not found", { status: 404 });
      }
      return new Response(fs.readFileSync(file), {
        headers: { "content-type": TYPES[path.extname(file)] ?? "application/octet-stream" },
      });
    },
  };
}

/** A directive's values, from a policy string. */
function directive(policy, name) {
  for (const part of policy.split(";")) {
    const words = part.trim().split(/\s+/).filter(Boolean);
    if (words[0] === name) {
      return words.slice(1);
    }
  }
  return null;
}

/**
 * Every URL the page *loads*, which is not every URL it names.
 *
 * An `<a href>` is a navigation and is left alone — the page links to the
 * documentation host on purpose, and `default-src` has no opinion about where
 * a person clicks to.
 */
function subresources(html) {
  const found = [];
  for (const match of html.matchAll(/<(script|link|img|source|iframe|embed|object)\b([^>]*)>/gi)) {
    const [, tag, attributes] = match;
    if (tag.toLowerCase() === "link" && /rel="(canonical|alternate|author)"/i.test(attributes)) {
      continue;
    }
    for (const attribute of ["src", "href", "data", "srcset"]) {
      const value = attributes.match(new RegExp(`\\s${attribute}="([^"]*)"`, "i"));
      if (value == null) continue;
      for (const candidate of value[1].split(",")) {
        const url = candidate.trim().split(/\s+/)[0];
        if (url !== "") found.push({ tag: tag.toLowerCase(), attribute, url });
      }
    }
  }
  return found;
}

async function main() {
  const module = await import(pathToFileURL(path.resolve(workerFile)).href);
  const handler = module.default;
  if (handler == null || typeof handler.fetch !== "function") {
    console.error(`apex-routes: \`${workerFile}\` has no default export with a \`fetch\``);
    process.exit(2);
  }

  const env = { ASSETS: assetsBinding(brand) };
  const ask = (url, init) => handler.fetch(new Request(url, init), env, { waitUntil: () => {} });
  const askBare = (url, init) =>
    handler.fetch(new Request(url, init), {}, { waitUntil: () => {} });

  // 1. The apex is a page.
  const home = await ask(`${APEX}/`);
  let html = "";
  if (home.status !== 200) {
    finding(
      `${workerFile} /`,
      `answered ${home.status} and the apex is supposed to be a page. A domain that ` +
        "bounces its own front door into the middle of the manual is the thing this " +
        "worker was changed to stop doing",
    );
  } else {
    const type = home.headers.get("content-type") ?? "";
    if (!type.startsWith("text/html")) {
      finding(`${workerFile} /`, `answered \`content-type: ${type}\`, and a page is text/html`);
    }
    html = await home.text();
    const required = [
      ["<html lang=", "a `lang` on the root element, which is what a screen reader reads it in"],
      ["<title>", "a `<title>`"],
      ['name="viewport"', "a viewport meta, without which it renders at desktop width on a phone"],
      ['class="skip"', "a skip link, which is the first thing a keyboard reaches"],
      ['id="content"', "the element the skip link skips to"],
      ["curl -fsSL https://setup.uniflowed.dev | sh", "the install command"],
      ["0.0.0-alpha", "the version, which is the one fact that decides whether to install it"],
      ["prefers-color-scheme: dark", "a dark scheme"],
    ];
    for (const [needle, what] of required) {
      if (!html.includes(needle)) {
        finding(`${workerFile} /`, `the page has no ${what} (looked for \`${needle}\`)`);
      }
    }
  }

  // A page is a GET. Anything else is answered rather than redirected onward.
  const posted = await ask(`${APEX}/`, { method: "POST" });
  if (posted.status !== 405 || posted.headers.get("allow") == null) {
    finding(
      `${workerFile} POST /`,
      `answered ${posted.status} with \`allow: ${posted.headers.get("allow")}\`, and a page ` +
        "that is only ever read should say so",
    );
  }

  // 2. Every path that resolved still resolves.
  //
  // `serve` is a path the apex answers itself; `docs` is one it hands on, and
  // the target has to be the same path on the documentation host — a redirect
  // that rewrites the path resolves to the wrong page.
  const ROUTES = [
    ["/robots.txt", "serve", "written here rather than borrowed from another host"],
    ["/brand/uf.png", "serve", "the source mark, out of `brand/`"],
    ["/brand/uniflowed-mark.svg", "serve", "the mark the page and the manual both use"],
    ["/brand/og.png", "serve", "the share card"],
    ["/brand/favicon.svg", "serve", "the icon"],
    ["/brand/tokens.css", "serve", "the design tokens, which examples link to"],
    ["/guide", "docs", "linked from the README and from issues"],
    ["/guide/install", "docs", "the install page"],
    ["/guide/scope", "docs", "what uf does not do"],
    ["/reference/cli", "docs", "the command reference"],
    ["/og.png", "docs", "an older share-card path"],
    ["/sitemap.xml", "docs", "the sitemap, which is a crawler's entry point"],
    ["/setup", "docs", "the documentation site's own redirect to the installer"],
    ["/brand/not-a-file.png", "docs", "a brand name that is not in the directory"],
    ["/nothing/here", "docs", "a path nobody has ever used, which is the default"],
    ["/guide?from=readme", "docs", "a path with a query string, which has to survive"],
  ];

  for (const [route, kind, why] of ROUTES) {
    const response = await ask(`${APEX}${route}`);
    if (kind === "serve") {
      if (response.status !== 200) {
        finding(
          `${workerFile} ${route}`,
          `answered ${response.status}, and this path is ${why}. It resolved before this ` +
            "worker served anything itself, so it has to resolve now",
        );
      }
      continue;
    }
    if (response.status !== 308) {
      finding(
        `${workerFile} ${route}`,
        `answered ${response.status} rather than a 308 to the documentation host, and ` +
          `this path is ${why}`,
      );
      continue;
    }
    const location = response.headers.get("location");
    const wanted = `${DOCS}${route}`;
    if (location !== wanted) {
      finding(
        `${workerFile} ${route}`,
        `redirects to \`${location}\` and the path it was asked for is \`${route}\`. ` +
          `Wanted \`${wanted}\` — a redirect that rewrites the path resolves to the wrong page`,
      );
    }
  }

  // And the same fallthrough with no binding configured at all. Compatibility
  // is not allowed to depend on a deployment detail.
  for (const route of ["/brand/uf.png", "/guide", "/sitemap.xml"]) {
    const response = await askBare(`${APEX}${route}`);
    if (response.status !== 308 || response.headers.get("location") !== `${DOCS}${route}`) {
      finding(
        `${workerFile} ${route} (no ASSETS binding)`,
        `answered ${response.status} → \`${response.headers.get("location")}\`. A worker ` +
          "deployed without its assets should still hand every path to the documentation " +
          "host, which is what it did before it had any",
      );
    }
  }

  // 3. `www` reaches the apex, and stops.
  for (const route of ["/", "/guide", "/brand/uf.png"]) {
    const response = await ask(`${WWW}${route}`);
    const location = response.headers.get("location");
    if (response.status < 300 || response.status >= 400 || location == null) {
      finding(
        `${workerFile} www${route}`,
        `answered ${response.status}. \`www\` is the apex one hop away, not a second copy ` +
          "of it under a second name",
      );
      continue;
    }
    if (location !== `${APEX}${route}`) {
      finding(
        `${workerFile} www${route}`,
        `redirects to \`${location}\`, wanted \`${APEX}${route}\``,
      );
    }
    if (new URL(location).hostname.startsWith("www.")) {
      finding(`${workerFile} www${route}`, `redirects to \`${location}\`, which is a loop`);
    }
  }

  // 4. The headers, on every branch there is.
  const REQUIRED = {
    "content-security-policy": null,
    "x-content-type-options": "nosniff",
    "referrer-policy": "strict-origin-when-cross-origin",
    "permissions-policy": "interest-cohort=()",
  };
  let policy = null;
  for (const url of [`${APEX}/`, `${APEX}/robots.txt`, `${APEX}/brand/uf.png`, `${APEX}/guide`, `${WWW}/`]) {
    const response = await ask(url);
    for (const [name, expected] of Object.entries(REQUIRED)) {
      const sent = response.headers.get(name);
      if (sent == null || sent === "") {
        finding(`${workerFile} ${url}`, `sent no \`${name}\``);
        continue;
      }
      if (expected != null && sent !== expected) {
        finding(`${workerFile} ${url}`, `sent \`${name}: ${sent}\`, wanted \`${expected}\``);
      }
    }
    const sent = response.headers.get("content-security-policy");
    if (sent != null && policy != null && sent !== policy) {
      finding(
        `${workerFile} ${url}`,
        "sent a different policy from the one it sent for `/`. One origin, one policy",
      );
    }
    policy ??= sent;
  }

  // 5. The policy is a policy about this page.
  if (policy == null) {
    finding(workerFile, "sent no `content-security-policy` at all");
  } else if (html !== "") {
    for (const name of ["default-src", "script-src", "object-src", "base-uri", "form-action", "frame-ancestors"]) {
      const values = directive(policy, name);
      if (values == null || values.join(" ") !== "'none'") {
        finding(
          workerFile,
          `\`${name}\` is ${values == null ? "absent" : `\`${values.join(" ")}\``}. This page ` +
            "has no script, no plugin, no `<base>`, no form and nothing to frame, so every " +
            "one of these costs nothing and closes a class",
        );
      }
    }
    if (directive(policy, "img-src")?.join(" ") !== "'self'") {
      finding(
        workerFile,
        "`img-src` is not `'self'`. The mark is served from this origin so that it does not " +
          "have to be anything else",
      );
    }

    const styleSrc = directive(policy, "style-src") ?? [];
    if (styleSrc.includes("'unsafe-inline'")) {
      finding(
        workerFile,
        "`style-src` holds `'unsafe-inline'`. The page has one `<style>` and its bytes are " +
          "a constant in the worker, so it can be named by its hash",
      );
    }

    // The digest, recomputed from what came back. This is the one that catches
    // a stylesheet edited without the policy following it — which on this page
    // is not a subtle degradation, it is the front door with no styles at all.
    const open = html.indexOf("<style>");
    const close = html.indexOf("</style>");
    if (open === -1 || close === -1) {
      finding(`${workerFile} /`, "the page has no `<style>`, and its stylesheet is inline");
    } else {
      const css = html.slice(open + "<style>".length, close);
      const hash = `'sha256-${crypto.createHash("sha256").update(css, "utf8").digest("base64")}'`;
      if (!styleSrc.includes(hash)) {
        finding(
          `${workerFile} /`,
          "`style-src` does not name the hash of the `<style>` the page returned, so a " +
            `browser would drop it and serve the front door unstyled. It is:\n             ${hash}`,
        );
      }
    }

    if (/<script[\s>]/i.test(html)) {
      finding(
        `${workerFile} /`,
        "the page has a `<script>` and the policy says `script-src 'none'`, so it would not " +
          "run. The documentation site needs an inline script for its theme toggle; this " +
          "page has no toggle and no other reason for one",
      );
    }
  }

  // 6. Everything the page loads, it can serve.
  let loads = 0;
  for (const { tag, attribute, url } of subresources(html)) {
    let resolved;
    try {
      resolved = new URL(url, `${APEX}/`);
    } catch {
      finding(`${workerFile} /`, `a <${tag} ${attribute}> names \`${url}\`, which is not a URL`);
      continue;
    }
    if (resolved.origin !== APEX) {
      finding(
        `${workerFile} /`,
        `a <${tag} ${attribute}> loads \`${url}\` from \`${resolved.origin}\`, and the policy ` +
          "is `img-src 'self'` under `default-src 'none'`. The browser would block it",
      );
      continue;
    }
    loads += 1;
    const response = await ask(resolved.toString());
    if (response.status !== 200) {
      finding(
        `${workerFile} /`,
        `a <${tag} ${attribute}> loads \`${url}\` and this worker answers it ${response.status}. ` +
          `Nothing in \`${brand}/\` is at that name`,
      );
    }
  }

  console.log(`apex-routes: ${workerFile}`);
  console.log(
    `  ${ROUTES.length} routes, ${loads} subresources, ` +
      `${Object.keys(REQUIRED).length} headers on 5 paths, ${html.length} bytes of page`,
  );

  if (findings.length === 0) {
    console.log("apex-routes: ok");
    return;
  }
  console.error("");
  for (const item of findings) {
    console.error(`  finding    ${item.where}`);
    console.error(`             ${item.message}`);
  }
  console.error(
    `\n${findings.length} finding(s). The apex serves a page and hands every other path to\n` +
      "the documentation host; a link that resolved yesterday has to resolve today, and\n" +
      "the page has to load only what this same worker can answer.",
  );
  process.exitCode = 1;
}

main().catch((error) => {
  console.error(`apex-routes: ${error?.stack ?? String(error)}`);
  process.exit(2);
});
NODE
