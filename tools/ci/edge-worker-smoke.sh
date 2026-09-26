#!/bin/sh
# Build uf's edge deployments and ask Wrangler's local runtime (workerd) to
# answer them over HTTP.
#
# The in-process library tests check the adapter modules uf owns. This smoke
# starts the worker entry `uf build --adapter edge` writes, with the
# `wrangler.json` and asset binding it writes beside it, so the Edge support row
# is backed by a Worker runtime rather than by Node calling the handler. Four
# Workers, one after another:
#
#   1. the served-app fixture: a prerendered page, a dynamic render, both route
#      handler methods, the not-found boundary, a hashed client asset, and the
#      access line every request leaves — with its request id, and filed at the
#      level it was written at;
#   2. the rsc-split-app fixture: a server action, called the way the client
#      reference calls it, and the same action refused from another origin;
#   3. the isr-app fixture: a page that regenerates, answered first with the
#      document the build wrote and then, past its lifetime, with one the
#      Worker rendered, which a restarted Worker reads back out of Workers KV,
#      and rendered by a Worker started after an invalidation rather than
#      answered with the build's document;
#   4. a probe built from `npm/vite/internal/worker-builtins.js`: every
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

# Every process under <pid>, parents before children: `npx` starts Wrangler, and
# Wrangler starts workerd.
descendants() {
  for child in $(pgrep -P "$1" 2>/dev/null); do
    echo "$child"
    descendants "$child"
  done
}

# Stop the Worker `start_worker` began, and everything under it. The process it
# holds is the subshell around `npx`, and signalling that alone left npm,
# Wrangler and workerd serving: measured locally, a run left every Worker it
# started behind, which also puts a restarted Worker beside the one it replaced.
# Wrangler does not always stop on the first signal, so whatever is still alive
# a few seconds later is killed.
stop_worker() {
  if [ -n "$server_pid" ]; then
    children="$(descendants "$server_pid")"
    # shellcheck disable=SC2086
    kill "$server_pid" $children >/dev/null 2>&1 || true
    wait "$server_pid" >/dev/null 2>&1 || true
    alive=""
    for _ in 1 2 3 4 5; do
      alive=""
      for pid in $children; do
        if kill -0 "$pid" >/dev/null 2>&1; then
          alive="$alive $pid"
        fi
      done
      [ -n "$alive" ] || break
      sleep 1
    done
    if [ -n "$alive" ]; then
      # shellcheck disable=SC2086
      kill -9 $alive >/dev/null 2>&1 || true
    fi
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

# 3. A page that regenerates, on the fixture the regeneration tests use. It
#    states a one-second lifetime, so the first answer is the document the
#    build wrote, a later one is a document the Worker rendered with no rebuild,
#    and what it rendered is kept in the KV namespace the build bound, so a
#    restarted Worker reads it back rather than the build's.
regenerating="crates/uf_cli/tests/fixtures/isr-app"
"$uf_binary" --cwd "$regenerating" build --adapter edge
grep -F '"UF_CACHE"' "$regenerating/.uf/deploy/edge/wrangler.json" >/dev/null 2>&1 ||
  fail "the isr-app edge build did not bind the KV namespace a regenerated page is kept in"

# The instant a document of that fixture says it was rendered at.
rendered_instant() {
  sed -n 's/.*rendered at \([0-9][0-9]*\).*/\1/p' "$1" | head -n 1
}

built="$(rendered_instant "$regenerating/.uf/deploy/edge/static/__uf/regenerate/clock/index.html")"
[ -n "$built" ] || fail "the isr-app build wrote no regenerating document for /clock"
start_worker "$regenerating/.uf/deploy/edge" isr-app

assert_response GET /clock 200 "rendered at $built"

# Past the lifetime a reader is still answered, and one regeneration runs behind
# it. Polled with a bound rather than slept for, because the regeneration is a
# render in a local Worker and how long it takes is not this script's to promise.
regenerated=""
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30; do
  sleep 1
  assert_status GET /clock 200
  answered="$(rendered_instant "$work/body")"
  if [ -n "$answered" ] && [ "$answered" != "$built" ]; then
    regenerated="$answered"
    break
  fi
done
[ -n "$regenerated" ] || {
  dump_wrangler_log
  fail "GET /clock still answered the build's document 30 seconds past its one-second lifetime"
}
[ "$regenerated" -gt "$built" ] ||
  fail "GET /clock regenerated to an instant before the build's: $regenerated"
if grep -F "a cache refresh failed" "$log" >/dev/null 2>&1; then
  dump_wrangler_log
  fail "the worker logged a failed regeneration"
fi
pass "GET /clock regenerated after its lifetime, with no rebuild"

# Kept in KV rather than in the isolate: a restarted Worker starts with empty
# memory, and answers with a document a Worker rendered rather than the build's.
stop_worker
start_worker "$regenerating/.uf/deploy/edge" isr-app-restarted
assert_status GET /clock 200
restarted="$(rendered_instant "$work/body")"
if [ -z "$restarted" ] || [ "$restarted" = "$built" ]; then
  dump_wrangler_log
  fail "a restarted Worker answered /clock with the build's document, so the regenerated one was not kept in KV"
fi
pass "a restarted Worker reads the regenerated /clock back out of KV"

# An invalidation is kept in KV too. A fresh Worker invalidates the page's tag
# before anything asks for the page, so no regeneration is running behind it,
# and is stopped. The next Worker finds no regenerated page in KV and a build's
# document older than the invalidation, so it renders the page.
stop_worker
start_worker "$regenerating/.uf/deploy/edge" isr-app-invalidating
assert_response POST /api/revalidate 200 '"expired"'
stop_worker
start_worker "$regenerating/.uf/deploy/edge" isr-app-after-invalidation
assert_status GET /clock 200 -D "$work/headers"
invalidated="$(rendered_instant "$work/body")"
if [ -z "$invalidated" ] || [ "$invalidated" = "$built" ] ||
  ! grep -i '^x-uf-cache: miss' "$work/headers" >/dev/null 2>&1; then
  dump_wrangler_log
  fail "a Worker started after /clock was invalidated did not render it (instant ${invalidated:-none}, the build's $built, $(grep -i '^x-uf-cache' "$work/headers"))"
fi
pass "a Worker started after /clock was invalidated renders it rather than the build's document"
stop_worker

# 4. The Node built-ins a Worker provides only as stubs, measured again.
probe="$work/builtins"
mkdir -p "$probe"
PROBE_DIRECTORY="$probe" TABLE="$repo_root/npm/vite/internal/worker-builtins.js" \
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
  fail "a module npm/vite/internal/worker-builtins.js calls a stub works on this Worker; measure again and update the table"
fi
[ "$(grep -c "	stub	" "$work/body" || true)" -gt 0 ] || fail "the built-ins probe answered nothing"
pass "every Node built-in the table names is still a stub at its compatibility date"
stop_worker

echo "edge-worker-smoke: ok"
