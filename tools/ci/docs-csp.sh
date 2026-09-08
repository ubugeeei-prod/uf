#!/bin/sh
# The documentation site's response headers, checked against the site.
#
# `docs/security.md` has a row for "XSS via CSP nonce handling", and the site
# that argues it sent no `content-security-policy` at all — ubugeeei-prod/uf#598.
# The policy now lives in `infra/cloudflare/workers/docs.js`, and a policy
# nothing checks is a comment. So this is the check, and it is deliberately not
# a grep.
#
# `tools/ci/security-scan.sh` says why not, in the paragraph that declined to
# cover response headers: *"asserting their behaviour by reading their source
# would be asserting the text of a file and calling it a server."* This drives
# the worker instead. It imports `workers/docs.js`, calls its `fetch` with an
# `ASSETS` binding that answers out of `docs/dist/docs`, and reads the headers
# off the responses that come back — the same shape
# `tests/library/deploy.test.js` uses for the adapter's worker, and for the same
# reason: the platform's own contract standing in for the platform.
#
# Two failures, and they are the two the issue asks for.
#
# ## 1. The header stops being sent
#
# Every branch of the worker is asked — the asset store, the `/setup` redirect
# and `/api/health` — and every answer must carry all four headers. Three of
# them predate the policy and are checked here too, because a check that only
# knows about the newest header is a check that lets the older ones rot.
#
# ## 2. The site starts loading something the policy forbids
#
# The policy is re-derived from `docs/dist/docs` and compared against the one
# the worker sent:
#
#   * every inline `<script>` in every built page is hashed, and its
#     `sha256-` must be in `script-src`. Four of them exist today: uf's theme
#     bootstrap and its `application/ld+json`, and React's two streaming
#     runtime blocks. React's change when React changes, and the failure they
#     would otherwise produce is a Suspense boundary that never reveals itself
#     in a browser nobody is watching. Here it is a line to paste.
#   * every `src` and `href` a page loads — a script, a stylesheet, an icon, an
#     image, a preload — must be same-origin, because `default-src 'self'` is
#     the whole policy for them. An `<a href>` is not a load and is not
#     checked; a navigation is not a fetch.
#   * `script-src` may not hold `'unsafe-inline'` or `'unsafe-eval'`, and the
#     five `'none'` directives must still be `'none'`. Those are the promises
#     the prose in the worker makes, and prose is not enforcement.
#
# `style-src` is where the site pays for Shiki, which colours every token of
# every code sample with a `style` attribute — 9,602 of them across the site.
# That is `'unsafe-inline'` on `style-src` and it is checked *for*: this asserts
# the site still has such attributes, so that if the highlighter ever stops
# emitting them somebody is told the exemption is no longer bought.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

worker="infra/cloudflare/workers/docs.js"
site="docs/dist/docs"

usage() {
  cat <<'USAGE'
usage: docs-csp.sh [--worker FILE] [--site DIR]

  --worker FILE   the worker whose handler to drive (default: infra/cloudflare/workers/docs.js)
  --site DIR      the built site to check it against (default: docs/dist/docs)
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --worker) shift; worker="${1:-}" ;;
    --worker=*) worker="${1#--worker=}" ;;
    --site) shift; site="${1:-}" ;;
    --site=*) site="${1#--site=}" ;;
    -h | --help) usage; exit 0 ;;
    *)
      printf 'docs-csp: unknown argument `%s`\n\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if [ ! -f "$worker" ]; then
  printf 'docs-csp: no worker at `%s`\n' "$worker" >&2
  exit 1
fi

if [ ! -d "$site" ]; then
  printf 'docs-csp: `%s` does not exist.\n' "$site" >&2
  printf 'This reads what `uf build` wrote; run `uf run docs:build` first, or pass\n' >&2
  printf '`--site DIR` for a build somewhere else.\n' >&2
  exit 1
fi

UF_CSP_WORKER="$worker" UF_CSP_SITE="$site" node - <<'NODE'
"use strict";

const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { pathToFileURL } = require("node:url");

const workerFile = process.env.UF_CSP_WORKER;
const site = process.env.UF_CSP_SITE;

const findings = [];
function finding(where, message) {
  findings.push({ where, message });
}

/** Every file under `dir`, recursively. */
function walk(dir, into) {
  for (const item of fs.readdirSync(dir, { withFileTypes: true })) {
    const child = path.join(dir, item.name);
    if (item.isDirectory()) {
      walk(child, into);
    } else if (item.isFile()) {
      into.push(child);
    }
  }
  return into;
}

/**
 * The `ASSETS` binding a Worker is given, over the built site.
 *
 * One line of Cloudflare's documentation is the whole contract: the binding
 * answers a `Request` with a `Response`, with a `404` where there is no such
 * asset. `html_handling: "auto-trailing-slash"` is why `/guide/` resolves
 * through `guide/index.html`.
 */
function assetsBinding(root) {
  return {
    fetch: async (request) => {
      const { pathname } = new URL(request.url);
      const candidates = [
        path.join(root, pathname),
        path.join(root, pathname, "index.html"),
        `${path.join(root, pathname)}.html`,
      ];
      for (const candidate of candidates) {
        if (fs.existsSync(candidate) && fs.statSync(candidate).isFile()) {
          const type = candidate.endsWith(".html")
            ? "text/html; charset=utf-8"
            : "application/octet-stream";
          return new Response(fs.readFileSync(candidate), {
            headers: { "content-type": type },
          });
        }
      }
      return new Response("not found", { status: 404 });
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
 * Every inline `<script>` body in `html`, with the attributes it carried.
 *
 * A hand-written scan rather than a regex over the whole document: rule 5 of
 * `docs/security.md` forbids a backtracking regex on untrusted input, and a
 * built page is 200 KB of somebody's prose either way.
 */
function inlineScripts(html) {
  const found = [];
  let at = 0;
  for (;;) {
    const open = html.indexOf("<script", at);
    if (open === -1) return found;
    const openEnd = html.indexOf(">", open);
    if (openEnd === -1) return found;
    const attributes = html.slice(open + "<script".length, openEnd);
    const close = html.indexOf("</script>", openEnd);
    if (close === -1) return found;
    if (!/\ssrc\s*=/.test(attributes)) {
      found.push({ attributes: attributes.trim(), body: html.slice(openEnd + 1, close) });
    }
    at = close + "</script>".length;
  }
}

/** Every URL the page *loads*, which is not every URL it names. */
function subresources(html) {
  const found = [];
  for (const match of html.matchAll(/<(script|link|img|source|iframe|embed|object)\b([^>]*)>/gi)) {
    const [, tag, attributes] = match;
    for (const attribute of ["src", "href", "data", "imageSrcSet", "srcSet"]) {
      const value = attributes.match(new RegExp(`\\s${attribute}="([^"]*)"`, "i"));
      if (value == null) continue;
      // A `<link rel="canonical">` is a statement about this page and not a
      // load, and `<link rel="alternate">` names a feed nothing fetches.
      if (tag.toLowerCase() === "link" && /rel="(canonical|alternate|author)"/i.test(attributes)) {
        continue;
      }
      for (const candidate of value[1].split(",")) {
        const url = candidate.trim().split(/\s+/)[0];
        if (url !== "") found.push({ tag: tag.toLowerCase(), url });
      }
    }
  }
  return found;
}

async function main() {
  const worker = await import(pathToFileURL(path.resolve(workerFile)).href);
  const handler = worker.default;
  if (handler == null || typeof handler.fetch !== "function") {
    console.error(`docs-csp: \`${workerFile}\` has no default export with a \`fetch\``);
    process.exit(2);
  }

  const env = { ASSETS: assetsBinding(site) };
  const ask = (pathname) =>
    handler.fetch(new Request(`https://docs.uniflowed.dev${pathname}`), env, {
      waitUntil: () => {},
    });

  // 1. The headers, on every branch the worker has.
  const REQUIRED = {
    "content-security-policy": null,
    "x-content-type-options": "nosniff",
    "referrer-policy": "strict-origin-when-cross-origin",
    "permissions-policy": "interest-cohort=()",
  };

  let policy = null;
  for (const pathname of ["/", "/guide/", "/assets/does-not-exist.js", "/setup", "/api/health"]) {
    const response = await ask(pathname);
    for (const [name, expected] of Object.entries(REQUIRED)) {
      const sent = response.headers.get(name);
      if (sent == null || sent === "") {
        finding(`${workerFile} ${pathname}`, `sent no \`${name}\``);
        continue;
      }
      if (expected != null && sent !== expected) {
        finding(`${workerFile} ${pathname}`, `sent \`${name}: ${sent}\`, wanted \`${expected}\``);
      }
    }
    const sentPolicy = response.headers.get("content-security-policy");
    if (sentPolicy != null && policy != null && sentPolicy !== policy) {
      finding(
        `${workerFile} ${pathname}`,
        "sent a different policy from the one it sent for `/`. One site, one policy: a " +
          "reader who checked one path would be wrong about another",
      );
    }
    policy ??= sentPolicy;
  }

  if (policy == null) {
    console.error("docs-csp: the worker sent no `content-security-policy` at all");
    process.exit(1);
  }

  // 2. The promises the policy's own prose makes.
  const scriptSrc = directive(policy, "script-src") ?? [];
  for (const forbidden of ["'unsafe-inline'", "'unsafe-eval'", "*", "data:"]) {
    if (scriptSrc.includes(forbidden)) {
      finding(
        workerFile,
        `\`script-src\` holds \`${forbidden}\`, which is the hole the rest of this policy ` +
          "is spent closing",
      );
    }
  }
  for (const name of ["object-src", "base-uri", "form-action", "frame-src", "frame-ancestors"]) {
    const values = directive(policy, name);
    if (values == null || values.join(" ") !== "'none'") {
      finding(
        workerFile,
        `\`${name}\` is ${values == null ? "absent" : `\`${values.join(" ")}\``} and the ` +
          "site needs none of what it would allow",
      );
    }
  }
  if (directive(policy, "default-src")?.join(" ") !== "'self'") {
    finding(workerFile, "`default-src` is not `'self'`, and the site is one origin");
  }

  // 3. The site, against the policy.
  const pages = walk(site, []).filter((file) => file.endsWith(".html"));
  if (pages.length === 0) {
    console.error(`docs-csp: no pages under \`${site}\` — there is nothing to check against`);
    process.exit(2);
  }

  const hashed = new Set();
  const missing = new Map();
  let styleAttributes = 0;
  let externalOrigins = 0;

  for (const page of pages) {
    const html = fs.readFileSync(page, "utf8");
    const relative = path.relative(site, page);

    for (const { attributes, body } of inlineScripts(html)) {
      const hash = `'sha256-${crypto.createHash("sha256").update(body, "utf8").digest("base64")}'`;
      hashed.add(hash);
      if (!scriptSrc.includes(hash)) {
        const at = missing.get(hash) ?? { where: `${relative} <script ${attributes}>`, body };
        missing.set(hash, at);
      }
    }

    for (const { tag, url } of subresources(html)) {
      if (/^(https?:)?\/\//i.test(url)) {
        externalOrigins += 1;
        finding(
          path.join(site, relative),
          `a <${tag}> loads \`${url}\`, and the policy is \`default-src 'self'\`. Either the ` +
            "site should not reach off its own origin, or this policy is out of date",
        );
      }
    }

    styleAttributes += (html.match(/\sstyle="/g) ?? []).length;
  }

  for (const [hash, at] of missing) {
    finding(
      at.where,
      "an inline script the policy does not name. Add it to `script-src` in " +
        `\`${workerFile}\`:\n             ${hash}\n             its first line is: ` +
        `${at.body.trim().split("\n")[0].slice(0, 88)}`,
    );
  }

  // A hash in the policy that no page produces is a hash somebody kept after
  // the script it was for went away. Not fatal on its own — but it is dead
  // weight in a header every visitor downloads, and it is how a reader comes
  // to believe a script is still there.
  for (const value of scriptSrc) {
    if (value.startsWith("'sha256-") && !hashed.has(value)) {
      finding(
        workerFile,
        `\`script-src\` names ${value}, and no page in the build has an inline script with ` +
          "that hash. It is either stale or it was never right",
      );
    }
  }

  const styleSrc = directive(policy, "style-src") ?? [];
  if (styleAttributes > 0 && !styleSrc.includes("'unsafe-inline'")) {
    finding(
      workerFile,
      `${styleAttributes} \`style\` attributes are in the build and \`style-src\` does not ` +
        "allow inline styles, so every code sample would lose its colours",
    );
  }
  if (styleAttributes === 0 && styleSrc.includes("'unsafe-inline'")) {
    finding(
      workerFile,
      "`style-src` allows inline styles and nothing in the build uses one. The exemption " +
        "was bought for Shiki's per-token `style` attributes; if those are gone, so is the " +
        "reason",
    );
  }

  console.log(`docs-csp: ${workerFile}`);
  console.log(`  ${Object.keys(REQUIRED).length} headers, on 5 paths through the worker`);
  console.log(`docs-csp: ${site}`);
  console.log(
    `  ${pages.length} pages, ${hashed.size} distinct inline scripts, ` +
      `${styleAttributes} style attributes, ${externalOrigins} off-origin subresources`,
  );

  if (findings.length === 0) {
    console.log("docs-csp: ok");
    return;
  }
  console.error("");
  for (const item of findings) {
    console.error(`  finding    ${item.where}`);
    console.error(`             ${item.message}`);
  }
  console.error(
    `\n${findings.length} finding(s). The policy in the worker and the site the build wrote\n` +
      "have to agree: a directive that forbids what a page loads breaks the page, and a\n" +
      "hash that names nothing is a promise about a script that is not there.",
  );
  process.exitCode = 1;
}

main().catch((error) => {
  console.error(`docs-csp: ${error?.stack ?? String(error)}`);
  process.exit(2);
});
NODE
