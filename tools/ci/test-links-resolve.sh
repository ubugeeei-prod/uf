#!/bin/sh
# `links-resolve.sh` against sites it can safely make wrong.
#
# A link checker is only worth the job it runs in if it fails on a dead link,
# and the failure mode of one that does not is silence: every page renders,
# every check is green, and a reader finds the 404. So each rule is planted
# here as the defect it is meant to catch.
#
# The sites are written by hand rather than built, and that is deliberate: this
# is a test of the checker, not of `uf build`. A planted site is three files
# and takes milliseconds, and it can hold shapes a real build would never
# produce — which is exactly what a check has to survive.
set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)"
script="$repo_root/tools/ci/links-resolve.sh"

work="$(mktemp -d "${TMPDIR:-/tmp}/uf-test-links-resolve.XXXXXX")"
trap 'rm -rf "$work"' EXIT INT TERM

fail() {
  echo "test-links-resolve: FAIL: $*" >&2
  exit 1
}

pass() {
  echo "  ok  $*"
}

site="$work/site"
source_dir="$work/app"

# A site whose links all resolve, rebuilt for each case. Two pages, an asset,
# an anchor, and a sitemap that agrees with all of it — plus the sources the
# pages were rendered from, so the checker has somewhere to point when it
# reports a bad link.
scratch() {
  rm -rf "$site" "$source_dir"
  mkdir -p "$site/guide/state" "$site/guide/effect" "$site/assets" \
    "$source_dir/guide/state" "$source_dir/guide/effect" "$source_dir/_design"

  cat > "$site/index.html" <<'HTML'
<!doctype html>
<html><head><link rel="stylesheet" href="/assets/site.css"></head>
<body><a href="/guide/state">State</a><a href="/guide/effect/">Effect</a></body></html>
HTML

  cat > "$site/guide/state/index.html" <<'HTML'
<!doctype html>
<html><body>
<h2 id="the-layer-below">The layer below</h2>
<a href="/guide/effect#doing-things">Effects</a>
<a href="../effect">Next</a>
<a href="#the-layer-below">Back to top</a>
<a href="mailto:security@example.invalid">Report</a>
<img src="/assets/diagram.svg" alt="">
</body></html>
HTML

  cat > "$site/guide/effect/index.html" <<'HTML'
<!doctype html>
<html><body>
<h2 id="doing-things">Doing things</h2>
<a href="https://example.invalid/not-fetched-without---external">Elsewhere</a>
</body></html>
HTML

  printf 'body { color: red }\n' > "$site/assets/site.css"
  printf '<svg xmlns="http://www.w3.org/2000/svg"></svg>\n' > "$site/assets/diagram.svg"

  cat > "$site/sitemap.xml" <<'XML'
<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
<url><loc>https://docs.example.invalid/</loc></url>
<url><loc>https://docs.example.invalid/guide/state</loc></url>
<url><loc>https://docs.example.invalid/guide/effect</loc></url>
</urlset>
XML

  # The sources. `writtenAt` searches these to name the file a person edits,
  # so what matters is that the href text appears in them.
  printf -- '---\ntitle: State\n---\n\nSee [Effects](/guide/effect#doing-things).\n' \
    > "$source_dir/guide/state/_uf.page.mdx"
  printf -- '---\ntitle: Effect\n---\n\nNothing here.\n' \
    > "$source_dir/guide/effect/_uf.page.mdx"
  printf 'export const pages = [{ href: "/guide/state" }];\n' \
    > "$source_dir/_design/nav.js"
}

run() {
  sh "$script" --site "$site" --source "$source_dir" "$@" >"$work/out" 2>&1
}

# 1. The shape it has to pass on. Everything below depends on this: a checker
#    that failed here would "catch" every planted defect for the wrong reason.
scratch
if ! run; then
  cat "$work/out" >&2
  fail "rejected a site in which every link resolves"
fi
pass "passes a site whose links all resolve"

# 2. The whole point: a link to a route the build did not write. `/guide/ci`
#    was added this week; a page still pointing at the name it had before would
#    render perfectly and 404 on the click.
scratch
sed 's|/guide/effect#doing-things|/guide/moved|' "$site/guide/state/index.html" \
  > "$site/guide/state/index.html.new"
mv "$site/guide/state/index.html.new" "$site/guide/state/index.html"
if run; then
  fail "accepted a link to a route the build did not write"
fi
grep -q "no route" "$work/out" || fail "did not say the route is missing"
pass "rejects a link to a route nothing serves"

# 3. And it names the file a person edits, not only the built document. A
#    report that says `dist/guide/state/index.html:3` sends somebody to
#    generated markup.
scratch
sed 's|/guide/effect#doing-things|/guide/moved|' "$site/guide/state/index.html" \
  > "$site/guide/state/index.html.new"
mv "$site/guide/state/index.html.new" "$site/guide/state/index.html"
sed 's|/guide/effect#doing-things|/guide/moved|' "$source_dir/guide/state/_uf.page.mdx" \
  > "$source_dir/guide/state/_uf.page.mdx.new"
mv "$source_dir/guide/state/_uf.page.mdx.new" "$source_dir/guide/state/_uf.page.mdx"
run && fail "accepted the moved route"
grep -q "_uf.page.mdx:5" "$work/out" \
  || fail "did not name the source file and line the link is written on"
pass "names the source file and line a bad link is written on"

# 4. An anchor the target page does not render. This one only exists in the
#    output: the id is the slugger's, so the source text can be correct and the
#    link still land at the top of the page.
scratch
sed 's|id="doing-things"|id="doing-other-things"|' "$site/guide/effect/index.html" \
  > "$site/guide/effect/index.html.new"
mv "$site/guide/effect/index.html.new" "$site/guide/effect/index.html"
if run; then
  fail "accepted an anchor no heading in the target page rendered"
fi
grep -q "no anchor" "$work/out" || fail "did not say the anchor is missing"
pass "rejects an anchor the target page does not render"

# 5. An asset the build did not write. A stylesheet or an image is a link like
#    any other, and a missing one is a page that renders wrong rather than a
#    page that 404s — which is harder to notice, not easier.
scratch
rm "$site/assets/diagram.svg"
if run; then
  fail "accepted an <img src> pointing at a file the build did not write"
fi
pass "rejects an asset the build did not write"

# 6. A `<loc>` for a route nothing serves. `sitemap.xml` is generated from the
#    documents that were prerendered, so this is one build disagreeing with
#    itself — which is what a moved page looks like from the outside.
scratch
sed 's|/guide/effect</loc>|/guide/effects</loc>|' "$site/sitemap.xml" > "$site/sitemap.xml.new"
mv "$site/sitemap.xml.new" "$site/sitemap.xml"
if run; then
  fail "accepted a sitemap <loc> for a route nothing serves"
fi
pass "rejects a sitemap entry for a route nothing serves"

# 7. And the other direction: a page that was prerendered and is in no `<loc>`.
#    A crawler handed the sitemap never reaches it, which is how a third of a
#    manual goes missing without a single broken link.
scratch
grep -v 'guide/effect' "$site/sitemap.xml" > "$site/sitemap.xml.new"
mv "$site/sitemap.xml.new" "$site/sitemap.xml"
if run; then
  fail "accepted a prerendered page that is in no <loc>"
fi
pass "rejects a prerendered page missing from the sitemap"

# 7b. Except a page behind a `_uf.middleware.js`. `uf build` leaves a guarded
#     page out of the sitemap on purpose — a guard says the route is not for
#     everyone and a sitemap is a submission to search engines — so a check
#     that demanded it would go red for the guard doing its job.
scratch
grep -v 'guide/effect' "$site/sitemap.xml" > "$site/sitemap.xml.new"
mv "$site/sitemap.xml.new" "$site/sitemap.xml"
printf 'export function middleware() {}\n' > "$source_dir/guide/effect/_uf.middleware.js"
run || fail "demanded a sitemap entry for a page behind a middleware"
pass "does not demand a sitemap entry for a guarded page"

# 8. Without `--external` nothing is fetched. The planted host does not exist,
#    so a run that tried would either fail or hang — and either would be the
#    check teaching people to ignore it.
scratch
run || fail "an offline run tried to resolve an http link"
grep -q "1 external" "$work/out" || fail "did not count the external link it left alone"
pass "leaves external links alone without --external"

# 8b. And the promise `--external` makes: a host that does not answer is
#     *unverified* and passes. This is the whole reason the network pass is a
#     schedule rather than a gate, so it is the half worth a test — a check
#     that went red because a host was down would teach people to re-run it
#     until it went green.
#
#     Port 1 on the loopback interface, where a connection is refused
#     immediately and everywhere: this test opens no socket of its own, which
#     it could not do in a sandbox anyway. The other branch — a real `404`
#     answered by a real server — needs a server, and a test that starts one
#     is a test that fails wherever binding a port is not allowed.
scratch
sed 's|https://example.invalid/not-fetched-without---external|http://127.0.0.1:1/nothing|' \
  "$site/guide/effect/index.html" > "$site/guide/effect/index.html.new"
mv "$site/guide/effect/index.html.new" "$site/guide/effect/index.html"
run --external || fail "a host that did not answer failed the check"
grep -q "unverified" "$work/out" || fail "did not report the unanswered host as unverified"
pass "an unanswered host is unverified under --external, and passes"

# 9. A relative link resolves against the page it is on rather than the site
#    root, and `mailto:` is not uf's to resolve. Both are in the passing site
#    above; this is the case that would notice if either stopped being true.
scratch
sed 's|href="../effect"|href="../gone"|' "$site/guide/state/index.html" \
  > "$site/guide/state/index.html.new"
mv "$site/guide/state/index.html.new" "$site/guide/state/index.html"
if run; then
  fail "accepted a relative link that resolves to nothing"
fi
grep -q "/guide/gone" "$work/out" \
  || fail "did not resolve the relative link against the page it is on"
pass "resolves a relative link against the page it is on"

# 10. A site directory that does not exist is a mistake in the *invocation*,
#     and reporting "no broken links" for it would be the worst answer of all.
if sh "$script" --site "$work/never-built" --source "$source_dir" >"$work/out" 2>&1; then
  fail "reported success over a site directory that does not exist"
fi
pass "refuses a site directory that was never built"

echo "test-links-resolve: ok"
