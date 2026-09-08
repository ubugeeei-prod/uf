#!/bin/sh
# End-to-end proof that install.sh can install a release this repo produced.
#
# `curl -fsSL https://setup.uniflowed.dev | sh` is the first thing a user runs,
# and until this existed nothing checked that the installer and the packaging
# script agreed on a single byte. This serves a real packaged release over HTTP
# and runs the real installer against it, then checks the failure paths that
# would otherwise only be discovered by a user.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

installer="infra/cloudflare/setup-assets/install.sh"
release_dir="${UF_TEST_RELEASE_DIR:-}"

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "test-install: missing required command: $1" >&2
    exit 1
  }
}
need python3
need curl
need tar

fail() {
  echo "test-install: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

# Package a release unless the caller points at one they already built. The
# packaging script is the same one CI runs, so a drift between it and the
# installer fails here rather than in front of a user.
if [ -z "$release_dir" ]; then
  echo "test-install: packaging a release (set UF_TEST_RELEASE_DIR to reuse one)" >&2
  tools/release/package-binaries.sh >/dev/null
  version="$(cat dist/release/uf/*/VERSION | head -1 | tr -d '[:space:]')"
  release_dir="dist/release/uf/${version}"
fi

[ -d "$release_dir" ] || fail "release directory does not exist: $release_dir"
version="$(tr -d '[:space:]' < "${release_dir}/VERSION")"
[ -n "$version" ] || fail "VERSION in $release_dir is empty"

target="$(rustc -vV | awk -F': ' '/^host:/ { print $2 }')"
archive="uf-${target}.tar.gz"
[ -f "${release_dir}/${archive}" ] || fail "missing ${release_dir}/${archive}"

# See `install.sh`: without a template, BSD `mktemp -d` ignores `TMPDIR`.
work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-install.XXXXXX")"
server_pid=""
cleanup() {
  [ -n "$server_pid" ] && kill "$server_pid" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT INT TERM

# Serve the flat `<base>/<version>/<asset>` layout, plus a `latest/` copy, so
# both the pinned and the default code paths get exercised.
site="${work}/site/uf"
mkdir -p "${site}/${version}" "${site}/latest"
cp "${release_dir}/${archive}" "${release_dir}/${archive}.sha256" \
  "${release_dir}/VERSION" "${site}/${version}/"
cp "${site}/${version}"/* "${site}/latest/"

port="$(python3 -c 'import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()')"

python3 -m http.server "$port" --bind 127.0.0.1 --directory "${work}/site" \
  >"${work}/server.log" 2>&1 &
server_pid=$!

ready=0
i=0
while [ "$i" -lt 100 ]; do
  if curl -fsS "http://127.0.0.1:${port}/uf/latest/VERSION" >/dev/null 2>&1; then
    ready=1
    break
  fi
  i=$((i + 1))
  python3 -c 'import time; time.sleep(0.05)'
done
[ "$ready" -eq 1 ] || fail "fixture server never came up (see ${work}/server.log)"

base="http://127.0.0.1:${port}/uf"

run_installer() {
  # Each case installs into its own root so one case cannot mask another.
  case_root="${work}/$1"
  shift
  env UF_RELEASE_BASE="$base" \
    UF_INSTALL_ROOT="${case_root}/share" \
    UF_BIN_DIR="${case_root}/bin" \
    "$@" sh "$installer"
}

echo "test-install: ${version} (${target}) via ${base}"

# 1. The default path: no UF_VERSION, so the installer resolves `latest`.
run_installer latest >"${work}/latest.log" 2>&1 \
  || fail "installing latest failed:
$(cat "${work}/latest.log")"
for name in uf ufr ufx; do
  [ -x "${work}/latest/bin/${name}" ] || fail "latest: ${name} was not linked"
done
[ -L "${work}/latest/bin/uf" ] || fail "latest: uf is not a symlink"
installed_version="$("${work}/latest/bin/uf" --version 2>&1)" \
  || fail "latest: installed uf did not run: ${installed_version}"
case "$installed_version" in
  *"$version"*) ;;
  *) fail "latest: uf --version said '${installed_version}', expected ${version}" ;;
esac
pass "latest resolves, installs, links uf/ufr/ufx, and runs"

# 2. A pinned version, including the `uf@` prefix the tags carry.
run_installer pinned UF_VERSION="uf@${version}" >"${work}/pinned.log" 2>&1 \
  || fail "installing uf@${version} failed:
$(cat "${work}/pinned.log")"
[ -x "${work}/pinned/bin/uf" ] || fail "pinned: uf was not linked"
grep -q "runtimes/uf@${version}" "${work}/pinned.log" \
  || fail "pinned: did not install into runtimes/uf@${version}"
pass "UF_VERSION=uf@${version} installs the pinned release"

# 3. Reinstalling over an existing runtime must succeed, not trip on the
#    symlinks or the populated directory it left behind.
run_installer pinned UF_VERSION="$version" >"${work}/reinstall.log" 2>&1 \
  || fail "reinstalling over an existing runtime failed:
$(cat "${work}/reinstall.log")"
"${work}/pinned/bin/uf" --version >/dev/null || fail "reinstall: uf stopped working"
pass "reinstalling over an existing runtime is idempotent"

# 3b. And while that runtime's `uf` is *running*, which is what `uf self-update`
#     is: the process asking for the install is executing the file the install
#     is about to write. On Linux that write is `ETXTBSY` and GNU tar does not
#     recover from it, so an installer that unpacked over the runtime directory
#     would fail here — in the most ordinary invocation there is. `uf lsp`
#     blocks on a stdin that never speaks, which is a running uf and nothing
#     else.
sleep 60 | "${work}/pinned/bin/uf" lsp >/dev/null 2>&1 &
running_pid=$!
python3 -c 'import time; time.sleep(0.5)'
kill -0 "$running_pid" 2>/dev/null || fail "running: uf lsp did not stay up"
if ! run_installer pinned UF_VERSION="$version" >"${work}/running.log" 2>&1; then
  kill "$running_pid" 2>/dev/null || true
  fail "reinstalling while the installed uf was running failed:
$(cat "${work}/running.log")"
fi
kill "$running_pid" 2>/dev/null || true
"${work}/pinned/bin/uf" --version >/dev/null \
  || fail "running: uf stopped working after the reinstall"
pass "reinstalling while the installed uf is running is safe"

# 4. A `TMPDIR` the caller set is where the download goes.
#
#    Pointed at a directory that does not exist, an installer that honours it
#    stops and one that ignores it carries on using the system temp — so this
#    is the case that tells the two apart. Honouring it is the point: BSD
#    `mktemp -d` given no template uses the system directory whatever
#    `TMPDIR` says, and on a machine whose system temp is not writable — a
#    sandbox, a container, a locked-down CI image — that is an install that
#    fails with `mkdtemp failed` and names neither cause nor fix.
if run_installer tmpdir TMPDIR="${work}/not-a-directory" >"${work}/tmpdir.log" 2>&1; then
  fail "TMPDIR was ignored: the installer used the system temp instead
$(cat "${work}/tmpdir.log")"
fi
if [ -e "${work}/tmpdir/bin/uf" ]; then
  fail "TMPDIR: a failed install left a binary behind"
fi

#    And with one that does exist, it installs.
mkdir -p "${work}/scratch"
run_installer tmpdir2 TMPDIR="${work}/scratch" >"${work}/tmpdir2.log" 2>&1 \
  || fail "installing with TMPDIR set failed:
$(cat "${work}/tmpdir2.log")"
[ -x "${work}/tmpdir2/bin/uf" ] || fail "TMPDIR: uf was not linked"
pass "a caller's TMPDIR is used, and a bad one stops the install"

# 5. A tampered archive must be rejected, not installed.
tampered="${site}/tampered"
mkdir -p "$tampered"
cp "${site}/${version}/${archive}.sha256" "${site}/${version}/VERSION" "$tampered/"
printf 'not an archive' > "${tampered}/${archive}"
if run_installer tampered UF_VERSION=tampered >"${work}/tampered.log" 2>&1; then
  fail "a tampered archive was installed"
fi
grep -q "checksum mismatch" "${work}/tampered.log" \
  || fail "tampered archive was rejected, but not for the checksum:
$(cat "${work}/tampered.log")"
[ -e "${work}/tampered/bin/uf" ] && fail "tampered: uf was linked anyway"
pass "a tampered archive fails the checksum and installs nothing"

# 6. An archive whose members escape the extraction root must be rejected even
#    though its checksum is honest — the same host serves both.
evil="${site}/evil"
mkdir -p "$evil"
python3 - "${evil}/${archive}" <<'EOF'
import io, sys, tarfile, time

with tarfile.open(sys.argv[1], "w:gz") as tar:
    for name, data in (("bin/uf", b"#!/bin/sh\n"), ("../../../escaped", b"pwned")):
        info = tarfile.TarInfo(name)
        info.size = len(data)
        info.mode = 0o755
        info.mtime = int(time.time())
        tar.addfile(info, io.BytesIO(data))
EOF
if command -v sha256sum >/dev/null 2>&1; then
  sha="$(sha256sum "${evil}/${archive}" | awk '{print $1}')"
else
  sha="$(shasum -a 256 "${evil}/${archive}" | awk '{print $1}')"
fi
printf '%s  %s\n' "$sha" "$archive" > "${evil}/${archive}.sha256"
printf 'evil\n' > "${evil}/VERSION"
if run_installer evil UF_VERSION=evil >"${work}/evil.log" 2>&1; then
  fail "an archive with escaping members was installed"
fi
grep -q "writes outside its own directory" "${work}/evil.log" \
  || fail "escaping archive was rejected, but not by the path guard:
$(cat "${work}/evil.log")"
[ -e "${work}/escaped" ] && fail "evil: a member escaped the extraction root"
pass "an archive with members outside the root is rejected"

# 7. A version that does not exist must fail loudly rather than leave a broken
#    install behind.
if run_installer missing UF_VERSION=99.99.99 >"${work}/missing.log" 2>&1; then
  fail "a nonexistent version reported success"
fi
[ -e "${work}/missing/bin/uf" ] && fail "missing: uf was linked anyway"
pass "a nonexistent version fails and installs nothing"

# 8. ORIGIN. The attack the sha256 beside the archive cannot see: a release
#    host that serves a tampered archive *and* a checksum that matches it. The
#    local check passes — the bytes are the bytes that host advertised — and the
#    binary is the attacker's. This is ubugeeei-prod/uf#551, and the second
#    opinion is what catches it: another host, not under the same control,
#    holding the digest of the archive that was actually published.
#
#    Both hosts are 127.0.0.1 here and differ by port, which is what
#    `uf_host_of` compares: scheme, host and port together.
mirror_root="${work}/mirror/uf"
mkdir -p "${mirror_root}/${version}"
cp "${release_dir}/${archive}.sha256" "${mirror_root}/${version}/"

mirror_port="$(python3 -c 'import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()')"
python3 -m http.server "$mirror_port" --bind 127.0.0.1 --directory "${work}/mirror" \
  >"${work}/mirror.log" 2>&1 &
mirror_pid=$!
cleanup_mirror() {
  [ -n "${mirror_pid:-}" ] && kill "$mirror_pid" 2>/dev/null || true
}
trap 'cleanup; cleanup_mirror' EXIT INT TERM

ready=0
i=0
while [ "$i" -lt 100 ]; do
  if curl -fsS "http://127.0.0.1:${mirror_port}/uf/${version}/${archive}.sha256" \
    >/dev/null 2>&1; then
    ready=1
    break
  fi
  i=$((i + 1))
  python3 -c 'import time; time.sleep(0.05)'
done
[ "$ready" -eq 1 ] || fail "mirror server never came up (see ${work}/mirror.log)"
mirror_base="http://127.0.0.1:${mirror_port}/uf"

#    The honest release still installs, and says the other host agrees.
run_installer agree UF_VERSION="$version" UF_CHECKSUM_BASE="$mirror_base" \
  >"${work}/agree.log" 2>&1 \
  || fail "an honest release with a second opinion failed to install:
$(cat "${work}/agree.log")"
grep -q "agrees" "${work}/agree.log" \
  || fail "the second opinion was not reported:
$(cat "${work}/agree.log")"
pass "a second-opinion checksum from another host is fetched and agreed with"

#    Now the release host is compromised: a different archive, and a checksum
#    that matches it. Nothing on that host disagrees with anything.
#    A working archive, so the only thing wrong with this install is where it
#    came from. An archive that merely failed to unpack would be caught by
#    something else and would prove nothing about the origin check.
forged="${site}/forged"
mkdir -p "$forged"
python3 - "${forged}/${archive}" <<'EOF'
import io, sys, tarfile, time

with tarfile.open(sys.argv[1], "w:gz") as tar:
    for name in ("bin/uf", "bin/ufr", "bin/ufx"):
        data = b"#!/bin/sh\necho pwned\n"
        info = tarfile.TarInfo(name)
        info.size = len(data)
        info.mode = 0o755
        info.mtime = int(time.time())
        tar.addfile(info, io.BytesIO(data))
EOF
if command -v sha256sum >/dev/null 2>&1; then
  forged_sha="$(sha256sum "${forged}/${archive}" | awk '{print $1}')"
else
  forged_sha="$(shasum -a 256 "${forged}/${archive}" | awk '{print $1}')"
fi
printf '%s  %s\n' "$forged_sha" "$archive" > "${forged}/${archive}.sha256"
printf 'forged\n' > "${forged}/VERSION"
mkdir -p "${mirror_root}/forged"
cp "${release_dir}/${archive}.sha256" "${mirror_root}/forged/"

#    Without a second opinion it installs, because the checksum is honest about
#    the bytes that host served. That is the hole, stated as a test.
run_installer forged_alone UF_VERSION=forged >"${work}/forged-alone.log" 2>&1 \
  || fail "the transit check should still pass on a self-consistent forgery:
$(cat "${work}/forged-alone.log")"
grep -q "origin not proven" "${work}/forged-alone.log" \
  || fail "an install with no origin evidence must say so:
$(cat "${work}/forged-alone.log")"
pass "a self-consistent forgery passes the transit check, and the installer says so"

#    With one, it does not install at all.
if run_installer forged UF_VERSION=forged UF_CHECKSUM_BASE="$mirror_base" \
  >"${work}/forged.log" 2>&1; then
  fail "an archive two hosts disagree about was installed"
fi
grep -q "two hosts disagree" "${work}/forged.log" \
  || fail "the disagreement was not the reason it was refused:
$(cat "${work}/forged.log")"
[ -e "${work}/forged/bin/uf" ] && fail "forged: uf was linked anyway"
pass "an archive two hosts disagree about is refused, naming both"

# 9. `require` refuses an install whose origin nothing established, rather than
#    reporting it. This is what the release smoke job runs with.
if run_installer required UF_VERSION="$version" UF_VERIFY_ORIGIN=require \
  >"${work}/required.log" 2>&1; then
  fail "UF_VERIFY_ORIGIN=require installed a release with no origin evidence"
fi
grep -q "origin of .* could not be established" "${work}/required.log" \
  || fail "require failed for the wrong reason:
$(cat "${work}/required.log")"
pass "UF_VERIFY_ORIGIN=require refuses an install nothing vouched for"

# 10. A second opinion from the host that served the archive is the first
#     opinion again. Counting it would be the exact mistake #551 is about, so
#     it is named as not independent — and `require` still refuses.
if run_installer same_host UF_VERSION="$version" UF_VERIFY_ORIGIN=require \
  UF_CHECKSUM_BASE="$base" >"${work}/same-host.log" 2>&1; then
  fail "a checksum from the archive's own host was accepted as a second opinion"
fi
grep -q "same host as the archive" "${work}/same-host.log" \
  || fail "the same-host checksum was not called out:
$(cat "${work}/same-host.log")"
pass "a checksum from the archive's own host is not counted as a second opinion"

# 11. A typo in the switch itself must stop, not quietly install under `auto`.
if run_installer typo UF_VERSION="$version" UF_VERIFY_ORIGIN=requires \
  >"${work}/typo.log" 2>&1; then
  fail "a misspelled UF_VERIFY_ORIGIN installed anyway"
fi
grep -q "is not one uf understands" "${work}/typo.log" \
  || fail "the misspelled value was not named:
$(cat "${work}/typo.log")"
pass "a misspelled UF_VERIFY_ORIGIN stops rather than falling back to auto"


# 12. THE SIGNATURE. Everything above proves origin by making an attacker hold
#     two hosts; this proves it by making them hold a signing identity they
#     cannot have. `cosign` is stubbed — the point is not that Sigstore's
#     cryptography works, it is that the installer asks the right question and
#     believes the answer.
#
#     The stub agrees only when the installer pinned both a certificate
#     identity and an OIDC issuer. A `cosign verify-blob` with neither accepts
#     a signature by anybody, which would turn this whole check into
#     decoration, so the stub refuses to be the thing that let it pass.
signed="${site}/signed"
mkdir -p "$signed"
cp "${release_dir}/${archive}" "${release_dir}/${archive}.sha256" "$signed/"
printf 'signed\n' > "${signed}/VERSION"
# The bundle's bytes are never read here: cosign is what reads them, and cosign
# is the stub. What matters is that one is served at all.
printf '{"mediaType":"application/vnd.dev.sigstore.bundle+json;version=0.3"}\n' \
  > "${signed}/${archive}.sigstore"

stub_ok="${work}/stub-ok"
mkdir -p "$stub_ok"
cat > "${stub_ok}/cosign" <<'STUB'
#!/bin/sh
printf '%s\n' "$@" > "${COSIGN_ARGS:-/dev/null}"
case " $* " in
  *" --certificate-identity-regexp "*) ;;
  *)
    echo "stub cosign: the installer pinned no certificate identity" >&2
    exit 1
    ;;
esac
case " $* " in
  *" --certificate-oidc-issuer "*) ;;
  *)
    echo "stub cosign: the installer pinned no OIDC issuer" >&2
    exit 1
    ;;
esac
exit 0
STUB
chmod +x "${stub_ok}/cosign"

run_installer signed UF_VERSION=signed PATH="${stub_ok}:${PATH}" \
  UF_VERIFY_ORIGIN=require COSIGN_ARGS="${work}/cosign-args" \
  >"${work}/signed.log" 2>&1 \
  || fail "a signed release did not install under UF_VERIFY_ORIGIN=require:
$(cat "${work}/signed.log")"
grep -q "signed by ${UF_REPO:-ubugeeei-prod/uf}" "${work}/signed.log" \
  || fail "the signature was not reported as the origin:
$(cat "${work}/signed.log")"
[ -x "${work}/signed/bin/uf" ] || fail "signed: uf was not linked"
#     And the identity it pinned is anchored at both ends: the repository, and
#     the workflow file inside it. An identity that matched any workflow in the
#     repository, or any repository, would verify a signature uf did not make.
grep -q 'yml@$' "${work}/cosign-args" \
  || fail "the certificate identity was not anchored past the workflow file:
$(cat "${work}/cosign-args")"
grep -qF 'workflows/release' "${work}/cosign-args" \
  || fail "the certificate identity did not name the release workflow:
$(cat "${work}/cosign-args")"
grep -qF "${UF_REPO:-ubugeeei-prod/uf}" "${work}/cosign-args" \
  || fail "the certificate identity did not name the repository:
$(cat "${work}/cosign-args")"
pass "a signature that verifies establishes origin, and the identity is pinned"

# 13. The attack the signature is for: a release host that serves an archive, a
#     matching checksum, and a signature that is not uf's. Every local check
#     agrees with itself. Only the identity disagrees, and that is enough.
stub_bad="${work}/stub-bad"
mkdir -p "$stub_bad"
cat > "${stub_bad}/cosign" <<'STUB'
#!/bin/sh
echo "Error: no matching signatures" >&2
exit 1
STUB
chmod +x "${stub_bad}/cosign"

if run_installer wrongly_signed UF_VERSION=signed PATH="${stub_bad}:${PATH}" \
  >"${work}/wrongly-signed.log" 2>&1; then
  fail "an archive signed by somebody else was installed"
fi
grep -q "would not verify the signature" "${work}/wrongly-signed.log" \
  || fail "the refusal did not say the signature was the reason:
$(cat "${work}/wrongly-signed.log")"
[ -e "${work}/wrongly_signed/bin/uf" ] && fail "wrongly_signed: uf was linked anyway"
#     And it refuses under the default, not only under `require`: a signature
#     that is present and wrong is never a warning.
pass "an archive signed by somebody else is refused under the default setting"

echo "test-install: all cases passed"
