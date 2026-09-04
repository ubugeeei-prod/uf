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

import "./_design/seam.css";
import { nextTheme, themeBootstrap, themeLabel, useTheme } from "./_design/theme.js";

const VERSION = "0.0.0-alpha";

/**
 * The document's title and description when a page does not give its own.
 *
 * The router merges this under each page's front matter, and renders the
 * result as a hoistable `<title>` — which is why there is no `<title>` in the
 * head below. Two of them is what you get if you write one here as well.
 */
export const metadata: {| readonly title: string, readonly description: string |} = {
  title: "uf — Unified Toolchain for Flow",
  description:
    "One binary that runs, builds, tests, formats and lints Flow and React. No Babel, no plugin list, no second config file.",
};

export component Layout(children: React.Node) {
  return (
    <html lang="en">
      <head>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="color-scheme" content="light dark" />
        <link rel="icon" href="/brand/favicon.svg" />
        <link rel="stylesheet" href="/brand/tokens.css" />
        <script dangerouslySetInnerHTML={{ __html: themeBootstrap }} />
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
