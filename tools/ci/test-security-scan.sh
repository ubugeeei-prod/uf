#!/bin/sh
# `security-scan.sh` against repositories it can safely make wrong.
#
# The scan's whole value is that it *notices*, and a scan that notices nothing
# looks exactly like a scan with nothing to notice — a green shield, which is
# the thing ubugeeei-prod/uf#524 says not to build. So every rule it applies is
# planted here as the defect it is meant to catch, and the check has to fail on
# it and pass without it.
#
# The repository each case runs in is a scratch tree with the shape the scan
# reads: a threat model, an output directory, and a scaffolded project. The
# scan resolves references against the tree it is standing in, so the rows in
# the planted model name files this script also plants — which means case 1 is
# a real end-to-end pass rather than a model with no claims in it.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
script="$repo_root/tools/ci/security-scan.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-security-scan.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM
# The physical path, because macOS puts `$TMPDIR` behind a symlink and case 9
# plants the string the scan compares against its own working directory.
work="$(CDPATH='' cd -- "$work" && pwd -P)"

fail() {
  echo "test-security-scan: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

root="$work/repo"

# A repository the scan passes on, rebuilt from scratch for each case so a
# planted defect cannot leak into the next one.
scratch() {
  rm -rf "$root"
  mkdir -p "$root/tools/ci" "$root/docs/dist/docs" "$root/scaffold" \
    "$root/crates/uf_demo/src" "$root/packages/demo" "$root/tests/library"
  cp "$script" "$root/tools/ci/security-scan.sh"

  # Things the model's rows point at.
  printf '[package]\nname = "uf_demo"\n' > "$root/crates/uf_demo/Cargo.toml"
  printf 'pub mod guard;\n' > "$root/crates/uf_demo/src/lib.rs"
  printf 'pub fn refuse() {}\n' > "$root/crates/uf_demo/src/guard.rs"
  printf '{"name": "@uniflowed/demo"}\n' > "$root/packages/demo/package.json"
  printf '// a test\n' > "$root/tests/library/demo.test.js"

  cat > "$root/docs/security.md" <<'MODEL'
# Security

The threat model.

## A table that is not a threat model table

Its last column is not `Test` or `Where`, so it is a table about a manager's
configuration rather than a row of the threat model, and nothing below reads it.

| manager | field |
| --- | --- |
| pnpm | `pnpm.onlyBuiltDependencies` |

## A section

The threat model table is last in this file on purpose: every case below adds a
row by appending to it.

| Past failure | Structural decision in `uf` | Test |
| --- | --- | --- |
| [CVE-2025-30208](https://example.invalid/1) — a path escaped a served root | Resolved and re-checked against the canonical root | `uf_demo::guard` |
| A dependency runs code at install time | Refused before anything is fetched | `crates/uf_demo` |
| Something nobody has answered yet | It is not answered | todo |
| A decision that is the absence of a feature | uf has no such feature to abuse | — |
| A package that has to exist | It does | `@uniflowed/demo`, `tests/library/demo.test.js` |
MODEL

  printf '<!doctype html><html><body>hello</body></html>\n' \
    > "$root/docs/dist/docs/index.html"

  printf '{\n  "name": "scaffolded",\n  "private": true\n}\n' > "$root/scaffold/package.json"
  printf 'dist/\n.uf/\n.env.local\n.env.*.local\n' > "$root/scaffold/.gitignore"
  cat > "$root/scaffold/uf.config.js" <<'CONFIG'
// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({ tasks: { build: { command: "uf build" } } });
CONFIG
}

run() {
  ( cd "$root" && sh tools/ci/security-scan.sh --scaffold scaffold ) >"$work/out" 2>&1
}

# 1. The shape it is meant to pass on. If this case ever fails, every case
#    below it is meaningless: they would be failing for the same reason.
scratch
if ! run; then
  cat "$work/out" >&2
  fail "rejected a repository in which every rule holds"
fi
pass "passes a threat model whose rows all resolve"

# 2. The defect this check was written for: three rows at the end of a section,
#    after the prose, with no header above them. GFM renders them as a
#    paragraph with pipes in it, so the row is invisible to a reader of the
#    page and to anything that parses tables.
scratch
cat >> "$root/docs/security.md" <<'ORPHAN'

Some prose that closes the section.

| A row nobody will ever see | Because it is not in a table | todo |
ORPHAN
if run; then
  fail "accepted a table row that is not inside a table"
fi
grep -q "outside a table" "$work/out" || fail "did not say why the orphan row is wrong"
pass "rejects a table row written outside a table"

# 3. A row naming a test that is not there. This is drift rather than a typo:
#    the row was true when it was written and a rename made it a claim about a
#    file nobody can open.
scratch
printf '| A guard that moved | It is still enforced | `tests/library/moved.test.js` |\n' \
  >> "$root/docs/security.md"
if run; then
  fail "accepted a row naming a test file that does not exist"
fi
scratch
sed -i.bak 's|`uf_demo::guard`|`uf_demo::gone`|' "$root/docs/security.md"
rm -f "$root/docs/security.md.bak"
if run; then
  fail "accepted a row naming a module the crate does not have"
fi
pass "rejects a row whose test no longer exists"

# 4. A row that points nowhere and does not say so reads as a claim. The same
#    row marked `todo` is the document working as intended.
scratch
printf '| Something | A structural decision | it is fine |\n' >> "$root/docs/security.md"
if run; then
  fail "accepted a row whose last cell names nothing at all"
fi
scratch
printf '| Something | A structural decision | todo |\n' >> "$root/docs/security.md"
run || fail "rejected a row that is honestly marked todo"
pass "rejects a row that points nowhere without saying so"

# 5. One CVE, two rows, two different answers. Whichever a reader reaches
#    first is what they believe, and one of them is wrong.
scratch
printf '| [CVE-2025-30208](https://example.invalid/1) — the same one again | Not answered | todo |\n' \
  >> "$root/docs/security.md"
if run; then
  fail "accepted one CVE claimed as tested by one row and todo by another"
fi
grep -q "CVE-2025-30208" "$work/out" || fail "did not name the contradicting CVE"
pass "rejects two rows that contradict each other about one CVE"

# 6. The build's own notes in the output directory. `docs/security.md` says
#    they are written to `.uf/build/meta/` "rather than into the output
#    directory", and a static host serves whatever is in that directory.
scratch
printf '{"routes": []}\n' > "$root/docs/dist/docs/uf-rsc-manifest.json"
if run; then
  fail "accepted uf-rsc-manifest.json in the published output"
fi
pass "rejects the build's own notes in the output directory"

# 7. A credential file that reached the output. Vite's public directory copies
#    what it is given, and nothing between it and a reader says no.
scratch
printf 'SECRET_TOKEN=hunter2\n' > "$root/docs/dist/docs/.env"
if run; then
  fail "accepted a .env file in the published output"
fi
pass "rejects a credential file in the published output"

# 8. A credential *string*, in a file that is otherwise ordinary markup.
#
#    Assembled from two pieces rather than written out, so that this file does
#    not itself contain something shaped like a GitHub token. Push protection
#    and every other secret scanner reads a repository's source, and a test
#    fixture that trips one is a test that has to be argued about forever.
scratch
printf '<html><body>%s_%s</body></html>\n' "ghp" "0123456789abcdefghijklmnopqrstuvwxyz" \
  > "$root/docs/dist/docs/index.html"
if run; then
  fail "accepted a GitHub token in a published file"
fi
pass "rejects a credential-shaped string in a published file"

# 9. The build machine's own path, which is what a source map root or a baked
#    stack trace leaves behind.
scratch
printf '{"sourceRoot": "%s/docs"}\n' "$root" > "$root/docs/dist/docs/app.js.map"
if run; then
  fail "accepted the absolute path of the build machine in a published file"
fi
pass "rejects the build machine's own path in a published file"

# 10. A scaffold that writes a lifecycle script produces a project `uf install`
#     refuses, and the way out a reader reaches for approves every dependency
#     they have.
scratch
printf '{\n  "name": "scaffolded",\n  "scripts": { "postinstall": "node setup.js" }\n}\n' \
  > "$root/scaffold/package.json"
if run; then
  fail "accepted a scaffold whose manifest declares a lifecycle script"
fi
pass "rejects a scaffold that declares a lifecycle script"

# 11. `*` is never written for the user.
scratch
cat > "$root/scaffold/uf.config.js" <<'CONFIG'
// @flow
export default { dev: { allowedHosts: ["*"] } };
CONFIG
if run; then
  fail "accepted a scaffold that writes allowedHosts: [\"*\"]"
fi
scratch
cat > "$root/scaffold/uf.config.js" <<'CONFIG'
// @flow
export default { pm: { allowLifecycleScripts: true } };
CONFIG
if run; then
  fail "accepted a scaffold that turns lifecycle scripts on"
fi
pass "rejects a scaffold that weakens a default"

# 12. The two files in the env cascade that hold credentials.
scratch
printf 'dist/\n.uf/\n' > "$root/scaffold/.gitignore"
if run; then
  fail "accepted a scaffold whose .gitignore does not cover the local env files"
fi
pass "rejects a scaffold that would let .env.local be committed"

# 13. `uf new` and `uf new --lib` write two different manifests and two
#     different configs, so the scan audits both and a defect in either has to
#     surface. This is the second one.
scratch
cp -R "$root/scaffold" "$root/scaffold-lib"
printf '{\n  "name": "lib",\n  "scripts": { "prepare": "node build.js" }\n}\n' \
  > "$root/scaffold-lib/package.json"
if ( cd "$root" \
  && sh tools/ci/security-scan.sh --scaffold scaffold --scaffold scaffold-lib ) \
  >"$work/out" 2>&1; then
  fail "audited only the first of two scaffolds"
fi
grep -q "scaffold-lib" "$work/out" || fail "did not name the second scaffold"
pass "audits every scaffold it is given"

echo "test-security-scan: ok"
