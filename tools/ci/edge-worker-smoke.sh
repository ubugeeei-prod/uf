#!/bin/sh
# Build uf's edge deployments and ask Wrangler's local runtime (workerd) to
# answer them over HTTP.
#
# The in-process library tests check the adapter modules uf owns. This smoke
# starts the worker entry `uf build --adapter edge` writes, with the
# `wrangler.json` and asset binding it writes beside it, so the Edge support row
# is backed by a Worker runtime rather than by Node calling the handler. Three
# Workers, one after another:
#
#   1. the served-app fixture: a prerendered page, a dynamic render, both route
#      handler methods, the not-found boundary, a hashed client asset, and the
#      access line every request leaves — with its request id, and filed at the
#      level it was written at;
#   2. the rsc-split-app fixture: a server action, called the way the client
#      reference calls it, and the same action refused from another origin;
#   3. a probe built from `packages/vite/internal/worker-builtins.js`: every
#      Node built-in that table says a Worker provides only as a stub must still
#      throw, or the warnings `uf build --adapter edge` prints are wrong.
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

wrangler_version="4.128.0"
work="$(mktemp -d "${TMPDIR:-/tmp}/uf-edge-worker-smoke.XXXXXX")"
log=""
server_pid=""

stop_worker() {
  if [ -n "$server_pid" ] && kill -0 "$server_pid" >/dev/null 2>&1; then
    kill "$server_pid" >/dev/null 2>&1 || true
    wait "$server_pid" >/dev/null 2>&1 || true
  fi
  server_pid=""
}

cleanup() {
  stop_worker
  rm -rf "$work"
}
trap cleanup EXIT INT TERM

dump_wrangler_log() {
  if [ -n "$log" ] && [ -f "$log" ]; then
    # The tail, because Wrangler opens with a listing of its local explorer
    # routes and the lines that explain a failure are the last ones.
    echo "---- $(basename "$log") ----" >&2
    tail -n 120 "$log" >&2 || true
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

# Start `wrangler dev --local` in <directory> on a fresh port and wait until it
# answers. Sets `base`, `log` and `server_pid`. The log level is Wrangler's
# default rather than `error`, because what the Worker writes at `info` is part
# of what this smoke checks.
start_worker() {
  directory="$1"
  name="$2"
  port="$(pick_port)"
  base="http://127.0.0.1:$port"
  log="$work/$name.log"
  (
    cd "$directory"
    CI=1 WRANGLER_SEND_METRICS=false npx --yes "wrangler@$wrangler_version" dev \
      --local \
      --config wrangler.json \
      --ip 127.0.0.1 \
      --port "$port" \
      --show-interactive-dev-session=false
  ) >"$log" 2>&1 &
  server_pid=$!

  ready=0
  for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 \
    31 32 33 34 35 36 37 38 39 40 41 42 43 44 45; do
    if ! kill -0 "$server_pid" >/dev/null 2>&1; then
      dump_wrangler_log
      fail "wrangler dev exited before the $name worker was reachable"
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
    fail "the $name worker did not become reachable at $base"
  }
}

# <method> <path> <status> — the status alone, for an answer whose body is not
# the application's to describe.
assert_status() {
  method="$1"
  path="$2"
  expected_status="$3"
  shift 3

  if ! status="$(curl -sS --connect-timeout 2 --max-time 15 -o "$work/body" -w '%{http_code}' -X "$method" "$@" "$base$path")"; then
    dump_wrangler_log
    fail "$method $path did not complete"
  fi
  if [ "$status" != "$expected_status" ]; then
    echo "---- response body ----" >&2
    sed -n '1,120p' "$work/body" >&2 || true
    echo "-----------------------" >&2
    dump_wrangler_log
    fail "$method $path returned $status, expected $expected_status"
  fi
}

assert_response() {
  method="$1"
  path="$2"
  expected_status="$3"
  expected_text="$4"
  shift 4

  assert_status "$method" "$path" "$expected_status" "$@"
  if ! grep -F -- "$expected_text" "$work/body" >/dev/null 2>&1; then
    echo "---- response body ----" >&2
    sed -n '1,120p' "$work/body" >&2 || true
    echo "-----------------------" >&2
    dump_wrangler_log
    fail "$method $path did not contain $expected_text"
  fi

  pass "$method $path"
}

# 1. The served-app fixture.
fixture="crates/uf_cli/tests/fixtures/served-app"
"$uf_binary" --cwd "$fixture" build --adapter edge
start_worker "$fixture/.uf/deploy/edge" served-app

assert_response GET / 200 "served-app home"
assert_response GET /guide 200 "served-app guide"
assert_response GET /posts/wrangler 200 "post: wrangler"
assert_response GET /api/health 200 '"status":"ok"'
assert_response POST /api/health 200 '"echoed":"wrangler"' \
  -H 'content-type: application/json' \
  --data '{"name":"wrangler"}'
assert_response GET /missing 404 "served-app has no such page"

# A hashed script the client build wrote, which only the assets binding can
# answer: the Worker asks the binding first, and a script is never a document.
asset="$(cd "$fixture/.uf/deploy/edge/static" && ls assets/*.js 2>/dev/null | head -n 1 || true)"
[ -n "$asset" ] || fail "the edge build wrote no client script under static/assets"
if ! curl -sS --connect-timeout 2 --max-time 15 -D "$work/headers" -o "$work/body" "$base/$asset" >/dev/null; then
  dump_wrangler_log
  fail "GET /$asset did not complete"
fi
grep -i '^content-type:.*javascript' "$work/headers" >/dev/null 2>&1 || {
  sed -n '1,20p' "$work/headers" >&2 || true
  fail "GET /$asset was not answered as a script"
}
pass "GET /$asset"

# The line every request leaves: an id uf generated, the route that matched,
# and the status — filed at `info` for a 200. The default sink writes every
# level to `console.error`, and a Worker's log store reads the method, so this is
# where a Worker whose every request succeeded used to read as failing.
sleep 1
access_line="$(grep -F 'path=/api/health' "$log" | grep -F 'status=200' | head -n 1 || true)"
[ -n "$access_line" ] || {
  dump_wrangler_log
  fail "the worker's log has no access line for GET /api/health"
}
case "$access_line" in
  *requestId=????????-????-????-????-????????????*) ;;
  *)
    fail "the access line for /api/health carries no request id: $access_line"
    ;;
esac
case "$access_line" in
  *ERROR*)
    fail "an info access line was filed as an error: $access_line"
    ;;
esac
pass "the access line carries a request id and is filed at info"
stop_worker

# 2. A server action, on the fixture the action tests use.
actions="crates/uf_cli/tests/fixtures/rsc-split-app"
"$uf_binary" --cwd "$actions" build --adapter edge
# The id is read from the build's own manifest, beside the output and never in
# it: it is keyed on a per-build secret, and the deployed artefact does not
# carry the manifest.
action_id="$(node -e '
const manifest = require(process.argv[1]);
const action = (manifest.serverActions || []).find((entry) => entry.export === "recordCount");
process.stdout.write(action ? action.id : "");
' "$repo_root/$actions/.uf/build/meta/uf-rsc-manifest.json")"
[ -n "$action_id" ] || fail "the rsc-split-app build names no recordCount action"
start_worker "$actions/.uf/deploy/edge" rsc-split-app

assert_response POST /counter 200 '"visitor":"ada"' \
  -H "origin: $base" \
  -H 'content-type: application/json' \
  -H "uf-action: $action_id" \
  -H 'cookie: visitor=ada' \
  --data '{"args":[4]}'
grep -F '"total":9' "$work/body" >/dev/null 2>&1 || fail "the action answered without its total"
assert_status POST /counter 403 \
  -H 'origin: http://evil.example' \
  -H 'content-type: application/json' \
  -H "uf-action: $action_id" \
  --data '{"args":[4]}'
pass "POST /counter from another origin is refused"
stop_worker

# 3. The Node built-ins a Worker provides only as stubs, measured again.
probe="$work/builtins"
mkdir -p "$probe"
PROBE_DIRECTORY="$probe" TABLE="$repo_root/packages/vite/internal/worker-builtins.js" \
  node --input-type=module <<'NODE'
import { writeFileSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

const { UNAVAILABLE_ON_WORKERS, WORKERS_COMPATIBILITY_DATE } = await import(
  pathToFileURL(process.env.TABLE).href
);
const imports = UNAVAILABLE_ON_WORKERS.map(
  (entry, at) => `import * as m${at} from ${JSON.stringify(`node:${entry.module}`)};`,
);
const probes = UNAVAILABLE_ON_WORKERS.map((entry, at) => {
  const args = JSON.stringify(entry.args);
  const call = entry.construct
    ? `new m${at}[${JSON.stringify(entry.member)}](...${args})`
    : `m${at}[${JSON.stringify(entry.member)}](...${args})`;
  return `[${JSON.stringify(entry.module)}, () => ${call}]`;
});
writeFileSync(
  path.join(process.env.PROBE_DIRECTORY, "worker.js"),
  `${imports.join("\n")}
const probes = [${probes.join(",\n")}];
export default {
  fetch() {
    const lines = [];
    for (const [name, probe] of probes) {
      try {
        probe();
        lines.push(name + "\\tprovided");
      } catch (error) {
        lines.push(name + "\\tstub\\t" + String(error && error.message));
      }
    }
    return new Response(lines.join("\\n") + "\\n");
  },
};
`,
);
writeFileSync(
  path.join(process.env.PROBE_DIRECTORY, "wrangler.json"),
  `${JSON.stringify(
    {
      name: "uf-builtins-probe",
      main: "./worker.js",
      compatibility_date: WORKERS_COMPATIBILITY_DATE,
      compatibility_flags: ["nodejs_compat"],
    },
    null,
    2,
  )}\n`,
);
NODE
start_worker "$probe" builtins-probe
assert_status GET / 200
if grep -F "	provided" "$work/body" >/dev/null 2>&1; then
  sed -n '1,40p' "$work/body" >&2 || true
  fail "a module packages/vite/internal/worker-builtins.js calls a stub works on this Worker; measure again and update the table"
fi
[ "$(grep -c "	stub	" "$work/body" || true)" -gt 0 ] || fail "the built-ins probe answered nothing"
pass "every Node built-in the table names is still a stub at its compatibility date"
stop_worker

echo "edge-worker-smoke: ok"
