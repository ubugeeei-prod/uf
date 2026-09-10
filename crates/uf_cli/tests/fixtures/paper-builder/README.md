# `paper-builder`

A **second implementation** of the builder contract in `docs/architecture.md`,
and the evidence that the contract is a contract rather than a description of
`@uniflowed/vite`.

It has no bundler in it. It walks the router root for `$page.js` files,
writes one paper-thin document per route, and reports what it did in the
driver's event vocabulary — which is the whole of what uf asks a builder for on
`uf build`. Nothing it does needs Vite, Rolldown, or a module graph, and that
is the point: if any part of uf's seam were Vite-shaped, this could not
satisfy it.

## What it is not

A builder anybody should use. The documents it writes contain no application:
it does not transform Flow, so it cannot render a component, and it implements
`build` alone — `dev`, `preview`, `start`, `compile` and `deploy` are answered
with the error the contract says to answer an unimplemented command with.

A real second builder — Rolldown, rspack, esbuild — is a different piece of
work, and Planned. This one exists so that the seam cannot quietly stop being
one: a change to uf that assumed Vite would fail here rather than in somebody's
project six months later.

`crates/uf_cli/tests/vite.rs` copies this into a throwaway project and builds
with it.
