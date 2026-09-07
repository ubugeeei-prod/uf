// @flow
//
// The document.
//
// Everything every page shares: the head, the masthead, the colophon, and the
// one inline script on the site — the theme bootstrap, which has to run before
// first paint. The manual's sidebar is not here; it belongs to `/guide` and
// `/reference`, which have their own layout.

import * as React from "@uniflowed/react";
import { Suspense } from "@uniflowed/react";
import { Link, useRoute } from "@uniflowed/router";
import type { Metadata } from "@uniflowed/router";

import "./_design/seam.css";
import { nextTheme, themeBootstrap, themeLabel, useTheme } from "./_design/theme.js";

const VERSION = "0.0.0-alpha";

/**
 * What every page shares, and what a page overrides by declaring its own.
 *
 * The router merges this under each page's front matter, and renders the
 * result as a hoistable `<title>` — which is why there is no `<title>` in the
 * head below. Two of them is what you get if you write one here as well.
 *
 * `metadataBase` is the same origin as `site.url` in `uf.config.js`, and it is
 * written twice because the two are read by different things at different
 * times: `site.url` is what `uf build` puts in a `<loc>`, in Rust, with no
 * JavaScript in the process; this is what the renderer resolves `/brand/uf.png`
 * against. Keep them in step.
 *
 * There is deliberately no `canonical` here. A canonical URL is one page's own
 * URL, and a layout's metadata reaches every page under it — so declaring one
 * here would tell a search engine that all thirty pages of the manual are the
 * home page.
 */
export const metadata: Metadata = {
  title: "uf — Unified Toolchain for Flow",
  description:
    "The best React experience for Flow: run, build, test, format and lint from one command, without Babel or plugin assembly.",
  metadataBase: "https://docs.uniflowed.dev",
  // No `title` or `description` here: they fall back to the document's, which
  // is what a page that already said what it is called meant. `siteName` is
  // the one a card cannot derive.
  openGraph: { siteName: "uf", images: ["/brand/uf.png"] },
  twitter: { card: "summary_large_image", images: ["/brand/uf.png"] },
};

/**
 * The theme bootstrap, as an element.
 *
 * Written out here rather than in the head below because the suppression it
 * carries needs a line of its own: a `//` directive inside JSX children would
 * be text on the page, and the rule reads the line the attribute is written
 * on.
 *
 * `security/no-dangerously-set-inner-html` is about HTML that came from
 * somewhere — a comment, a profile, a response — and its escape hatch is a
 * `@uniflowed/markdown` sanitizer, which is the right answer for markup and no
 * answer at all for a script. This is the opposite end of the rule's subject:
 * a string constant in `_design/theme.js`, assembled from a literal through
 * `JSON.stringify`, with no parameter and nothing user-supplied within reach
 * of it. There is also no other spelling. A script's body has to be `__html`
 * because React escapes a text child — `t === "dark"` would reach the page as
 * `t === &quot;dark&quot;` — and it has to be inline because an external one
 * costs a round trip before first paint, which is the white flash the script
 * exists to prevent.
 */
// uf-lint-disable-next-line security/no-dangerously-set-inner-html
const themeBootstrapScript = <script dangerouslySetInnerHTML={{ __html: themeBootstrap }} />;

export component Layout(children: React.Node) {
  return (
    <html lang="en">
      <head>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="color-scheme" content="light dark" />
        <link rel="icon" href="/brand/favicon.svg" />
        <link rel="stylesheet" href="/brand/tokens.css" />
        {themeBootstrapScript}
      </head>
      <body>
        <a className="skip" href="#content">
          Skip to content
        </a>
        <div className="shell">
          <Masthead />
          <Suspense fallback={null}>{children}</Suspense>
          <Colophon />
        </div>
      </body>
    </html>
  );
}

component Masthead() {
  const { pathname } = useRoute();

  return (
    <header className="masthead">
      <Link className="masthead-brand" to="/">
        <img src="/brand/uniflowed-mark.svg" alt="" width="22" height="22" />
        uf
        <span className="version">{VERSION}</span>
      </Link>
      <nav className="masthead-nav" aria-label="Site">
        <Link to="/guide" aria-current={section(pathname) === "guide" ? "page" : undefined}>
          Guide
        </Link>
        <Link
          to="/reference/cli"
          aria-current={section(pathname) === "reference" ? "page" : undefined}
        >
          Reference
        </Link>
        <a href="https://github.com/ubugeeei-prod/uf">Source</a>
        <ThemeToggle />
      </nav>
    </header>
  );
}

component ThemeToggle() {
  const [theme, setTheme] = useTheme();

  return (
    <button className="theme-toggle" type="button" onClick={() => setTheme(nextTheme(theme))}>
      {themeLabel(theme)}
    </button>
  );
}

component Colophon() {
  return (
    <footer className="colophon">
      <span>uf is MIT licensed and pre-release. Nothing here is stable yet.</span>
      <span>
        Built with uf — this site is a uf project, in Flow, in{" "}
        <a href="https://github.com/ubugeeei-prod/uf/tree/main/docs">docs/</a>.
      </span>
    </footer>
  );
}

/** Which top-level part of the site a pathname belongs to. */
function section(pathname: string): "home" | "guide" | "reference" {
  if (pathname.startsWith("/reference")) {
    return "reference";
  }
  if (pathname.startsWith("/guide")) {
    return "guide";
  }
  return "home";
}
