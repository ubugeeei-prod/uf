#!/usr/bin/env sh
# What actually reached GitHub Releases, checked against what reached npm.
#
# `verify-npm.sh` asks one direction: a version that was released is on npm.
# This asks the other: a version on npm has a GitHub release carrying its
# binaries. Nothing asked it, and it has gone wrong twice — `uf@0.0.0-alpha.9`
# (#464) and `uf@0.0.0-alpha.44` (#1328) are on npm, every package of them,
# with no release and nothing behind `curl | sh`. A project can pin
# `@uniflowed/*` at either version and have no toolchain binary to match it.
# Both were found days later, by a person.
#
#   tools/release/verify-release.sh                  # the version in the tree
#   tools/release/verify-release.sh 0.0.0-alpha.47   # one version, every target
#   tools/release/verify-release.sh --all            # every version npm has
#
# One version is checked strictly: the release exists, is not a draft, and
# carries `VERSION`, `manifest.json`, and for each of the five targets the
# archive, its `.sha256`, its `.sigstore` bundle and its manifest. `release.yml`
# runs this after publishing.
#
# `--all` reads every version of `@uniflowed/core` from the registry — every
# release publishes it — and checks each one has a published release with
# `VERSION`, `manifest.json` and at least one archive with its checksum. Older
# releases shipped fewer targets and no signatures, so the five-target rule is
# held only to the newest version on npm; each version was the newest when it
# went out, and `release.yml` checks it strictly then. A schedule runs this
# daily, so a gap is found within a day whichever path produced the version.
#
# A version npm accepted within the last `UF_RELEASE_GRACE_HOURS` (3) hours is
# reported and not failed: `release.yml` publishes binaries after npm, and a
# release in progress is not a missing one.
#
# `release-gaps.txt` lists the versions that are known to have no release and
# will not get one. They are reported, not failed — and a listed version that
# does have a release now fails, so the list cannot outlive what it describes.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

repository="${GITHUB_REPOSITORY:-ubugeeei-prod/uf}"
grace_hours="${UF_RELEASE_GRACE_HOURS:-3}"
# The list, and a way for the test to hand it another one.
gaps="${UF_RELEASE_GAPS:-tools/release/release-gaps.txt}"
all=false
version=""

for argument in "$@"; do
  case "$argument" in
    --all) all=true ;;
    -*) echo "verify-release: unknown option: $argument" >&2; exit 2 ;;
    *) version="${argument#uf@}" ;;
  esac
done

if [ "$all" = true ] && [ -n "$version" ]; then
  echo "verify-release: --all checks every version; do not also name one" >&2
  exit 2
fi
if [ "$all" = false ] && [ -z "$version" ]; then
  version="$(node -p "require('./npm/core/package.json').version")"
fi

# With a template: BSD `mktemp -d` ignores `TMPDIR` without one.
work="$(mktemp -d "${TMPDIR:-/tmp}/uf-verify-release.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

# Every release, a page at a time. One request per version would be forty
# requests for `--all`; the list is one or two.
page=1
while :; do
  gh api "repos/${repository}/releases?per_page=100&page=${page}" >"${work}/releases-${page}.json"
  # Anything but a list — a rate limit, a bad token — is an answer about the
  # request, not about the releases, and must not read as "none".
  # shellcheck disable=SC2016 # JavaScript template literal, not the shell's
  count="$(node -e '
    const body = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8"));
    if (!Array.isArray(body)) {
      console.error(`verify-release: GitHub did not list releases: ${JSON.stringify(body).slice(0, 300)}`);
      process.exit(1);
    }
    console.log(body.length);
  ' "${work}/releases-${page}.json")"
  [ "$count" -lt 100 ] && break
  page=$((page + 1))
done

if [ "$all" = true ]; then
  echo "verify-release: every @uniflowed/core version on npm has a release on ${repository}"
  npm view @uniflowed/core versions time --json >"${work}/npm.json"
else
  echo "verify-release: uf@${version} on ${repository} carries every target"
fi

node - "$work" "$all" "$version" "$grace_hours" "$gaps" <<'NODE'
const fs = require("node:fs");
const path = require("node:path");
const [work, all, only, graceHours, gapsFile] = process.argv.slice(2);

// The five `release.yml` refuses to publish without.
const TARGETS = [
  "x86_64-unknown-linux-gnu",
  "aarch64-unknown-linux-gnu",
  "x86_64-apple-darwin",
  "aarch64-apple-darwin",
  "x86_64-pc-windows-msvc",
];

const releases = new Map();
for (const name of fs.readdirSync(work).filter((n) => /^releases-\d+\.json$/.test(n))) {
  for (const release of JSON.parse(fs.readFileSync(path.join(work, name), "utf8"))) {
    releases.set(release.tag_name, release);
  }
}

const gaps = new Map();
for (const line of fs.readFileSync(gapsFile, "utf8").split("\n")) {
  const trimmed = line.trim();
  if (!trimmed || trimmed.startsWith("#")) continue;
  const [version, ...reason] = trimmed.split(/\s+/);
  gaps.set(version, reason.join(" "));
}

// What a release is missing, or [] when it has everything asked of it.
function missing(version, strict) {
  const release = releases.get(`uf@${version}`);
  if (!release) return ["no release"];
  if (release.draft) return ["the release is a draft"];
  // An asset still uploading, or one that failed, is listed with its name.
  const assets = new Set(
    release.assets.filter((a) => a.state === "uploaded" && a.size > 0).map((a) => a.name),
  );
  const need = ["VERSION", "manifest.json"];
  if (strict) {
    for (const target of TARGETS) {
      const archive = `uf-${target}.tar.gz`;
      need.push(archive, `${archive}.sha256`, `${archive}.sigstore`, `manifest-${target}.json`);
    }
  } else {
    const archives = [...assets].filter((name) => /^uf-.+\.tar\.gz$/.test(name));
    if (archives.length === 0) need.push("uf-<target>.tar.gz");
    for (const archive of archives) need.push(`${archive}.sha256`);
  }
  return need.filter((name) => !assets.has(name));
}

const line = (label, version, detail) =>
  console.log(`  ${label.padEnd(11)} uf@${version}${detail ? `  ${detail}` : ""}`);

const failed = [];
let stale = [];

if (all === "false") {
  const found = missing(only, true);
  if (found.length) {
    line("MISSING", only, found.join(", "));
    failed.push(only);
  } else line("released", only, `${TARGETS.length} targets, signed`);
} else {
  const npm = JSON.parse(fs.readFileSync(path.join(work, "npm.json"), "utf8"));
  const versions = [...(npm.versions || [])].sort(
    (a, b) => Date.parse(npm.time[a]) - Date.parse(npm.time[b]),
  );
  if (versions.length === 0) {
    console.error("verify-release: npm reported no versions of @uniflowed/core");
    process.exit(1);
  }
  const newest = versions.at(-1);
  const grace = Number(graceHours) * 60 * 60 * 1000;
  for (const version of versions) {
    const found = missing(version, version === newest);
    const age = Date.now() - Date.parse(npm.time[version]);
    if (gaps.has(version)) {
      if (found.length === 0) {
        line("REPAIRED", version, `has a release now; take it out of ${gapsFile}`);
        stale.push(version);
      } else line("known gap", version, gaps.get(version));
    } else if (found.length === 0) {
      line("released", version, version === newest ? `${TARGETS.length} targets, signed` : "");
    } else if (age < grace) {
      line("in flight", version, `on npm ${Math.round(age / 60000)} min ago; ${found.join(", ")}`);
    } else {
      line("MISSING", version, found.join(", "));
      failed.push(version);
    }
  }
  const unknown = [...gaps.keys()].filter((version) => !versions.includes(version));
  for (const version of unknown) line("NOT ON NPM", version, `listed in ${gapsFile}`);
  stale = stale.concat(unknown);
}

if (failed.length) {
  console.error(`
Not released with every binary:${failed.map((v) => ` ${v}`).join("")}

npm has these versions, so a project can install their @uniflowed/* packages —
and on a platform whose archive is missing, \`UF_VERSION=<version>\` and
\`uf self-update <version>\` have nothing to download. npm is not the part to
undo: its versions are permanent.

  * The release run failed or never ran. Re-run the failed jobs of that
    version's Release run, or repeat \`uf run release\` while its progress is
    still saved: it resumes and dispatches only what has not succeeded.
  * The release cannot be rebuilt (it predates the release PR flow, or its
    validated archives have expired). Record it in ${gapsFile}
    with the issue that says so, and the next run reports it instead.`);
  process.exit(1);
}
if (stale.length) {
  console.error(`
${gapsFile} is out of date:${stale.map((v) => ` ${v}`).join("")}
A listed version that has a release, or that npm does not have, is not a gap.`);
  process.exit(1);
}
console.log(
  all === "false"
    ? `\nverify-release: uf@${only} is released with every target`
    : "\nverify-release: every version on npm has a release with binaries",
);
NODE
