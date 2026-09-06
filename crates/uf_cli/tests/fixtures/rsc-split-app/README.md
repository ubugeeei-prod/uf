# `rsc-split-app`

Two routes, told apart by one import.

- `/` renders `app/_content/almanac.js` and nothing else. No `"use client"`
  module is reachable from its page or from the layout above it, so the browser
  never re-renders it and `uf build` leaves its page out of the client route
  table. Neither the page nor the almanac appears in `dist/assets/*.js`.
- `app/_content/almanac.css` is why that route's page is still *imported*, for
  its side effects, with nothing read from it. A uf build links the stylesheets
  it finds in the client module graph, so a route removed from that graph
  outright loses its rules — on every page of the site, because the linked
  sheets are the whole graph's. The rule `.almanac-note` has to be in
  `dist/assets/*.css` and the page's code has to not be in `dist/assets/*.js`,
  and those two sentences are the whole of what this fixture is for.
- `/counter` renders `app/counter/_components/Counter.js`, which is a
  `"use client"` module. The route keeps its page, so the page, the layout and
  the counter are all in the client bundle and the button works.

Each of the three modules carries a marker string — `ALMANAC_MARKER`,
`COUNTER_MARKER` — because a production bundle renames identifiers and keeps
string literals, so a marker is the one thing a test can look for in the
emitted JavaScript. `crates/uf_cli/tests/vite.rs` builds this project and
asserts that the counter's marker is in `dist/assets/*.js` and that the
almanac's is not.

The documentation site cannot answer that question: its root layout imports a
`"use client"` theme toggle, so every route in it reaches a boundary and
nothing is dropped. That is the point of it being a separate fixture rather
than a route added to the docs to make a test pass.
