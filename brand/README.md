# uf Brand System

This directory is the source of truth for the uf visual system: the glossy mark,
SVG fallbacks, CSS custom properties, JSON design tokens, and Flow constants
used by docs and examples.

## Files

- `uf.png`: primary glossy logo
- `uniflowed-mark.svg`: square icon
- `uniflowed-logo.svg`: mark plus wordmark
- `uniflowed-wordmark.svg`: text-only wordmark
- `favicon.svg`: browser icon
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
