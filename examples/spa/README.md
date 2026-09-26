# Taskboard — a Flow SPA

A small browser application with two routes, shared atoms, derived progress,
keyboard accessible task controls and uf's paper, ink and seam styling.

```sh
cd examples/spa
uf install
uf dev
```

Add and complete tasks on the board, then open **Progress**. Client navigation
keeps the same reactive store. State is intentionally in memory and resets on
a reload; this sample sends no task data to a server.

## Build

```sh
uf build
```

`app.rendering.modes: ["csr"]` produces a single empty HTML shell, browser
assets and an identical `404.html` fallback. There is no server bundle or
prerendered task data. Serve `dist/` on a static host and return `index.html`
for application URLs such as `/progress`. `public/_redirects` provides that
fallback for hosts supporting the redirects-file convention; configure the
equivalent history fallback on other hosts.

## Read the source

- `uf.config.js`: the client rendering plan.
- `app/model.js`: typed tasks, shared state and actions.
- `app/$layout.js`: navigation and the shared application frame.
- `app/$page.js`: add, toggle and clear tasks.
- `app/progress/$page.js`: another route subscribing to the same state.

When running from a repository checkout, install the root npm workspace first
so `@uniflowed/*` resolve to its local packages. In a copied example, `uf install`
uses the published package versions declared here.
