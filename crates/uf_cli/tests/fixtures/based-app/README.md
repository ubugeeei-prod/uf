# `based-app`

The smallest uf application served somewhere other than the root of its host:
`app.router.basePath` is `/docs`, and `app.router.trailingSlash` is `"never"`.

- `app/$page.js` — the root, which is `/docs` itself, with a `Link` to
  `/guide` that has to be written as `href="/docs/guide"`;
- `app/guide/$page.js` — a prerendered page, which the policy writes as
  `dist/guide.html` rather than `dist/guide/index.html`;
- `app/posts/[slug]/$page.js` — a page rendered per request, handed the path
  without the base;
- `app/old/[slug]/$page.js` — a loader's `redirect()`, whose `Location` has to
  carry the base;
- `app/api/health/$route.js` — a route handler that says which path it was
  handed;
- `app/$not-found.js` — the 404 for a path under the base that has no route,
  as against the plain 404 for a path outside it.

`site.url` is set, so the build writes `sitemap.xml`, whose `<loc>` entries have
to be the addresses a server answers without a redirect.

`served-app` could not grow a base path: every question asked of it would move
under `/docs`, and it is the fixture every adapter's answers for everything
else are compared on. This is the other project, and it is asked only about the
two settings.

`crates/uf_cli/tests/vite.rs` builds it, reads the build, and asks `uf preview`,
`uf start`, three adapters' artefacts and `uf dev` the same questions.
