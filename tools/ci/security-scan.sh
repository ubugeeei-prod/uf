#!/bin/sh
# uf's own security scan: the threat model, the build, and the scaffold.
#
# `.github/workflows/security.yml` runs zizmor over the workflows and CodeQL
# over the JavaScript. Both are worth having and neither looks at what uf ships
# to a user, so between them they can be entirely green about a release that
# leaks a private key into `dist/` — which is the shape of check this is not
# meant to be. Every assertion below is written so that failing it produces a
# sentence somebody can act on, and passing it says something specific.
#
# Three passes, in the order they matter.
#
# ## 1. The threat model (`docs/security.md`)
#
# Every row names a published failure in an incumbent tool, the structural
# decision that makes it impossible or loud here, and where the regression test
# lives. The document's own rules say it: *"Every guard has a test that fails
# without it. A guard with no failing test is a comment."*, and *"A row whose
# test does not exist on `main` yet is marked `todo`; that row is a work item,
# not a claim."*
#
# Nothing enforced either sentence, so the table could say anything. This
# checks four things about it:
#
#   * every row that is written as a table row is *in* a table. GFM needs a
#     header and a delimiter row; a `|`-line after a paragraph renders as prose
#     with pipes in it, so the row is invisible on the site and invisible to a
#     reader of the rendered page. Three such rows stood at the end of the
#     framework section, one of them contradicting a row above it.
#   * every row has the three cells the header promises, and none is empty.
#   * every row that is not `todo` names something that exists — a file, a
#     crate, a module, a package. This is a *name* check: it cannot know
#     whether `packages/router/middleware.test.js` still asserts the thing the
#     row claims, and says so. What it does catch is the drift that makes the
#     table dishonest without anybody touching it — a rename, a deleted test,
#     a module that moved crates.
#   * no CVE is cited by two rows where one claims a test and the other is
#     `todo`. Both cannot be true, and whichever a reader believes, the
#     document has told them the other thing.
#
# The `todo` count is printed rather than failed on. A row marked `todo` is the
# document working as intended; making that a failure would only teach people
# to delete rows.
#
# ## 2. What the build wrote
#
# Over the output directory `uf build` produced. A static site is served
# byte-for-byte by whatever host it lands on, so anything in that directory is
# published — there is no `.gitignore` and no denylist between it and a reader.
#
#   * no credential-shaped *file*: `.env`, `.env.*`, a PEM, a key, a `.npmrc`,
#     a `.git` directory. Vite's `publicDir` copies what it is given.
#   * none of the build's own notes. `docs/security.md` says
#     `uf-build-manifest.json`, `uf-rsc-manifest.json` and
#     `uf-bundle-report.json` are written to `.uf/build/meta/` "rather than into
#     the output directory", and between them they name every route including
#     the ones that were never prerendered. This is that sentence, checked.
#   * no credential-shaped *string*. The well-known prefixes only — a PEM
#     header, `AKIA…`, `ghp_…`, `github_pat_…`, `npm_…`, `xox[baprs]-`,
#     `AIza…`, `sk-…`. High-signal patterns rather than entropy, because a
#     scanner that guesses at randomness spends its life being overruled and
#     ends up switched off.
#   * not the absolute path of the machine that built it. A source map's
#     `sourceRoot`, a stack trace baked into a bundle, an error string with a
#     developer's home directory in it: all of them publish the build machine's
#     layout, and all of them are found by looking for the one string this
#     process already knows, which is its own working directory. Prose cannot
#     produce a false positive here, because prose does not know what the CI
#     runner's checkout path is.
#
# ## 3. What the scaffold writes
#
# `uf new` is the first uf anybody runs, and what it writes is what a project
# lives with. Both shapes are scaffolded and audited — `uf new` and
# `uf new --lib` write different manifests and different configs, and a default
# weakened in one would be invisible from the other. Three of their properties
# are security properties:
#
#   * the manifest declares no lifecycle script. `uf install` *fails* on a
#     manifest that declares them (`docs/security.md`, "Package manager"), so a
#     scaffold that wrote a `postinstall` would produce a project uf itself
#     refuses to install — and the fix a reader would reach for is
#     `pm.allowLifecycleScripts: true`, which approves every dependency they
#     have.
#   * the config weakens nothing: no `allowedHosts` with a `*` in it, no
#     `allowLifecycleScripts`, no `fs.deny` override. `*` is never written for
#     the user, and the place that would break that promise first is a
#     template.
#   * the `.gitignore` covers `.env.local` and `.env.*.local`. Those are the
#     two files in the cascade meant to hold credentials, and a scaffold that
#     does not ignore them is a scaffold whose first commit can carry one.
#
# ## What this is not
#
# It is not a dependency audit (#492), not an install-script audit (#495), and
# not a substitute for a row's own regression test. It does not run the site:
# response headers are set by the Cloudflare workers in `infra/cloudflare/`,
# which are a deployment rather than something `uf build` writes, and asserting
# their behaviour by reading their source would be asserting the text of a file
# and calling it a server.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

model="docs/security.md"
site="docs/dist/docs"
scaffolds=""
uf_bin="${UF_BIN:-./target/release/uf}"
work=""

usage() {
  cat <<'USAGE'
usage: security-scan.sh [--model FILE] [--site DIR] [--scaffold DIR]

  --model FILE    the threat model to audit (default: docs/security.md)
  --site DIR      the built site to scan (default: docs/dist/docs)
  --scaffold DIR  a project directory to audit instead of running `uf new`.
                  May be given more than once.
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --model) shift; model="${1:-}" ;;
    --model=*) model="${1#--model=}" ;;
    --site) shift; site="${1:-}" ;;
    --site=*) site="${1#--site=}" ;;
    --scaffold) shift; scaffolds="$scaffolds${1:-}
" ;;
    --scaffold=*) scaffolds="$scaffolds${1#--scaffold=}
" ;;
    -h | --help) usage; exit 0 ;;
    *)
      printf 'security-scan: unknown argument `%s`\n\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if [ ! -f "$model" ]; then
  printf 'security-scan: no threat model at `%s`\n' "$model" >&2
  exit 1
fi

if [ ! -d "$site" ]; then
  printf 'security-scan: `%s` does not exist.\n' "$site" >&2
  printf 'The second pass reads what `uf build` wrote; run `uf run docs:build` first,\n' >&2
  printf 'or pass `--site DIR` for a build somewhere else.\n' >&2
  exit 1
fi

# The projects to audit. Written by the binary this repository builds, because
# the question is what `uf new` writes *now* and not what a template file says
# it writes — the two are the same until somebody edits the code between them.
#
# Both shapes, because they are two different files: `uf new` writes an
# application's manifest and config and `uf new --lib` writes a library's, and
# a default weakened in one of them would be invisible from the other.
cleanup() {
  if [ -n "$work" ]; then
    rm -rf "$work"
  fi
}
trap cleanup EXIT INT TERM

if [ -z "$scaffolds" ]; then
  if [ ! -x "$uf_bin" ]; then
    printf 'security-scan: no uf binary at `%s`.\n' "$uf_bin" >&2
    printf 'The third pass runs `uf new` and audits what it wrote. Build it with\n' >&2
    printf '`cargo build --release --bin uf`, set UF_BIN, or pass `--scaffold DIR`.\n' >&2
    exit 1
  fi
  work="$(mktemp -d "${TMPDIR:-/tmp}/uf-security-scan.XXXXXX")"
  # `--name` so the manifest does not take the temporary directory's random
  # name, which would make the output differ between runs for no reason.
  "$uf_bin" new "$work/app" --name uf-security-scan-app >/dev/null
  "$uf_bin" new "$work/lib" --lib --name uf-security-scan-lib >/dev/null
  scaffolds="$work/app
$work/lib"
fi

UF_SCAN_MODEL="$model" \
UF_SCAN_SITE="$site" \
UF_SCAN_SCAFFOLDS="$scaffolds" \
node - <<'NODE'
"use strict";

const fs = require("node:fs");
const path = require("node:path");

const modelFile = process.env.UF_SCAN_MODEL;
const site = process.env.UF_SCAN_SITE;
const scaffolds = process.env.UF_SCAN_SCAFFOLDS.split("\n")
  .map((line) => line.trim())
  .filter((line) => line !== "");

if (scaffolds.length === 0) {
  // A pass with nothing to look at is the failure this whole file is against,
  // wearing a green tick. `--scaffold` with no argument is the way to get here.
  console.error("security-scan: no project to audit — `--scaffold` was given nothing");
  process.exit(2);
}

const findings = [];
function finding(where, message) {
  findings.push({ where, message });
}

// ---------------------------------------------------------------------------
// 1. The threat model.
// ---------------------------------------------------------------------------

const lines = fs.readFileSync(modelFile, "utf8").split("\n");

/** A markdown row's cells, honouring `\|` inside one. */
function cells(line) {
  const trimmed = line.trim().replace(/^\|/, "").replace(/\|$/, "");
  const out = [];
  let current = "";
  for (let i = 0; i < trimmed.length; i += 1) {
    if (trimmed[i] === "\\" && trimmed[i + 1] === "|") {
      current += "|";
      i += 1;
    } else if (trimmed[i] === "|") {
      out.push(current.trim());
      current = "";
    } else {
      current += trimmed[i];
    }
  }
  out.push(current.trim());
  return out;
}

const isRow = (line) => line.trimStart().startsWith("|");
const isDelimiter = (line) =>
  isRow(line) && cells(line).every((cell) => /^:?-{3,}:?$/.test(cell));

// A "threat model table" is one whose last header cell is `Test` or `Where`:
// those are the two spellings the document uses for "and here is the thing
// that makes this claim checkable". The two-column `| manager | field |` table
// under the approvals section is a table about npm's configuration and is not
// one of these — naming the rule this way is what keeps it out, rather than a
// list of line numbers that goes stale.
const rows = [];
let inFence = false;
let heading = "(no section)";
let table = null;
let inside = false;

for (let i = 0; i < lines.length; i += 1) {
  const line = lines[i];
  if (/^\s*```/.test(line)) {
    inFence = !inFence;
    continue;
  }
  if (inFence) {
    continue;
  }
  if (/^#{2,3}\s/.test(line)) {
    heading = line.replace(/^#+\s*/, "").trim();
    inside = false;
    table = null;
    continue;
  }
  if (!isRow(line)) {
    if (line.trim() !== "") {
      inside = false;
      table = null;
    }
    continue;
  }
  if (inside) {
    if (table != null) {
      rows.push({ line: i + 1, heading, cells: cells(line), header: table });
    }
    continue;
  }
  // A row that opens a table: the next line has to be the delimiter.
  if (i + 1 < lines.length && isDelimiter(lines[i + 1])) {
    const header = cells(line);
    const last = header[header.length - 1];
    table = last === "Test" || last === "Where" ? header : null;
    inside = true;
    i += 1;
    continue;
  }
  // A `|`-line that opens nothing. GFM renders it as a paragraph with pipes in
  // it, so whatever it says is said to nobody.
  finding(
    `${modelFile}:${i + 1}`,
    "a table row outside a table — with no header and delimiter above it GFM " +
      "renders this as prose, so the row is invisible on the rendered page: " +
      `\`${line.trim().slice(0, 72)}…\``,
  );
}

// Resolving a reference. Four shapes appear in the last column, and anything
// that is not one of them is prose — `dist/`, say, which is punctuation in a
// sentence rather than a place. A row whose cell yields no reference at all is
// reported, so the narrowness of this list cannot quietly excuse a row.
const crateDirs = fs.existsSync("crates")
  ? new Set(fs.readdirSync("crates", { withFileTypes: true }).filter((d) => d.isDirectory()).map((d) => d.name))
  : new Set();

const crateIndex = new Map();
function crateNames(crate) {
  if (!crateIndex.has(crate)) {
    const names = new Set();
    const root = path.join("crates", crate);
    const walk = (dir) => {
      for (const item of fs.readdirSync(dir, { withFileTypes: true })) {
        const child = path.join(dir, item.name);
        if (item.isDirectory()) {
          if (item.name !== "target" && item.name !== "fixtures") {
            names.add(item.name);
            walk(child);
          }
          continue;
        }
        if (!item.name.endsWith(".rs")) {
          continue;
        }
        names.add(item.name.replace(/\.rs$/, ""));
        const body = fs.readFileSync(child, "utf8");
        for (const match of body.matchAll(
          /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:mod|fn|struct|enum|trait|type|const|static|union)\s+([A-Za-z_][A-Za-z0-9_]*)/gm,
        )) {
          names.add(match[1]);
        }
        for (const match of body.matchAll(/^\s*macro_rules!\s+([A-Za-z_][A-Za-z0-9_]*)/gm)) {
          names.add(match[1]);
        }
      }
    };
    walk(root);
    crateIndex.set(crate, names);
  }
  return crateIndex.get(crate);
}

function resolve(reference) {
  if (/^[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z0-9_]+)+$/.test(reference)) {
    const [crate, ...rest] = reference.split("::");
    if (!crateDirs.has(crate)) {
      return `names no crate: \`crates/${crate}\` does not exist`;
    }
    const names = crateNames(crate);
    for (const segment of rest) {
      if (!names.has(segment)) {
        return `\`${crate}\` has no \`${segment}\` — a module, a test or an item by that name`;
      }
    }
    return null;
  }
  if (/^uf_[a-z0-9_]+$/.test(reference)) {
    return crateDirs.has(reference) ? null : `names no crate: \`crates/${reference}\` does not exist`;
  }
  if (/^@uniflowed\/[a-z0-9-]+$/.test(reference)) {
    const manifest = path.join("packages", reference.slice("@uniflowed/".length), "package.json");
    return fs.existsSync(manifest) ? null : `names no package: \`${manifest}\` does not exist`;
  }
  if (/^crates\/[a-z0-9_]+$/.test(reference)) {
    return fs.existsSync(path.join(reference, "Cargo.toml")) ? null : `\`${reference}\` is not a crate`;
  }
  // A directory, written with the trailing slash — `.github/workflows/`. Two
  // segments at least, so that a one-word `dist/` used as punctuation in a
  // sentence is prose rather than a place this has to find.
  if (/^[A-Za-z0-9_.][A-Za-z0-9_.-]*(?:\/[A-Za-z0-9_.-]+)+\/$/.test(reference)) {
    return fs.existsSync(reference) ? null : "names a directory that does not exist";
  }
  if (/\/.+\.(rs|js|jsx|json|md|mdx|toml|sh|yml|yaml)$/.test(reference)) {
    return fs.existsSync(reference) ? null : "names a file that does not exist";
  }
  return "unrecognised";
}

// The two markers a row uses when there is nothing to point at, and they mean
// different things. `todo` is the document's own: *a row whose test does not
// exist on `main` yet is marked `todo`; that row is a work item, not a claim*.
// An em dash is the other honest answer — the "Checked against what actually
// happened" tables use it for a row whose decision is the absence of a feature,
// where there is no file to name because there is no code. Both are counted
// and neither is failed on; what is failed on is a row that points nowhere
// without saying so, because that reads as a claim.
const NOTHING_TO_POINT_AT = new Set(["todo", "—", "-", "n/a"]);

let todo = 0;
let unlocated = 0;
let claimed = 0;
const cited = new Map();

for (const row of rows) {
  if (row.cells.length !== row.header.length) {
    finding(
      `${modelFile}:${row.line}`,
      `${row.cells.length} cells under a ${row.header.length}-column header — a row ` +
        "that does not line up with its header renders with its claim in the wrong " +
        "column",
    );
    continue;
  }
  const empty = row.cells.findIndex((cell) => cell === "");
  if (empty !== -1) {
    finding(`${modelFile}:${row.line}`, `column ${empty + 1} is empty`);
    continue;
  }

  const cell = row.cells[row.cells.length - 1];
  const marker = cell.replace(/`/g, "").trim();
  const isTodo = marker === "todo";
  for (const match of `${row.cells[0]} ${row.cells[1]}`.matchAll(/\b(CVE-\d{4}-\d{4,})\b/g)) {
    const entry = cited.get(match[1]) ?? [];
    entry.push({ line: row.line, todo: isTodo, heading: row.heading });
    cited.set(match[1], entry);
  }

  if (NOTHING_TO_POINT_AT.has(marker)) {
    if (isTodo) {
      todo += 1;
    } else {
      unlocated += 1;
    }
    continue;
  }

  const references = [...cell.matchAll(/`([^`]+)`/g)].map((match) => match[1].trim());
  const recognised = [];
  for (const reference of references) {
    const problem = resolve(reference);
    if (problem === "unrecognised") {
      continue;
    }
    recognised.push(reference);
    if (problem != null) {
      finding(`${modelFile}:${row.line}`, `\`${reference}\` ${problem}`);
    }
  }
  if (recognised.length === 0) {
    finding(
      `${modelFile}:${row.line}`,
      "points nowhere, and does not say so. A row is a guard with somewhere to " +
        "look, a `todo`, or an em dash for a decision that is the absence of a " +
        `feature. Its last cell reads: \`${cell.slice(0, 72)}\``,
    );
    continue;
  }
  claimed += 1;
}

for (const [cve, entries] of cited) {
  const tested = entries.filter((entry) => !entry.todo);
  const pending = entries.filter((entry) => entry.todo);
  if (tested.length > 0 && pending.length > 0) {
    finding(
      `${modelFile}:${pending[0].line}`,
      `${cve} is cited by a row marked \`todo\` and by a row at line ${tested[0].line} ` +
        "that names a test. Both cannot be true, and a reader believes whichever they " +
        "read first",
    );
  }
}

console.log(`security-scan: ${modelFile}`);
console.log(
  `  ${rows.length} rows, ${claimed} naming a guard, ${todo} marked todo, ` +
    `${unlocated} with nothing to name`,
);

// ---------------------------------------------------------------------------
// 2. What the build wrote.
// ---------------------------------------------------------------------------

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

const published = walk(site, []);

// Files that are a credential, or that hold one, by their name alone.
const CREDENTIAL_NAMES = [
  { test: /^\.env(\..+)?$/, what: "an env file — the cascade is where a project's secrets live" },
  { test: /\.(pem|key|p12|pfx|jks|keystore)$/i, what: "key material" },
  { test: /^\.npmrc$/, what: "an npm config, which is where a registry token goes" },
  { test: /^(id_rsa|id_ed25519|id_ecdsa)(\.pub)?$/, what: "an ssh private key" },
  { test: /^\.git-credentials$/, what: "stored git credentials" },
];

// The build's own notes. `docs/security.md` says these live in
// `.uf/build/meta/` rather than in the output, and this is that claim.
const BUILD_NOTES = new Set([
  "uf-build-manifest.json",
  "uf-rsc-manifest.json",
  "uf-bundle-report.json",
]);

for (const file of published) {
  const name = path.basename(file);
  const relative = path.relative(site, file);
  for (const rule of CREDENTIAL_NAMES) {
    if (rule.test.test(name)) {
      finding(file, `${rule.what}, and it is in the published output`);
    }
  }
  if (BUILD_NOTES.has(name)) {
    finding(
      file,
      "the build's own notes, in the output directory. They name every route " +
        "including the ones that were never prerendered, and a static host " +
        "serves whatever is in this directory to whoever asks for it",
    );
  }
  if (relative.split(path.sep).includes(".git")) {
    finding(file, "a `.git` entry in the published output");
  }
}

// Strings that are a credential wherever they appear. Prefixes that the issuer
// itself documents, so a match is a match rather than a guess.
const SECRET_SHAPES = [
  { name: "a PEM private key", test: /-----BEGIN (?:[A-Z ]+ )?PRIVATE KEY-----/ },
  { name: "an AWS access key id", test: /\bAKIA[0-9A-Z]{16}\b/ },
  { name: "a GitHub token", test: /\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36}\b/ },
  { name: "a GitHub fine-grained token", test: /\bgithub_pat_[A-Za-z0-9_]{22,}\b/ },
  { name: "an npm token", test: /\bnpm_[A-Za-z0-9]{36}\b/ },
  { name: "a Slack token", test: /\bxox[baprs]-[A-Za-z0-9-]{10,}\b/ },
  { name: "a Google API key", test: /\bAIza[0-9A-Za-z_-]{35}\b/ },
  { name: "a private key in an OpenSSH file", test: /-----BEGIN OPENSSH PRIVATE KEY-----/ },
];

// The checkout this ran in. Nothing uf builds has a reason to name it, and no
// prose can say it by accident: on the runner it is a path nobody wrote down.
// Both spellings, because a checkout reached through a symlink — `/tmp` on
// macOS, a worktree under one — has two absolute paths and a leak can carry
// either.
const checkouts = new Set([process.cwd()]);
try {
  checkouts.add(fs.realpathSync(process.cwd()));
} catch {
  // A working directory that cannot be resolved is one this process is about
  // to fail on anyway; the unresolved spelling is still worth looking for.
}

const TEXT = /\.(html|js|mjs|cjs|css|json|xml|txt|svg|map|webmanifest)$/i;
let scanned = 0;
for (const file of published) {
  if (!TEXT.test(file)) {
    continue;
  }
  const body = fs.readFileSync(file, "utf8");
  scanned += 1;
  for (const shape of SECRET_SHAPES) {
    if (shape.test.test(body)) {
      finding(file, `${shape.name} in a published file`);
    }
  }
  for (const checkout of checkouts) {
    if (body.includes(checkout)) {
      finding(
        file,
        `contains \`${checkout}\` — the absolute path of the machine that built it. ` +
          "A source map root, a baked stack trace or an error string; whichever it is, " +
          "it publishes the build machine's layout",
      );
      break;
    }
  }
}

console.log(`security-scan: ${site}`);
console.log(`  ${published.length} published files, ${scanned} of them text`);

// ---------------------------------------------------------------------------
// 3. What the scaffold wrote.
// ---------------------------------------------------------------------------

for (const scaffold of scaffolds) {
  const scaffoldFiles = walk(scaffold, []).map((file) => path.relative(scaffold, file));
  console.log(`security-scan: ${scaffold}`);
  console.log(`  ${scaffoldFiles.length} files in a freshly scaffolded project`);

  const manifestFile = path.join(scaffold, "package.json");
  if (!fs.existsSync(manifestFile)) {
    finding(scaffold, "the scaffold wrote no `package.json`");
  } else {
    let manifest = null;
    try {
      manifest = JSON.parse(fs.readFileSync(manifestFile, "utf8"));
    } catch (error) {
      finding(manifestFile, `is not JSON: ${error.message}`);
    }
    if (manifest != null) {
      const scripts = Object.keys(manifest.scripts ?? {});
      if (scripts.length > 0) {
        finding(
          manifestFile,
          `declares ${scripts.join(", ")}. \`uf install\` fails on a manifest that ` +
            "declares lifecycle scripts, so a scaffold that writes one produces a " +
            "project uf itself refuses to install — and the reader's way out is " +
            "`pm.allowLifecycleScripts: true`, which approves every dependency they have",
        );
      }
    }
  }

  for (const relative of scaffoldFiles) {
    if (!relative.endsWith(".js") && !relative.endsWith(".json")) {
      continue;
    }
    const body = fs.readFileSync(path.join(scaffold, relative), "utf8");
    const allowed = body.match(/allowedHosts\s*:\s*(\[[^\]]*\]|true)/);
    if (allowed != null && (allowed[1] === "true" || allowed[1].includes("*"))) {
      finding(
        path.join(scaffold, relative),
        `writes \`allowedHosts: ${allowed[1]}\`. \`*\` is never written for the user — ` +
          "a host allowlist that accepts anything is the DNS rebinding hole with an " +
          "allowlist-shaped comment on it",
      );
    }
    if (/allowLifecycleScripts\s*:\s*true/.test(body)) {
      finding(
        path.join(scaffold, relative),
        "turns lifecycle scripts on. That is a deliberate act with a deliberate " +
          "spelling, and a scaffold cannot be deliberate on somebody else's behalf",
      );
    }
  }

  const gitignoreFile = path.join(scaffold, ".gitignore");
  if (!fs.existsSync(gitignoreFile)) {
    finding(scaffold, "the scaffold wrote no `.gitignore`");
  } else {
    const ignored = fs
      .readFileSync(gitignoreFile, "utf8")
      .split("\n")
      .map((line) => line.trim());
    for (const pattern of [".env.local", ".env.*.local"]) {
      if (!ignored.includes(pattern)) {
        finding(
          gitignoreFile,
          `does not ignore \`${pattern}\`. It is one of the two files in the env ` +
            "cascade meant to hold credentials, and the first commit of a new project " +
            "is where it would go",
        );
      }
    }
  }
}

// ---------------------------------------------------------------------------

if (findings.length === 0) {
  console.log("security-scan: ok");
} else {
  console.error("");
  for (const item of findings) {
    console.error(`  finding    ${item.where}`);
    console.error(`             ${item.message}`);
  }
  console.error(
    `\n${findings.length} finding(s). Each is something to change rather than a score:\n` +
      "a row of the threat model that says something untrue, a file the build\n" +
      "published that should not be published, or a scaffold that starts a project\n" +
      "off worse than uf's own rules.",
  );
  process.exitCode = 1;
}
NODE
