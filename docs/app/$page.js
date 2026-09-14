// @flow
//
// The home page.
//
// A choice of path, not a pitch. The heading says what uf is for; under it,
// before anything else, is every kind of reader the manual has and the page
// each should open first. That list is generated from `_design/nav.js`, so the
// home page cannot offer a path the manual does not have. The install command
// comes after it, because the most common path starts with it.
//
// The argument for uf, with the rows it loses, is the "Why uf" section — one
// of the paths, rather than the first thing on the page.

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import type { Metadata } from "@uniflowed/router";

import { Command, Eyebrow, Lede, ReaderPaths } from "./_design/parts.js";

/**
 * The one page whose canonical URL is the site's own.
 *
 * Only `canonical`: the merge is shallow per key, so declaring this leaves the
 * title, the description and the card the root layout set exactly as they
 * were. Resolved against the layout's `metadataBase`, which is why it is a
 * path here and an absolute URL in the document.
 */
export const metadata: Metadata = { canonical: "/" };

export default component Home() {
  return (
    <div className="home seam" id="content">
      <section className="hero">
        <div className="hero-copy">
          <Eyebrow>Unified toolchain for Flow</Eyebrow>
          <h1>
            Build the strongest React
            <br />
            experience with Modern Flow.
          </h1>
          <Lede>
            uf gives Flow-first React apps one native command for dev, builds, tests, formatting and
            linting. The official Flow parser, React Compiler and oxc stay in the same toolchain,
            with one config file for the whole path.
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
        <div className="notice">
          <strong>Pre-release.</strong> uf is at <code>0.0.0-alpha</code>. Interfaces move without
          warning, and every <code>@uniflowed/*</code> release on npm is a prerelease under the{" "}
          <code>alpha</code> tag. The guide says what works today and{" "}
          <Link to="/guide/testing">where it loses</Link> to the tools it means to replace.
        </div>
      </section>

      <section className="home-section">
        <h2 className="seam-mark">Where to begin</h2>
        <p>
          The manual is arranged by what you came to do. Find the question you arrived with: the
          page it leads to says who that part of the manual is for, what is in it, and the order to
          read it in.
        </p>
        <ReaderPaths />
      </section>

      <section className="home-section">
        <h2 className="seam-mark">Install it</h2>
        <p>
          One binary. It reads the checksum from the release manifest before it writes anything, and
          it is short enough to read first.
        </p>
        <Command>curl -fsSL https://setup.uniflowed.dev | sh</Command>
        <p>Then a project, its packages, and a dev server:</p>
        <Command>uf new my-site</Command>
        <Command>cd my-site &amp;&amp; uf install</Command>
        <Command>uf dev</Command>
        <p>
          <code>uf new</code> writes the project and nothing else, so its packages come second:{" "}
          <code>uf install</code> fetches them with the project&apos;s package manager. There is no
          toolchain to <code>npm install</code> and no config to copy from somewhere. To build uf
          from source, or to pin a project to Bun, <Link to="/guide/install">the install page</Link>{" "}
          covers both.
        </p>
      </section>
    </div>
  );
}
