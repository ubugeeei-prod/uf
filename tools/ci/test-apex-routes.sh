#!/bin/sh
# `apex-routes.sh` against workers it can safely make wrong.
#
# A check on a redirect table is only worth the job it runs in if it fails when
# the table is wrong, and the failure mode of one that does not is silence:
# every deploy is green and somebody else finds the 404, months later, from a
# link in an issue.
#
# So each rule is planted here as the defect it is meant to catch. The planted
# workers are wrappers: each one imports the real worker, takes its answer, and
# breaks exactly one thing about it. That is deliberate — a hand-written stub
# would drift away from the real handler and end up testing a worker nobody
# deploys, and the interesting failures here are all *one* change away from
# correct. A worker that drops the query string from a redirect is otherwise
# perfect, which is exactly why nobody notices it.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$repo_root"

script="$repo_root/tools/ci/apex-routes.sh"
real="$repo_root/infra/cloudflare/workers/root.js"
brand="$repo_root/brand"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-apex-routes.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
  echo "test-apex-routes: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

# A wrapper around the real worker, with `__REAL__` replaced by its path. The
# body arrives on stdin and runs with `response` and `request` in scope; what it
# returns is what the planted worker answers.
plant() {
  name="$1"
  cat > "$work/$name.js" <<PLANT
import real from "__REAL__";

export default {
  async fetch(request, env, ctx) {
    const response = await real.fetch(request, env, ctx);
    return await (async () => {
$(cat)
    })();
  },
};
PLANT
  # `sed` rather than interpolating the path into the heredoc: a repository
  # checked out under a directory whose name holds a backslash would otherwise
  # write a string literal that does not parse.
  sed -i.bak "s|__REAL__|file://$real|" "$work/$name.js"
  rm -f "$work/$name.js.bak"
}

# The checker must fail on this worker, and its output must say why in a way
# somebody can act on.
expect_fail() {
  name="$1"
  needle="$2"
  if "$script" --worker "$work/$name.js" --brand "$brand" > "$work/$name.out" 2>&1; then
    cat "$work/$name.out" >&2
    fail "$name passed and it is broken"
  fi
  if ! grep -q -- "$needle" "$work/$name.out"; then
    cat "$work/$name.out" >&2
    fail "$name failed, but nothing in the output said \`$needle\`"
  fi
  pass "$name"
}

# --- The worker that is deployed ------------------------------------------
#
# First, so that a failure below is a statement about the planted defect rather
# than about the repository being red already.
if ! "$script" > "$work/real.out" 2>&1; then
  cat "$work/real.out" >&2
  fail "the real worker does not pass its own check"
fi
pass "the deployed worker passes"

# --- 1. The apex stops being a page ---------------------------------------
#
# The nine-line worker this replaced. Somebody who types the domain lands in
# the middle of the manual, and every fact on the front door goes with it.
plant front-door <<'CASE'
const url = new URL(request.url);
if (url.pathname === "/") {
  return new Response(null, { status: 308, headers: { location: "https://docs.uniflowed.dev/" } });
}
return response;
CASE
expect_fail front-door "the apex is supposed to be a page"

# --- 2. A redirect that drops the query string ----------------------------
#
# `?from=readme` is how somebody finds out where a visitor came from, and a
# campaign link that silently loses it looks like it worked.
plant drops-the-query <<'CASE'
const location = response.headers.get("location");
if (location == null) return response;
const target = new URL(location);
target.search = "";
const headers = new Headers(response.headers);
headers.set("location", target.toString());
return new Response(null, { status: response.status, headers });
CASE
expect_fail drops-the-query "a redirect that rewrites the path resolves to the wrong page"

# --- 3. A redirect that rewrites the path ---------------------------------
#
# The worst of the three, because every link resolves — to the documentation
# home page, whatever it was for.
plant rewrites-the-path <<'CASE'
if (response.headers.get("location") == null) return response;
const headers = new Headers(response.headers);
headers.set("location", "https://docs.uniflowed.dev/");
return new Response(null, { status: response.status, headers });
CASE
expect_fail rewrites-the-path "a redirect that rewrites the path resolves to the wrong page"

# --- 4. The fallthrough stops falling through -----------------------------
#
# What an apex that serves a landing page and forgets it used to serve a whole
# manual looks like from outside.
plant no-fallthrough <<'CASE'
const url = new URL(request.url);
if (response.status === 308 && !url.hostname.startsWith("www.")) {
  return new Response("not found\n", { status: 404 });
}
return response;
CASE
expect_fail no-fallthrough "rather than a 308 to the documentation host"

# --- 5. `www` redirects to itself -----------------------------------------
#
# A loop on the front door is a site that is down, and it is one character away
# from the line that is there.
plant www-loop <<'CASE'
const url = new URL(request.url);
if (!url.hostname.startsWith("www.")) return response;
const headers = new Headers(response.headers);
headers.set("location", "https://www.uniflowed.dev" + url.pathname);
return new Response(null, { status: 308, headers });
CASE
expect_fail www-loop "which is a loop"

# --- 6. The page reaches off its own origin -------------------------------
#
# `default-src 'none'` with `img-src 'self'` means the browser blocks it and
# nothing on the server says a word.
plant off-origin-image <<'CASE'
if (!(response.headers.get("content-type") ?? "").startsWith("text/html")) return response;
const html = (await response.text()).replace(
  "/brand/uniflowed-mark.svg",
  "https://cdn.example.invalid/mark.svg",
);
return new Response(html, { status: response.status, headers: response.headers });
CASE
expect_fail off-origin-image "The browser would block it"

# --- 7. The page loads a file that is not in `brand/` ---------------------
#
# Same-origin, allowed by the policy, and a broken image. This is the one a
# rename produces.
plant missing-image <<'CASE'
if (!(response.headers.get("content-type") ?? "").startsWith("text/html")) return response;
const html = (await response.text()).replace("/brand/uniflowed-mark.svg", "/brand/no-such-mark.svg");
return new Response(html, { status: response.status, headers: response.headers });
CASE
expect_fail missing-image "is at that name"

# --- 8. The stylesheet drifts away from its own hash ----------------------
#
# The worker derives the hash from the stylesheet at runtime so that this
# cannot happen. If somebody ever replaces that with a literal, the failure is
# the front door with no styles at all — so the check recomputes rather than
# trusts.
plant stale-style-hash <<'CASE'
if (!(response.headers.get("content-type") ?? "").startsWith("text/html")) return response;
const html = (await response.text()).replace("<style>", "<style>/* one more rule */");
return new Response(html, { status: response.status, headers: response.headers });
CASE
expect_fail stale-style-hash "serve the front door unstyled"

# --- 9. A script appears on a page whose policy forbids one ---------------
plant a-script <<'CASE'
if (!(response.headers.get("content-type") ?? "").startsWith("text/html")) return response;
const html = (await response.text()).replace("</body>", "<script>void 0;</script></body>");
return new Response(html, { status: response.status, headers: response.headers });
CASE
expect_fail a-script "so it would not run"

# --- 10. The response headers go away on one branch -----------------------
#
# One branch, not all of them: a check that only looks at `/` is a check that
# lets the redirect rot.
plant headers-on-one-branch <<'CASE'
if (response.status !== 308) return response;
const headers = new Headers(response.headers);
headers.delete("content-security-policy");
headers.delete("referrer-policy");
return new Response(null, { status: response.status, headers });
CASE
expect_fail headers-on-one-branch "sent no \`content-security-policy\`"

# --- 11. The policy opens back up -----------------------------------------
plant unsafe-inline <<'CASE'
const headers = new Headers(response.headers);
const policy = headers.get("content-security-policy");
if (policy != null) {
  headers.set("content-security-policy", policy.replace(/style-src [^;]*/, "style-src 'unsafe-inline'"));
}
return new Response(response.body, { status: response.status, headers });
CASE
expect_fail unsafe-inline "it can be named by its hash"

echo "test-apex-routes: ok"
