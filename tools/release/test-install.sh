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

work="$(mktemp -d)"
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

# 4. A tampered archive must be rejected, not installed.
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

# 5. An archive whose members escape the extraction root must be rejected even
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
grep -q "paths outside the archive root" "${work}/evil.log" \
  || fail "escaping archive was rejected, but not by the path guard:
$(cat "${work}/evil.log")"
[ -e "${work}/escaped" ] && fail "evil: a member escaped the extraction root"
pass "an archive with members outside the root is rejected"

# 6. A version that does not exist must fail loudly rather than leave a broken
#    install behind.
if run_installer missing UF_VERSION=99.99.99 >"${work}/missing.log" 2>&1; then
  fail "a nonexistent version reported success"
fi
[ -e "${work}/missing/bin/uf" ] && fail "missing: uf was linked anyway"
pass "a nonexistent version fails and installs nothing"

echo "test-install: all cases passed"
