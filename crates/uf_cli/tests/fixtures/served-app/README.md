# `served-app`

The smallest uf application that has the things a build could not serve until
`uf preview` and `uf start` existed:

- `app/posts/[slug]/$page.js` — a route with a parameter and **no**
  `generateStaticParams`, so `uf build` prerenders nothing for it and the only
  way it can answer is per request;
- `app/api/health/$route.js` — a route handler, which answers a `POST` that
  no page can and which `uf build` never called;
- `app/old/[slug]/$page.js` — a page whose loader calls `redirect()`, which
  is the one answer that is entirely a *header*: a server that writes the status
  and the body and drops the render's headers answers a 307 with no `Location`.
  A parameter and no `generateStaticParams` keeps it out of the prerender, so
  `uf dev`, `uf preview` and `uf start` all have to render it and can all be
  asked the same question.

It also has the route that shows the renderer streaming:

- `app/slow/[id]/$page.js` — a page that suspends for 500 ms while it
  renders, with `app/slow/$loading.js` beside it. A parameter and no
  `generateStaticParams` keeps it out of the prerender, so `uf start` renders it
  per request and the test can watch the layout and the fallback arrive before
  the page does.

`site.url` is set, so the build writes `sitemap.xml` and `robots.txt` here too
— and the parameterised routes above are exactly the ones a sitemap cannot
name, because nothing enumerates them. `dist/404.html` is the other case: a
document that is served and is not a page.

The documentation site is the fixture for everything else, and it deliberately
has neither: it is a static site, and adding a `/posts/[slug]` to it to make a
test pass would be the artificial usage `ubugeeei-redundancy.md` forbids. This
is the other project, and it exists to be served rather than read.

`crates/uf_cli/tests/vite.rs` builds it and drives all three servers against
it — `uf dev` as well as `uf preview` and `uf start`, because "the dev server
answers what the deployment answers" is a claim that needs the same fixture
asked the same questions rather than a second one that agrees by construction.
