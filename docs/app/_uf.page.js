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

const cyan = ufPalette[0];
const blue = ufPalette[1];
const indigo = ufPalette[2];
const violet = ufPalette[3];
const magenta = ufPalette[4];
const ink = ufPalette[5];
const slate = ufPalette[6];
const mist = ufPalette[7];

const textXs = ufTextScale[0];
const textSm = ufTextScale[1];
const textMd = ufTextScale[2];
const textLg = ufTextScale[3];
const textXl = ufTextScale[4];
const text2xl = ufTextScale[5];

const space1 = ufSpacingScale[0];
const space2 = ufSpacingScale[1];
const space3 = ufSpacingScale[2];
const space4 = ufSpacingScale[3];
const space6 = ufSpacingScale[4];
const space8 = ufSpacingScale[5];
const space12 = ufSpacingScale[6];

const radiusSm = ufRadiusScale[0];
const radiusMd = ufRadiusScale[1];
const radiusLg = ufRadiusScale[2];
const radiusXl = ufRadiusScale[3];
const radiusPill = ufRadiusScale[4];

export component Page() {
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
            <li>
              <span className="swatch" style={{ backgroundColor: cyan.value }} />
              <strong>{cyan.name}</strong>
              <code>{cyan.token}</code>
              <span>{cyan.value}</span>
            </li>
            <li>
              <span className="swatch" style={{ backgroundColor: blue.value }} />
              <strong>{blue.name}</strong>
              <code>{blue.token}</code>
              <span>{blue.value}</span>
            </li>
            <li>
              <span className="swatch" style={{ backgroundColor: indigo.value }} />
              <strong>{indigo.name}</strong>
              <code>{indigo.token}</code>
              <span>{indigo.value}</span>
            </li>
            <li>
              <span className="swatch" style={{ backgroundColor: violet.value }} />
              <strong>{violet.name}</strong>
              <code>{violet.token}</code>
              <span>{violet.value}</span>
            </li>
            <li>
              <span className="swatch" style={{ backgroundColor: magenta.value }} />
              <strong>{magenta.name}</strong>
              <code>{magenta.token}</code>
              <span>{magenta.value}</span>
            </li>
            <li>
              <span className="swatch" style={{ backgroundColor: ink.value }} />
              <strong>{ink.name}</strong>
              <code>{ink.token}</code>
              <span>{ink.value}</span>
            </li>
            <li>
              <span className="swatch" style={{ backgroundColor: slate.value }} />
              <strong>{slate.name}</strong>
              <code>{slate.token}</code>
              <span>{slate.value}</span>
            </li>
            <li>
              <span className="swatch" style={{ backgroundColor: mist.value }} />
              <strong>{mist.name}</strong>
              <code>{mist.token}</code>
              <span>{mist.value}</span>
            </li>
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
            <li>
              <code>{textXs.token}</code>
              <span>{textXs.value}</span>
            </li>
            <li>
              <code>{textSm.token}</code>
              <span>{textSm.value}</span>
            </li>
            <li>
              <code>{textMd.token}</code>
              <span>{textMd.value}</span>
            </li>
            <li>
              <code>{textLg.token}</code>
              <span>{textLg.value}</span>
            </li>
            <li>
              <code>{textXl.token}</code>
              <span>{textXl.value}</span>
            </li>
            <li>
              <code>{text2xl.token}</code>
              <span>{text2xl.value}</span>
            </li>
          </ul>
        </article>

        <article className="panel">
          <h3>Spacing Tokens</h3>
          <ul className="bars">
            <li>
              <code>{space1.token}</code>
              <span>{space1.value}</span>
              <i style={{ width: space1.value }} />
            </li>
            <li>
              <code>{space2.token}</code>
              <span>{space2.value}</span>
              <i style={{ width: space2.value }} />
            </li>
            <li>
              <code>{space3.token}</code>
              <span>{space3.value}</span>
              <i style={{ width: space3.value }} />
            </li>
            <li>
              <code>{space4.token}</code>
              <span>{space4.value}</span>
              <i style={{ width: space4.value }} />
            </li>
            <li>
              <code>{space6.token}</code>
              <span>{space6.value}</span>
              <i style={{ width: space6.value }} />
            </li>
            <li>
              <code>{space8.token}</code>
              <span>{space8.value}</span>
              <i style={{ width: space8.value }} />
            </li>
            <li>
              <code>{space12.token}</code>
              <span>{space12.value}</span>
              <i style={{ width: space12.value }} />
            </li>
          </ul>
        </article>

        <article className="panel">
          <h3>Radius Tokens</h3>
          <ul className="radius-list">
            <li>
              <span style={{ borderRadius: radiusSm.value }} />
              <code>{radiusSm.token}</code>
              <em>{radiusSm.value}</em>
            </li>
            <li>
              <span style={{ borderRadius: radiusMd.value }} />
              <code>{radiusMd.token}</code>
              <em>{radiusMd.value}</em>
            </li>
            <li>
              <span style={{ borderRadius: radiusLg.value }} />
              <code>{radiusLg.token}</code>
              <em>{radiusLg.value}</em>
            </li>
            <li>
              <span style={{ borderRadius: radiusXl.value }} />
              <code>{radiusXl.token}</code>
              <em>{radiusXl.value}</em>
            </li>
            <li>
              <span style={{ borderRadius: radiusPill.value }} />
              <code>{radiusPill.token}</code>
              <em>{radiusPill.value}</em>
            </li>
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
