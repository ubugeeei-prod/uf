#!/bin/sh
# `verify-release.sh` against `gh` and `npm` stubs, without touching either.
#
# The check exists because a version reached npm with no GitHub release twice
# (#464, #1328), and a daily run that goes red on a release still in progress,
# or green on a rate-limited API answer, is one people stop reading. So the
# cases here are as much about what it must not report as about what it must.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

script="tools/release/verify-release.sh"
work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-verify-release.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
  echo "test-verify-release: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

mkdir -p "${work}/bin"
# `gh api repos/<repo>/releases?...&page=N` answers `releases-N.json` from the
# fixture, and an empty list past the last page, as GitHub does. Every request
# is logged so the test can see how many pages were asked for.
cat >"${work}/bin/gh" <<'EOF'
#!/bin/sh
[ "$1" = api ] || { echo "gh stub: unexpected command $*" >&2; exit 1; }
echo "$2" >>"${FIXTURE}/gh.log"
page="${2##*page=}"
if [ -f "${FIXTURE}/releases-${page}.json" ]; then
  cat "${FIXTURE}/releases-${page}.json"
else
  echo "[]"
fi
EOF
cat >"${work}/bin/npm" <<'EOF'
#!/bin/sh
[ "$1 $2" = "view @uniflowed/core" ] || { echo "npm stub: unexpected command $*" >&2; exit 1; }
echo "$*" >>"${FIXTURE}/npm.log"
cat "${FIXTURE}/npm.json"
EOF
chmod +x "${work}/bin/gh" "${work}/bin/npm"

# fixture <name> <spec>...
#
# Each spec is `<version>:<npm age in hours>:<release>`, where the release is
#   full     every target, signed — what release.yml publishes today
#   old      two targets with checksums, no signatures — an early release
#   partial  every target but Windows
#   draft    a full release left as a draft
#   none     no release at all
#   pending  a full release whose Windows archive is still uploading
# and a version with no age (`<version>::<release>`) is on GitHub but not npm.
fixture() {
  name="$1"
  shift
  mkdir -p "${work}/${name}"
  node - "${work}/${name}" "$@" <<'NODE'
const fs = require("node:fs");
const path = require("node:path");
const [dir, ...specs] = process.argv.slice(2);
const targets = [
  "x86_64-unknown-linux-gnu",
  "aarch64-unknown-linux-gnu",
  "x86_64-apple-darwin",
  "aarch64-apple-darwin",
  "x86_64-pc-windows-msvc",
];
const asset = (name, state = "uploaded") => ({ name, state, size: 100 });
const signed = (list) =>
  list.flatMap((t) => [
    asset(`uf-${t}.tar.gz`),
    asset(`uf-${t}.tar.gz.sha256`),
    asset(`uf-${t}.tar.gz.sigstore`),
    asset(`manifest-${t}.json`),
  ]);
const shapes = {
  full: () => signed(targets),
  old: () => targets.slice(0, 2).flatMap((t) => [asset(`uf-${t}.tar.gz`), asset(`uf-${t}.tar.gz.sha256`)]),
  partial: () => signed(targets.slice(0, 4)),
  draft: () => signed(targets),
  pending: () =>
    signed(targets).map((a) => (a.name === "uf-x86_64-pc-windows-msvc.tar.gz" ? { ...a, state: "starter" } : a)),
};
const releases = [];
const npm = { versions: [], time: { created: "2020-01-01T00:00:00.000Z" } };
for (const spec of specs) {
  const [version, hours, shape] = spec.split(":");
  if (hours !== "") {
    npm.versions.push(version);
    npm.time[version] = new Date(Date.now() - Number(hours) * 3600 * 1000).toISOString();
  }
  if (shape === "none") continue;
  releases.push({
    tag_name: `uf@${version}`,
    draft: shape === "draft",
    assets: [asset("VERSION"), asset("manifest.json"), ...shapes[shape]()],
  });
}
fs.writeFileSync(path.join(dir, "releases-1.json"), JSON.stringify(releases));
fs.writeFileSync(path.join(dir, "npm.json"), JSON.stringify(npm));
fs.writeFileSync(path.join(dir, "gaps.txt"), "# none\n");
NODE
}

run() {
  name="$1"
  shift
  FIXTURE="${work}/${name}" UF_RELEASE_GAPS="${work}/${name}/gaps.txt" \
    GITHUB_REPOSITORY=example/uf PATH="${work}/bin:$PATH" \
    sh "$script" "$@" >"${work}/${name}.log" 2>&1
}

log() {
  cat "${work}/$1.log"
}

# ---- --all ------------------------------------------------------------------

fixture healthy 1.0.0-alpha.1:900:old 1.0.0-alpha.2:500:partial 1.0.0-alpha.3:100:full
run healthy --all || fail "every version released was refused:
$(log healthy)"
grep -q "every version on npm has a release with binaries" "${work}/healthy.log" \
  || fail "success did not say so:
$(log healthy)"
pass "older releases with fewer targets pass; the newest carries all five"

fixture unreleased 1.0.0-alpha.1:900:full 1.0.0-alpha.2:48:none 1.0.0-alpha.3:24:full
if run unreleased --all; then
  fail "a version on npm with no release passed:
$(log unreleased)"
fi
grep -q "MISSING     uf@1.0.0-alpha.2  no release" "${work}/unreleased.log" \
  || fail "the unreleased version was not named:
$(log unreleased)"
grep -q "Not released with every binary: 1.0.0-alpha.2\$" "${work}/unreleased.log" \
  || fail "the summary did not name it alone:
$(log unreleased)"
grep -q "Record it in .*gaps.txt" "${work}/unreleased.log" \
  || fail "the failure did not say where a permanent gap is recorded:
$(log unreleased)"
pass "a version on npm with no release fails, even with newer ones released"

fixture inflight 1.0.0-alpha.1:900:full 1.0.0-alpha.2:0.5:none
run inflight --all || fail "a release still in progress failed the check:
$(log inflight)"
grep -q "in flight   uf@1.0.0-alpha.2  on npm 30 min ago; no release" "${work}/inflight.log" \
  || fail "the release in progress was not reported:
$(log inflight)"
(UF_RELEASE_GRACE_HOURS=0; export UF_RELEASE_GRACE_HOURS; run inflight --all) \
  && fail "with no grace the same version passed:
$(log inflight)"
pass "a version npm accepted within the grace period is reported, not failed"

fixture draft 1.0.0-alpha.1:900:full 1.0.0-alpha.2:24:draft
run draft --all && fail "a draft release passed:
$(log draft)"
grep -q "MISSING     uf@1.0.0-alpha.2  the release is a draft" "${work}/draft.log" \
  || fail "the draft was not reported as one:
$(log draft)"
pass "a draft release is not a release"

fixture newest-partial 1.0.0-alpha.1:900:partial 1.0.0-alpha.2:24:partial
run newest-partial --all && fail "the newest version without Windows passed:
$(log newest-partial)"
grep -q "MISSING     uf@1.0.0-alpha.2  uf-x86_64-pc-windows-msvc.tar.gz, " "${work}/newest-partial.log" \
  || fail "the missing target was not named:
$(log newest-partial)"
grep -q "released    uf@1.0.0-alpha.1\$" "${work}/newest-partial.log" \
  || fail "the older four-target release was held to five:
$(log newest-partial)"
pass "the newest version is held to every target"

fixture gap 1.0.0-alpha.1:900:full 1.0.0-alpha.2:500:none 1.0.0-alpha.3:24:full
printf '# known\n1.0.0-alpha.2 #1 it never ran\n' >"${work}/gap/gaps.txt"
run gap --all || fail "a recorded gap failed the check:
$(log gap)"
grep -q "known gap   uf@1.0.0-alpha.2  #1 it never ran" "${work}/gap.log" \
  || fail "the recorded gap was not reported with its reason:
$(log gap)"
pass "a gap recorded in the list is reported with its reason"

fixture repaired 1.0.0-alpha.1:900:full 1.0.0-alpha.2:24:full
printf '1.0.0-alpha.1 #1 it never ran\n' >"${work}/repaired/gaps.txt"
run repaired --all && fail "a listed version that has a release passed:
$(log repaired)"
grep -q "REPAIRED    uf@1.0.0-alpha.1" "${work}/repaired.log" \
  || fail "the repaired version was not named:
$(log repaired)"
fixture unknown 1.0.0-alpha.1:900:full
printf '9.9.9 #1 not a version npm has\n' >"${work}/unknown/gaps.txt"
run unknown --all && fail "a listed version npm does not have passed:
$(log unknown)"
grep -q "NOT ON NPM  uf@9.9.9" "${work}/unknown.log" \
  || fail "the unknown version was not named:
$(log unknown)"
pass "the list of gaps cannot outlive what it describes"

# ---- one version --------------------------------------------------------------

fixture one 1.0.0-alpha.1::full 1.0.0-alpha.2::partial 1.0.0-alpha.3::pending
run one 1.0.0-alpha.1 || fail "a complete release was refused:
$(log one)"
grep -q "uf@1.0.0-alpha.1 is released with every target" "${work}/one.log" \
  || fail "success did not say so:
$(log one)"
run one uf@1.0.0-alpha.1 || fail "the tag form was not accepted:
$(log one)"
run one 1.0.0-alpha.2 && fail "a release without Windows passed:
$(log one)"
grep -q "manifest-x86_64-pc-windows-msvc.json" "${work}/one.log" \
  || fail "the missing Windows manifest was not named:
$(log one)"
run one 1.0.0-alpha.3 && fail "an archive still uploading counted:
$(log one)"
grep -q "MISSING     uf@1.0.0-alpha.3  uf-x86_64-pc-windows-msvc.tar.gz\$" "${work}/one.log" \
  || fail "the uploading archive was not named:
$(log one)"
run one 1.0.0-alpha.4 && fail "a version with no release passed:
$(log one)"
[ ! -e "${work}/one/npm.log" ] || fail "checking one version asked npm"
pass "one version is held to every target, signed, fully uploaded"

# ---- what GitHub answers --------------------------------------------------------

fixture limited 1.0.0-alpha.1:900:full
echo '{"message":"API rate limit exceeded"}' >"${work}/limited/releases-1.json"
run limited --all && fail "a rate-limited answer passed:
$(log limited)"
grep -q "GitHub did not list releases: .*rate limit" "${work}/limited.log" \
  || fail "the rate limit was not reported as the reason:
$(log limited)"
[ "$(wc -l <"${work}/limited/gh.log" | tr -d ' ')" = 1 ] \
  || fail "it went on asking after a non-list answer"
pass "an answer that is not a list fails instead of reading as no releases"

fixture paged 1.0.0-alpha.1:900:full
# shellcheck disable=SC2016 # JavaScript template literals, not the shell's
node -e '
const fs = require("node:fs");
const dir = process.argv[1];
const real = JSON.parse(fs.readFileSync(`${dir}/releases-1.json`, "utf8"));
const filler = Array.from({ length: 100 }, (_, i) => ({ tag_name: `other@${i}`, draft: false, assets: [] }));
fs.writeFileSync(`${dir}/releases-1.json`, JSON.stringify(filler));
fs.writeFileSync(`${dir}/releases-2.json`, JSON.stringify(real));
' "${work}/paged"
run paged --all || fail "a release on the second page was not found:
$(log paged)"
[ "$(wc -l <"${work}/paged/gh.log" | tr -d ' ')" = 2 ] \
  || fail "expected two pages to be asked for:
$(cat "${work}/paged/gh.log")"
pass "every page of releases is read"

echo "test-verify-release: ok"
