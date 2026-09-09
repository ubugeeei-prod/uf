// The front door: `uniflowed.dev`.
//
// This worker used to be nine lines. It set `url.hostname` to the docs host and
// returned a 308, so somebody who typed `uniflowed.dev` landed halfway down the
// manual with no idea what they had installed or whether they should. The zone
// carries real weight — `setup.` is what `curl … | sh` resolves, `releases.`
// holds the archives — and the apex was the one hostname with nothing behind
// it.
//
// So the apex answers three things itself and hands over everything else:
//
//   * `/` is a page. One document, no scripts, no webfonts, no CDN, no build
//     step: the markup and the stylesheet below are the whole of it, and the
//     only subresources are files that already live in `brand/`.
//   * `/robots.txt` is written here rather than borrowed. A redirect for
//     `robots.txt` resolves, but what it resolves to is the *documentation
//     host's* rules, which is a different origin with a different sitemap.
//   * `/brand/*` is served from `brand/` through the `ASSETS` binding, so the
//     mark on this page is same-origin and `img-src 'self'` can stay closed.
//     A name that is not in that directory falls through to the redirect, so
//     nothing that used to resolve stops resolving.
//
// **Everything else 308s to the documentation host exactly as it did before.**
// That is the compatibility rule and it is deliberately the default rather than
// a list: `/guide`, `/og.png`, `/brand/uf.png`, `/sitemap.xml` and every other
// path anybody has ever written down keep working because the fallthrough did
// not change, not because somebody remembered to enumerate them.
// `tools/ci/apex-routes.sh` drives this handler and fails when any of that
// stops being true.
//
// `www` redirects to the apex rather than serving the same bytes. One origin is
// canonical, and two hostnames serving one document split the cache, split the
// referrers, and need a `rel=canonical` afterwards to say which of them counts.
// A 308 is one hop, once, and browsers cache it. It is the same status the old
// worker sent; only the target changed.
//
// There is no JavaScript on the page and the policy below says so, which is the
// one place this deliberately does not copy the documentation site. That site
// has a theme toggle, so it needs an inline bootstrap to apply a stored choice
// before first paint. This page has no toggle — `localStorage` is per-origin,
// so a choice made on `docs.` would not be here to read anyway — and a page
// with nothing to remember can respect `prefers-color-scheme` in CSS and ship
// `script-src 'none'`.

/** Where every path this worker does not answer itself goes. */
const DOCS_ORIGIN = "https://docs.uniflowed.dev";

/**
 * The canonical origin, for the three absolute URLs a document cannot express
 * relatively: `rel=canonical`, `og:url` and `og:image`. Hard-coded rather than
 * read off the request because that is what those three fields mean — a card
 * scraped from a preview deployment should still name the real page.
 */
const SITE_ORIGIN = "https://uniflowed.dev";

const GITHUB = "https://github.com/ubugeeei-prod/uf";

/**
 * The stylesheet.
 *
 * A subset of `docs/app/_design/seam.css` with the same tokens and the same
 * refusals, so the front door and the manual read as one system: warm paper,
 * rules instead of cards, no shadow, no radius, no transition, and the brand
 * spectrum exactly once — down the first 9rem of the seam. `brand/tokens.css`
 * is inlined here rather than linked because a stylesheet the page cannot
 * render without is a round trip before first paint, and this is one document.
 *
 * Dark mode is `prefers-color-scheme` alone. The manual also honours a
 * `data-theme` attribute, because it has a control that sets one; this page has
 * no control, so the attribute would be a selector nothing can ever match.
 *
 * Every byte of this string is hashed into `style-src` at first request, so the
 * policy cannot go stale against the stylesheet it is a policy for.
 */
const STYLE = `
:root {
  color-scheme: light;

  --uf-color-cyan-500: #35d6f6;
  --uf-color-blue-500: #2677ff;
  --uf-color-violet-500: #8f4bff;
  --uf-color-magenta-500: #d84bff;
  --uf-font-display: "Satoshi", "Inter", ui-sans-serif, system-ui, sans-serif;
  --uf-font-body: "Inter", ui-sans-serif, system-ui, sans-serif;
  --uf-font-mono: "JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace;

  --doc-paper: #fbfaf8;
  --doc-ink: #16171a;
  --doc-ink-muted: #5b6068;
  --doc-ink-faint: #8b9098;
  --doc-rule: #e0ddd6;
  --doc-rule-strong: #c9c5bc;
  --doc-link: #1a56d6;
  --doc-link-visited: #6b3fb8;
  --doc-code-ground: #f4f2ee;
  --doc-selection: #dfe7fb;

  --doc-measure: 68ch;
  --doc-seam: 1px;
  --doc-gutter: clamp(1.25rem, 4vw, 3rem);
}

@media (prefers-color-scheme: dark) {
  :root {
    color-scheme: dark;

    --doc-paper: #0c0d0f;
    --doc-ink: #e8e6e1;
    --doc-ink-muted: #9aa0a8;
    --doc-ink-faint: #6a7078;
    --doc-rule: #24272c;
    --doc-rule-strong: #363a41;
    --doc-link: #7fb0ff;
    --doc-link-visited: #c0a2ff;
    --doc-code-ground: #131518;
    --doc-selection: #1d2c4d;
  }
}

html {
  background: var(--doc-paper);
  color: var(--doc-ink);
  font-family: var(--uf-font-body);
  font-size: 17px;
  line-height: 1.7;
  -webkit-text-size-adjust: 100%;
  text-rendering: optimizeLegibility;
}

body { margin: 0; min-width: 320px; }
::selection { background: var(--doc-selection); }

a {
  color: var(--doc-link);
  text-decoration-color: color-mix(in srgb, var(--doc-link) 35%, transparent);
  text-underline-offset: 0.18em;
}
a:hover { text-decoration-color: currentColor; }
a:visited { color: var(--doc-link-visited); }
strong { font-weight: 630; }
img { max-width: 100%; }

h1, h2 {
  font-family: var(--uf-font-display);
  font-weight: 640;
  letter-spacing: -0.021em;
  line-height: 1.2;
  margin: 0;
  text-wrap: balance;
}
h1 { font-size: clamp(2rem, 1.4rem + 2.4vw, 3.1rem); letter-spacing: -0.03em; }
h2 { font-size: 1.55rem; margin-block: 3.6rem 1.35rem; }
p { margin: 1.1em 0; }
h2 + p { margin-top: 0; }

code {
  font-family: var(--uf-font-mono);
  font-size: 0.87em;
  font-variant-ligatures: none;
}
:not(pre) > code {
  background: var(--doc-code-ground);
  border-radius: 3px;
  padding: 0.1em 0.32em;
  white-space: nowrap;
}

/* The skip link is visible the moment it is focused and nowhere else. */
.skip {
  background: var(--doc-paper);
  border: 1px solid var(--doc-rule-strong);
  clip-path: inset(50%);
  height: 1px;
  left: 0.5rem;
  overflow: hidden;
  padding: 0.5rem 0.9rem;
  position: absolute;
  top: 0.5rem;
  white-space: nowrap;
  width: 1px;
  z-index: 10;
}
.skip:focus-visible { clip-path: none; height: auto; width: auto; }

:focus-visible { outline: 2px solid var(--doc-link); outline-offset: 3px; }

.shell { margin: 0 auto; max-width: 82rem; padding: 0 var(--doc-gutter); }

.masthead {
  align-items: baseline;
  border-bottom: 1px solid var(--doc-rule);
  display: flex;
  flex-wrap: wrap;
  gap: 1.5rem;
  justify-content: space-between;
  padding: 1.4rem 0;
}
.masthead-brand {
  align-items: center;
  color: inherit;
  display: inline-flex;
  font-family: var(--uf-font-display);
  font-size: 1.05rem;
  font-weight: 660;
  gap: 0.55rem;
  letter-spacing: -0.02em;
  text-decoration: none;
}
.masthead-brand img { display: block; height: 22px; width: 22px; }
.masthead-brand .version {
  color: var(--doc-ink-faint);
  font-family: var(--uf-font-mono);
  font-size: 0.72rem;
  font-weight: 500;
  letter-spacing: 0;
}
.masthead-nav {
  align-items: baseline;
  display: flex;
  flex-wrap: wrap;
  font-size: 0.92rem;
  gap: 1.35rem;
}
.masthead-nav a { color: var(--doc-ink-muted); text-decoration: none; }
.masthead-nav a:hover { color: var(--doc-ink); }

/* The seam. One hairline down the page, carrying the brand spectrum over its
   first 9rem and nothing else anywhere. Content hangs off it. */
.seam { position: relative; padding-left: 1.75rem; }
.seam::before {
  background:
    linear-gradient(
      180deg,
      var(--uf-color-cyan-500) 0%,
      var(--uf-color-blue-500) 22%,
      var(--uf-color-violet-500) 48%,
      var(--uf-color-magenta-500) 66%,
      transparent 100%
    )
    no-repeat 0 0 / var(--doc-seam) 9rem,
    linear-gradient(var(--doc-rule), var(--doc-rule));
  bottom: 0;
  content: "";
  left: 0;
  position: absolute;
  top: 0;
  width: var(--doc-seam);
}
.seam-mark { position: relative; }
.seam-mark::before {
  background: var(--doc-paper);
  border: 1px solid var(--doc-rule-strong);
  border-radius: 50%;
  content: "";
  height: 7px;
  left: calc(-1.75rem - 3px);
  position: absolute;
  top: 0.62em;
  width: 7px;
}

.home { max-width: 64rem; }

.hero {
  align-items: center;
  column-gap: 3rem;
  display: flex;
  flex-wrap: wrap-reverse;
  justify-content: space-between;
  padding-block: 4.5rem 0;
}
.hero-copy { flex: 1 1 26rem; }
.hero-mark {
  flex: 0 1 auto;
  height: auto;
  margin-bottom: 1.5rem;
  width: clamp(9rem, 46vw, 22rem);
}
.hero h1 { margin-top: 0; }

/* The small mono label above a heading that says what kind of thing follows. */
.eyebrow {
  color: var(--doc-ink-faint);
  font-family: var(--uf-font-mono);
  font-size: 0.72rem;
  font-weight: 500;
  letter-spacing: 0.14em;
  margin: 0 0 0.75rem;
  text-transform: uppercase;
}

/* The one paragraph that says what this is. */
.lede {
  color: var(--doc-ink-muted);
  font-size: 1.32rem;
  line-height: 1.55;
  margin-top: 1rem;
  max-width: 46ch;
}

.install { padding-block: 0 1rem; }
.install p { max-width: var(--doc-measure); }

/* The prompt is drawn by CSS so that copying the line copies the command
   and not the dollar sign in front of it. */
.command {
  background: var(--doc-code-ground);
  border-left: 2px solid var(--uf-color-blue-500);
  display: flex;
  gap: 0.7rem;
  margin: 1.25rem 0;
  overflow-x: auto;
  padding: 0.8rem 1.1rem;
}
.command::before { color: var(--doc-ink-faint); content: "$"; flex: none; user-select: none; }
.command code { background: none; padding: 0; white-space: pre; }

.actions { align-items: center; display: flex; flex-wrap: wrap; gap: 1.4rem; margin-top: 2.2rem; }
.button {
  background: var(--doc-ink);
  border: 1px solid var(--doc-ink);
  color: var(--doc-paper);
  display: inline-block;
  font-size: 0.95rem;
  font-weight: 560;
  padding: 0.55rem 1.15rem;
  text-decoration: none;
}
.button:visited { color: var(--doc-paper); }
.button:hover { background: transparent; color: var(--doc-ink); }
.button-quiet { background: transparent; border-color: var(--doc-rule-strong); color: var(--doc-ink); }
.button-quiet:visited { color: var(--doc-ink); }
.button-quiet:hover { border-color: var(--doc-ink); }

.section { margin-top: 4.5rem; }
.section > h2 { margin-bottom: 1.35rem; }
.section > p { max-width: var(--doc-measure); }

/* Each row is a fact and the page that backs it. No icons, no cards — which is
   what a definition list is for. */
.facts { border-top: 1px solid var(--doc-rule); margin: 2.5rem 0 0; }
.facts > div {
  border-bottom: 1px solid var(--doc-rule);
  display: grid;
  gap: 0.35rem 2.5rem;
  padding: 1.35rem 0;
}
@media (min-width: 48rem) {
  .facts > div { grid-template-columns: 15rem minmax(0, 1fr); }
}
.facts dt {
  font-family: var(--uf-font-display);
  font-size: 1rem;
  font-weight: 620;
  letter-spacing: -0.015em;
}
.facts dd { color: var(--doc-ink-muted); margin: 0; max-width: 60ch; }
.facts dd a { white-space: nowrap; }

.colophon {
  border-top: 1px solid var(--doc-rule);
  color: var(--doc-ink-faint);
  display: flex;
  flex-wrap: wrap;
  font-size: 0.85rem;
  gap: 1.25rem;
  justify-content: space-between;
  margin-top: 5rem;
  padding: 1.6rem 0 3rem;
}
.colophon a { color: inherit; }

@media (max-width: 40rem) {
  html { font-size: 16px; }
  :root { --doc-gutter: 1.1rem; }
  .masthead { gap: 0.75rem 1.1rem; padding: 1rem 0; }
  .masthead-nav { font-size: 0.88rem; gap: 1rem; width: 100%; }
  .seam { padding-left: 1.1rem; }
  .seam-mark::before { left: calc(-1.1rem - 3px); }
  .hero { padding-block: 2rem 0; }
  .lede { font-size: 1.1rem; }
  /* A command on a phone is read by scrolling it, so it may use the whole
     screen rather than sitting inside the gutter. */
  .command {
    margin-inline: calc(-1 * var(--doc-gutter) - 1.1rem) calc(-1 * var(--doc-gutter));
    padding-inline: calc(var(--doc-gutter) + 1.1rem) var(--doc-gutter);
  }
  .colophon { gap: 0.6rem; margin-top: 3rem; }
}
`;

/**
 * The page.
 *
 * What it says, and why each part of it is there. A reader who types the domain
 * has two questions — *what is this* and *should I install it* — and the answer
 * to the second one here is mostly "not yet, and here is exactly why", because
 * that is true and every other document in this repository says so.
 *
 * It does not restate the documentation home page. That page argues the
 * toolchain: five claims, each with the page that proves it, and a pasted
 * `uf build` run. This one is a door — the sentence, the command, and the
 * shape of the word "alpha" — and its own section is the one the manual only
 * has room for a two-line notice about.
 *
 * No version number smaller than `0.0.0-alpha` appears anywhere on it. The
 * README's list of rough edges is scoped to a release on purpose and says so;
 * a second copy of it here is a second thing to keep true, and this one deploys
 * on a different schedule from the file it was copied from.
 */
const PAGE = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<title>uf — Unified Toolchain for Flow</title>
<meta name="description" content="uf runs, builds, tests, formats and lints a React application written in Flow, from one native binary. It is 0.0.0-alpha and nothing in it is stable yet.">
<link rel="canonical" href="${SITE_ORIGIN}/">
<link rel="icon" href="/brand/favicon.svg">
<meta property="og:type" content="website">
<meta property="og:site_name" content="uf">
<meta property="og:url" content="${SITE_ORIGIN}/">
<meta property="og:title" content="uf — Unified Toolchain for Flow">
<meta property="og:description" content="One native binary that runs, builds, tests, formats and lints a React application written in Flow.">
<meta property="og:image" content="${SITE_ORIGIN}/brand/og.png">
<meta property="og:image:alt" content="The uf mark beside the word uf, on a dark ground.">
<meta name="twitter:card" content="summary_large_image">
<style>${STYLE}</style>
</head>
<body>
<a class="skip" href="#content">Skip to content</a>
<div class="shell">

<header class="masthead">
  <a class="masthead-brand" href="/">
    <img src="/brand/uniflowed-mark.svg" alt="" width="22" height="22">
    uf
    <span class="version">0.0.0-alpha</span>
  </a>
  <nav class="masthead-nav" aria-label="Site">
    <a href="${DOCS_ORIGIN}">Documentation</a>
    <a href="${DOCS_ORIGIN}/guide/install">Install</a>
    <a href="${GITHUB}">Source</a>
  </nav>
</header>

<main class="home seam" id="content">

  <section class="hero">
    <div class="hero-copy">
      <p class="eyebrow">Unified toolchain for Flow</p>
      <h1>One binary for React<br>written in Flow.</h1>
      <p class="lede">
        uf runs, builds, tests, formats, lints and type-checks a React
        application written in Flow — every command reading one syntax tree,
        produced once by Meta&#39;s own Flow parser, compiled into the binary.
      </p>
    </div>
    <img
      class="hero-mark"
      src="/brand/uniflowed-mark.png"
      srcset="/brand/uniflowed-mark.png 512w, /brand/uf.png 1254w"
      sizes="(max-width: 48rem) 46vw, 22rem"
      alt=""
      width="1254"
      height="1254"
      fetchpriority="high"
    >
  </section>

  <section class="install">
    <div class="command"><code>curl -fsSL https://setup.uniflowed.dev | sh</code></div>
    <p>
      macOS and Linux, on x86-64 and ARM64. There is no Windows build. The
      script checks the archive against the sha256 in the release manifest
      before it unpacks anything, and it is served from
      <a href="https://setup.uniflowed.dev/install.sh">setup.uniflowed.dev/install.sh</a>
      if you would rather read it before you run it. uf still needs Node.js or
      Bun on the machine, because Vite and your test bodies run there.
    </p>
    <p>
      The command is <code>uf</code>. The package scope on npm is
      <code>@uniflowed</code>, and every release there so far is a prerelease
      under the <code>alpha</code> tag.
    </p>
    <div class="actions">
      <a class="button" href="${DOCS_ORIGIN}">Read the manual</a>
      <a class="button button-quiet" href="${GITHUB}">Source</a>
    </div>
  </section>

  <section class="section">
    <h2 class="seam-mark">Before you spend an afternoon on it</h2>
    <p>
      uf is <code>0.0.0-alpha</code>. It installs in seconds and scaffolds a
      project that builds, and it changes under you. These four things are true
      of every version so far; what is broken in the current one is in the
      README, which is written per release rather than copied here.
    </p>
    <dl class="facts">
      <div>
        <dt>Nothing is stable</dt>
        <dd>
          Commands, config keys and package exports move without a deprecation
          cycle, and no version promises compatibility with the one before it.
          <a href="${GITHUB}#where-it-stands">Where it stands</a>
        </dd>
      </div>
      <div>
        <dt>Every gap has an issue number</dt>
        <dd>
          What uf refuses to do and what is simply not written yet are two
          different lists, kept apart, with a reason on every refusal and an
          issue on every gap.
          <a href="${DOCS_ORIGIN}/guide/scope">What uf does not do</a>
        </dd>
      </div>
      <div>
        <dt>It does not check your types, and it is not a bundler</dt>
        <dd>
          Flow type-checks; <code>uf check</code> runs it. The dev server and
          the production build are Vite, driven over a JSON protocol, so Vite
          plugins keep working.
          <a href="${DOCS_ORIGIN}/guide/dev">Dev and build</a>
        </dd>
      </div>
      <div>
        <dt>It knows how this shape fails</dt>
        <dd>
          An all-in-one toolchain is the shape of
          <code>create-react-app</code>, and the way that failed is a
          specification for how uf could. The rules, and an audit of where uf
          does not meet them yet.
          <a href="${GITHUB}/blob/main/docs/red-lines.md">Architecture red lines</a>
        </dd>
      </div>
    </dl>
  </section>

</main>

<footer class="colophon">
  <span>uf is MIT licensed and pre-release. Nothing here is stable yet.</span>
  <span>
    This page is one Worker, in
    <a href="${GITHUB}/blob/main/infra/cloudflare/workers/root.js">infra/cloudflare</a>.
  </span>
</footer>

</div>
</body>
</html>
`;

/**
 * The apex's own `robots.txt`.
 *
 * Written here rather than inherited through the redirect, because what the
 * redirect resolves to is the documentation host's file: its `Sitemap:` line
 * names `docs.uniflowed.dev/sitemap.xml`, which is the right sitemap for that
 * host and no statement at all about this one. The apex has exactly one
 * indexable URL — everything else is a 308 — so there is nothing here to
 * disallow and one thing worth pointing at.
 */
const ROBOTS = `User-agent: *
Allow: /

Sitemap: ${DOCS_ORIGIN}/sitemap.xml
`;

/**
 * The response headers every answer carries, and the policy behind them.
 *
 * The policy is computed rather than written down. `style-src` names the
 * sha256 of `STYLE` itself, so the two cannot disagree: a stylesheet edited
 * without touching the policy would be a blank page on the site's front door,
 * and that is exactly the failure a hand-maintained hash produces. One digest,
 * once per isolate, memoised below.
 *
 * `default-src 'none'` is the whole policy for everything the page does not do,
 * and the page does almost nothing: no script, no font, no fetch, no frame, no
 * form. The two openings are `img-src 'self'` for the mark — which is why
 * `/brand/*` is served here rather than redirected — and the one style hash.
 * `script-src` and `object-src` are spelled out even though `default-src`
 * already covers them, so that a reader sees the two that matter said rather
 * than inferred.
 */
let policyText = null;

async function policy() {
  if (policyText === null) {
    const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(STYLE));
    const hash = btoa(String.fromCharCode(...new Uint8Array(digest)));
    policyText = [
      "default-src 'none'",
      "img-src 'self'",
      `style-src 'sha256-${hash}'`,
      "script-src 'none'",
      "object-src 'none'",
      "base-uri 'none'",
      "form-action 'none'",
      "frame-ancestors 'none'",
      "upgrade-insecure-requests",
    ].join("; ");
  }
  return policyText;
}

async function withHeaders(response, extra) {
  const headers = new Headers(response.headers);
  headers.set("content-security-policy", await policy());
  headers.set("x-content-type-options", "nosniff");
  headers.set("referrer-policy", "strict-origin-when-cross-origin");
  headers.set("permissions-policy", "interest-cohort=()");
  for (const [name, value] of Object.entries(extra ?? {})) {
    headers.set(name, value);
  }
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}

/** Where a path goes when the apex does not answer it itself. */
function docsRedirect(url) {
  return new Response(null, {
    status: 308,
    headers: {
      location: `${DOCS_ORIGIN}${url.pathname}${url.search}`,
      "cache-control": "public, max-age=3600",
    },
  });
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    // `www` is the apex, one hop away. Before anything else, because a `www`
    // request for `/` should reach the canonical origin rather than a second
    // copy of the page served under a second name.
    if (url.hostname.startsWith("www.")) {
      return withHeaders(
        new Response(null, {
          status: 308,
          headers: {
            location: `https://${url.hostname.slice("www.".length)}${url.pathname}${url.search}`,
            "cache-control": "public, max-age=3600",
          },
        }),
      );
    }

    const readOnly = request.method === "GET" || request.method === "HEAD";

    if (url.pathname === "/") {
      if (!readOnly) {
        return withHeaders(
          new Response("method not allowed\n", {
            status: 405,
            headers: { "content-type": "text/plain; charset=utf-8", allow: "GET, HEAD" },
          }),
        );
      }
      return withHeaders(
        new Response(PAGE, {
          headers: {
            "content-type": "text/html; charset=utf-8",
            "cache-control": "public, max-age=300",
          },
        }),
      );
    }

    if (url.pathname === "/robots.txt" && readOnly) {
      return withHeaders(
        new Response(ROBOTS, {
          headers: {
            "content-type": "text/plain; charset=utf-8",
            "cache-control": "public, max-age=3600",
          },
        }),
      );
    }

    // `/brand/<name>` is the repository's `brand/` directory, one level up in
    // the URL because that is the path the documentation site publishes it at
    // and the path everything already written points to. A name that is not
    // there — and every path when there is no `ASSETS` binding at all — falls
    // through to the redirect, which is what answered it before.
    if (url.pathname.startsWith("/brand/") && readOnly && env?.ASSETS != null) {
      const asset = new URL(url);
      asset.pathname = url.pathname.slice("/brand".length);
      asset.search = "";
      const response = await env.ASSETS.fetch(new Request(asset, { method: "GET" }));
      if (response.status === 200) {
        return withHeaders(response, { "cache-control": "public, max-age=3600" });
      }
    }

    return withHeaders(docsRedirect(url));
  },
};
