#!/bin/sh
# Build the served-app fixture for Cloudflare Workers and ask Wrangler's local
# runtime to answer it over HTTP.
#
# The in-process library tests check the adapter modules uf owns. This smoke
# starts the worker entry `uf build --adapter edge` writes, with the
# `wrangler.json` and asset binding it writes beside it, so the Edge support
# row is backed by a Worker runtime rather than by Node calling the handler.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

fail() {
  echo "edge-worker-smoke: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

if [ "$#" -gt 1 ]; then
  fail "usage: tools/ci/edge-worker-smoke.sh [path-to-uf]"
fi

uf_binary="${UF_BINARY:-./target/release/uf}"
if [ "$#" -eq 1 ]; then
  uf_binary="$1"
fi

case "$uf_binary" in
  */*) ;;
  *) uf_binary="$(command -v "$uf_binary" || true)" ;;
esac

[ -n "$uf_binary" ] || fail "could not find the uf binary"
[ -x "$uf_binary" ] || fail "$uf_binary is not executable"
command -v node >/dev/null 2>&1 || fail "node is required to choose a local port"
command -v curl >/dev/null 2>&1 || fail "curl is required to smoke the worker"
command -v npx >/dev/null 2>&1 || fail "npx is required to run wrangler"

fixture="crates/uf_cli/tests/fixtures/served-app"
deploy_dir="$fixture/.uf/deploy/edge"
work="$(mktemp -d "${TMPDIR:-/tmp}/uf-edge-worker-smoke.XXXXXX")"
log="$work/wrangler.log"

server_pid=""
cleanup() {
  if [ -n "$server_pid" ] && kill -0 "$server_pid" >/dev/null 2>&1; then
    kill "$server_pid" >/dev/null 2>&1 || true
    wait "$server_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$work"
}
trap cleanup EXIT INT TERM

dump_wrangler_log() {
  if [ -f "$log" ]; then
    echo "---- wrangler.log ----" >&2
    sed -n '1,200p' "$log" >&2 || true
    echo "----------------------" >&2
  fi
}

pick_port() {
  node <<'NODE'
const net = require("node:net");
const server = net.createServer();
server.listen(0, "127.0.0.1", () => {
  const address = server.address();
  console.log(address.port);
  server.close();
});
NODE
}

port="${UF_EDGE_SMOKE_PORT:-$(pick_port)}"
base="http://127.0.0.1:$port"

"$uf_binary" --cwd "$fixture" build --adapter edge

(
  cd "$deploy_dir"
  CI=1 WRANGLER_SEND_METRICS=false npx --yes wrangler@4.128.0 dev \
    --local \
    --config wrangler.json \
    --ip 127.0.0.1 \
    --port "$port" \
    --log-level error \
    --show-interactive-dev-session=false
) >"$log" 2>&1 &
server_pid=$!

ready=0
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30; do
  if ! kill -0 "$server_pid" >/dev/null 2>&1; then
    dump_wrangler_log
    fail "wrangler dev exited before the worker was reachable"
  fi
  # Bounded, because curl has no overall timeout of its own: a connection
  # Wrangler accepts and the worker never answers would otherwise hold this
  # loop, and the job with it, until the runner gives up.
  if curl -sS --connect-timeout 2 --max-time 5 "$base/" >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 1
done

[ "$ready" -eq 1 ] || {
  dump_wrangler_log
  fail "worker did not become reachable at $base"
}

assert_response() {
  method="$1"
  path="$2"
  expected_status="$3"
  expected_text="$4"
  shift 4

  body="$work/body"
  if ! status="$(curl -sS --connect-timeout 2 --max-time 15 -o "$body" -w '%{http_code}' -X "$method" "$@" "$base$path")"; then
    dump_wrangler_log
    fail "$method $path did not complete"
  fi

  if [ "$status" != "$expected_status" ]; then
    echo "---- response body ----" >&2
    sed -n '1,120p' "$body" >&2 || true
    echo "-----------------------" >&2
    dump_wrangler_log
    fail "$method $path returned $status, expected $expected_status"
  fi

  if ! grep -F "$expected_text" "$body" >/dev/null 2>&1; then
    echo "---- response body ----" >&2
    sed -n '1,120p' "$body" >&2 || true
    echo "-----------------------" >&2
    dump_wrangler_log
    fail "$method $path did not contain $expected_text"
  fi

  pass "$method $path"
}

assert_response GET / 200 "served-app home"
assert_response GET /guide 200 "served-app guide"
assert_response GET /posts/wrangler 200 "post: wrangler"
assert_response GET /api/health 200 '"status":"ok"'
assert_response POST /api/health 200 '"echoed":"wrangler"' \
  -H 'content-type: application/json' \
  --data '{"name":"wrangler"}'
assert_response GET /missing 404 "served-app has no such page"

echo "edge-worker-smoke: ok"
