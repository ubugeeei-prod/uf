# `served-app`

The smallest uf application that has the two things a build could not serve
until `uf preview` and `uf start` existed:

- `app/posts/[slug]/_uf.page.js` — a route with a parameter and **no**
  `generateStaticParams`, so `uf build` prerenders nothing for it and the only
  way it can answer is per request;
- `app/api/health/_uf.route.js` — a route handler, which answers a `POST` that
  no page can and which `uf build` never called.

The documentation site is the fixture for everything else, and it deliberately
has neither: it is a static site, and adding a `/posts/[slug]` to it to make a
test pass would be the artificial usage `ubugeeei-redundancy.md` forbids. This
is the other project, and it exists to be served rather than read.

`crates/uf_cli/tests/vite.rs` builds it and drives both servers against it.
