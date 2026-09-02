// @flow
import * as React from "@uniflowed/react";
import {
  ufBrand,
  ufCommands,
  ufPalette,
  ufRadiusScale,
  ufSpacingScale,
  ufTextScale,
} from "@uniflowed/brand";

component Page() {
  return (
    <main className="shell">
      <nav className="topbar" aria-label="Primary">
        <a className="brand-lockup" href="/">
          <img src="/brand/uf.png" alt="" width="40" height="40" />
          <span>{ufBrand.name}</span>
        </a>
        <div className="nav-links">
          <a href="/install/">Install</a>
          <a href="/brand/tokens.json">Tokens</a>
        </div>
      </nav>

      <section className="hero">
        <div className="logo-stage">
          <img src="/brand/uf.png" alt="uf gradient mark" />
        </div>
        <div className="hero-copy">
          <p className="eyebrow">uf Design System</p>
          <h1>{ufBrand.name}</h1>
          <h2>{ufBrand.headline}</h2>
          <p>{ufBrand.tagline}</p>
          <div className="accent-line" />
          <ul className="capabilities">
            <li>Unified</li>
            <li>Fast</li>
            <li>Elegant</li>
            <li>Modern</li>
            <li>Developer-first</li>
          </ul>
        </div>
      </section>

      <section className="token-grid" aria-label="Design tokens">
        <article className="panel palette-panel">
          <h3>Color Tokens</h3>
          <ul className="swatches">
            {ufPalette.map((color) => (
              <li key={color.token}>
                <span
                  className="swatch"
                  style={{ backgroundColor: color.value }}
                />
                <strong>{color.name}</strong>
                <code>{color.token}</code>
                <span>{color.value}</span>
              </li>
            ))}
          </ul>
        </article>

        <article className="panel">
          <h3>Typography Tokens</h3>
          <div className="type-spec">
            <strong>Satoshi</strong>
            <span>Display / Headings</span>
          </div>
          <div className="type-spec body">
            <strong>Inter</strong>
            <span>Body / UI Text</span>
          </div>
          <div className="type-spec mono">
            <strong>JetBrains Mono</strong>
            <span>Code / Monospace</span>
          </div>
          <ul className="token-list">
            {ufTextScale.map((token) => (
              <li key={token.token}>
                <code>{token.token}</code>
                <span>{token.value}</span>
              </li>
            ))}
          </ul>
        </article>

        <article className="panel">
          <h3>Spacing Tokens</h3>
          <ul className="bars">
            {ufSpacingScale.map((token) => (
              <li key={token.token}>
                <code>{token.token}</code>
                <span>{token.value}</span>
                <i style={{ width: token.value }} />
              </li>
            ))}
          </ul>
        </article>

        <article className="panel">
          <h3>Radius Tokens</h3>
          <ul className="radius-list">
            {ufRadiusScale.map((token) => (
              <li key={token.token}>
                <span style={{ borderRadius: token.value }} />
                <code>{token.token}</code>
                <em>{token.value}</em>
              </li>
            ))}
          </ul>
        </article>
      </section>

      <section className="terminal-section">
        <div className="terminal">
          <div className="traffic">
            <span />
            <span />
            <span />
            <strong>uf</strong>
          </div>
          <pre><code>{`uf v0.1.0
Unified Toolchain for Flow

$ ${ufCommands.curl}
$ ${ufCommands.nixProfile}
$ ${ufCommands.info}

All set: Happy coding.`}</code></pre>
        </div>
        <div className="install-copy">
          <h3>Install</h3>
          <p>Use the hosted installer, or install the flake package directly.</p>
          <pre><code>{`${ufCommands.curl}
${ufCommands.nixRun}
${ufCommands.nixProfile}`}</code></pre>
          <a className="primary-action" href="/install/">Open install docs</a>
        </div>
      </section>
    </main>
  );
}

export default Page;
