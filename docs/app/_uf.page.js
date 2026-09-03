// @flow
//
// The home page.
//
// It leads with the thing itself — one command, and what that command really
// prints — because the argument for uf is not a feature list, it is that the
// list collapses into one binary. Every claim below links to the page that
// backs it, and the ones uf loses are on that list too.

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";

import { Claims, Command, Eyebrow, Lede, Terminal } from "./_design/parts.js";

/**
 * Output of `uf build` on this site, pasted from a real run.
 *
 * Trimmed to the phases and the summary — the size table underneath it is
 * fifty rows long — but not otherwise edited. When the build changes shape
 * this block is re-pasted rather than adjusted by hand: a page that shows
 * invented output is wrong the first time somebody checks it.
 */
const BUILD_OUTPUT = [
  { text: "uf build  docs" },
  { text: "──────────────", tone: "muted" },
  { text: "" },
  { text: "  config       ························ 105.2µs" },
  { text: "  routes       ························ 402.2µs" },
  { text: "  router types ························ 300.7µs" },
  { text: "  rsc analysis ························   4.7ms" },
  { text: "  vite         ························   2.11s" },
  { text: "  manifest     ························ 134.7µs" },
  { text: "  total        ························   6.44s" },
  { text: "" },
  { text: "  engine             vite", tone: "muted" },
  { text: "  host               node", tone: "muted" },
  { text: "  prerendered pages  14", tone: "muted" },
  { text: "  modules            12", tone: "muted" },
];

export default component Home() {
  return (
    <div className="home seam" id="content">
      <section className="hero">
        <div className="hero-copy">
          <Eyebrow>Unified toolchain for Flow</Eyebrow>
          <h1>
            One binary between
            <br />
            your code and the web.
          </h1>
          <Lede>
            uf runs, builds, tests, formats and lints Flow and React from a
            single command. No Babel in the pipeline, no plugin list to
            assemble, and one config file for all of it.
          </Lede>
        </div>

        {/*
          Decorative: the name is already in the heading and in the masthead,
          so announcing the mark again would only repeat it.

          The mark exists at two sizes and neither is a vector — the `.svg` in
          `brand/` is a PNG in an SVG wrapper. `srcset` lets a phone take the
          512px file and a retina desktop take the 1254px one, rather than
          every visitor paying 400 kB or every retina screen getting a soft
          mark. The intrinsic size is declared so the row does not reflow when
          it arrives.
        */}
        <img
          className="hero-mark"
          src="/brand/uniflowed-mark.png"
          srcSet="/brand/uniflowed-mark.png 512w, /brand/uf.png 1254w"
          sizes="(max-width: 48rem) 46vw, 26rem"
          alt=""
          width="1254"
          height="1254"
          fetchPriority="high"
        />
      </section>

      <section className="hero-tail">
        <div className="hero-actions">
          <Link className="button" to="/guide/install">
            Install uf
          </Link>
          <Link className="button button-quiet" to="/guide/why">
            Why Flow
          </Link>
        </div>

        <div className="notice">
          <strong>Pre-release.</strong> uf is at{" "}
          <code>0.0.0-alpha</code>. Interfaces move without warning and the
          packages are not on npm yet. The guide says what works today and{" "}
          <Link to="/guide/testing">where it loses</Link> to the tools it means
          to replace.
        </div>
      </section>

      <section className="home-section">
        <h2 className="seam-mark">Install it</h2>
        <p>
          One binary. It reads the checksum from the release manifest before it
          writes anything, and it is short enough to read first.
        </p>
        <Command>curl -fsSL https://setup.uniflowed.dev | sh</Command>
        <p>
          Then a project, and a dev server, with nothing to configure in
          between:
        </p>
        <Command>uf create app my-site</Command>
        <Command>cd my-site &amp;&amp; uf dev</Command>
        <p>
          That is the whole setup. No <code>npm install</code> of a toolchain,
          no config to copy from somewhere. If you would rather build from
          source, or pin the project to Bun or Deno,{" "}
          <Link to="/guide/install">the install page</Link> covers both.
        </p>
      </section>

      <section className="home-section">
        <h2 className="seam-mark">What a build looks like</h2>
        <Command>uf build</Command>
        <Terminal lines={BUILD_OUTPUT} label="Output of uf build" />
        <p>
          There is no <code>vite.config.ts</code> next to that, no{" "}
          <code>babel.config.js</code>, and no <code>@babel/preset-flow</code>{" "}
          in the dependency tree. Flow reaches JavaScript through Meta's own
          Rust parser and React's own compiler, both linked into the binary.
        </p>
      </section>

      <section className="home-section">
        <h2 className="seam-mark">What it claims</h2>
        <Claims
          items={[
            {
              title: "Flow without Babel",
              body: (
                <>
                  The official Flow parser, the official React Compiler and oxc,
                  in one Rust pipeline. <code>component</code>, <code>hook</code>,{" "}
                  <code>match</code> and enums are lowered where they are parsed.{" "}
                  <Link to="/guide/flow">How it works</Link>
                </>
              ),
            },
            {
              title: "Vite, not a fork of it",
              body: (
                <>
                  The dev server and the production build are Vite 8. uf decides
                  what Vite is handed and drives it over a JSON protocol, so
                  Vite's plugin ecosystem keeps working.{" "}
                  <Link to="/guide/dev">Dev and build</Link>
                </>
              ),
            },
            {
              title: "Tests that actually run",
              body: (
                <>
                  Rust owns discovery, ordering, the worker pool and the report;
                  the host runs the bodies. About nine times faster than Vitest
                  on 1,000 tests — and about three times slower than Bun, which
                  the guide explains rather than hides.{" "}
                  <Link to="/guide/testing">Testing</Link>
                </>
              ),
            },
            {
              title: "One config file",
              body: (
                <>
                  <code>uf.config.js</code> configures the runtime, the router,
                  the build, the test runner and the formatter. It is Flow, so
                  it is type-checked like the rest of your code.{" "}
                  <Link to="/reference/config">Every option</Link>
                </>
              ),
            },
            {
              title: "Any JavaScript host",
              body: (
                <>
                  Node.js, Bun and Deno are capabilities, not targets: uf asks
                  the host what it can do and picks one. The same project builds
                  and tests on all three.{" "}
                  <Link to="/guide/install">Install</Link>
                </>
              ),
            },
          ]}
        />
      </section>
    </div>
  );
}
