# uf Brand System

This directory is the source of truth for the uf visual system: the extracted
glossy mark, stable raster lockups, SVG wrappers, CSS custom properties, JSON
design tokens, and Flow constants used by docs and examples.

## Files

- `uf.png`: high-resolution source mark
- `uniflowed-mark.png`: trimmed display mark
- `uniflowed-mark.svg`: SVG wrapper for the display mark
- `uniflowed-logo.png`: mark plus wordmark lockup
- `uniflowed-logo.svg`: SVG wrapper for the lockup
- `uniflowed-wordmark.png`: extracted text-only wordmark
- `uniflowed-wordmark.svg`: SVG wrapper for the wordmark
- `favicon.svg`: browser icon
- `og.png`: the 1200×630 share card, and `og.html` the CSS it is rendered from

  The mark and the name on the dark ground, and nothing else. A card never
  appears alone — every platform prints `og:title` and `og:description` beside
  it — so a sentence in the image is that sentence twice, and it is the copy in
  the image that is too small to read. What the picture is for is being
  recognised.

  `og.html` is the source and `tools/brand/render-og.sh` turns it into the PNG,
  which is committed because a build should not need a browser.
- `tokens.json`: complete design token data
- `tokens.css`: CSS custom properties
- `index.js`: Flow constants for dogfooding in uf projects

## Palette

- Primary Cyan: `#35D6F6`
- Electric Blue: `#2677FF`
- Indigo: `#5C49FF`
- Violet: `#8F4BFF`
- Magenta: `#D84BFF`
- Ink: `#0F172A`
- Slate: `#475569`
- Mist: `#F8FAFC`

## Usage

Docs copy this directory into the generated static asset tree before running
`uf build`, so the deployed pages and bundle report exercise the same token
files checked into the repo.
