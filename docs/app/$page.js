// @flow
//
// The documentation home page: what uf is, how to install it, and where to go next.

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
            Build React apps with Modern Flow. Use one native toolchain for development, builds,
            tests, formatting and type checking.
          </Lede>
          <nav className="hero-actions" aria-label="Documentation">
            <Link className="hero-primary" to="/guide/start">
              Getting started <span aria-hidden="true">→</span>
            </Link>
            <Link to="/reference">Reference</Link>
          </nav>
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
          <strong>Pre-release.</strong> uf is in alpha. Commands, configuration and package APIs may
          change between releases. See <Link to="/guide/scope">current capabilities</Link>.
        </div>
      </section>

      <section className="home-section">
        <h2 className="seam-mark">Install it</h2>
        <p>On macOS or Linux, install the native CLI:</p>
        <Command>curl -fsSL https://setup.uniflowed.dev | sh</Command>
        <p>Then a project, its packages, and a dev server:</p>
        <Command>uf new my-site</Command>
        <Command>cd my-site &amp;&amp; uf install</Command>
        <Command>uf dev</Command>
        <p>
          See the <Link to="/guide/install">installation guide</Link> for Windows, Nix, source
          builds and JavaScript hosts.
        </p>
      </section>
      <section className="home-section">
        <h2 className="seam-mark">Where to begin</h2>
        <p>
          Each part of the manual, the question it answers, and the pages most of its readers open
          first.
        </p>
        <ReaderPaths />
      </section>
    </div>
  );
}
