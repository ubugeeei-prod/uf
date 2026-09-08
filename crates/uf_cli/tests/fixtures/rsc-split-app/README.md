# `rsc-split-app`

Two routes told apart by one import, and inside one of them a module told apart
by one directive. The two units of the React Server Components split, in one
project small enough to read.

## The route

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
  and those two sentences are the whole of what this half is for.
- `/counter` renders `app/counter/_components/Counter.js`, which is a
  `"use client"` module. The route keeps its page, so the page, the layout and
  the counter are all in the client bundle and the button works.

## The module

`/counter`'s counter also calls a server action, and that is the second unit.

- `app/counter/_actions/tally.js` is a `"use server"` module. In the browser it
  is one `createServerReference` per callable export — an id and a `fetch` —
  and in the server bundle it is the module itself. So a route the browser
  certainly does need still keeps something off it.
- It has two exports because there are two ways a browser reaches one.
  `recordCount` is a click, called with a number. `submitNote` is a form:
  `Counter.js` renders `<form action={…}>` around `useActionState`, React hands
  the action the previous state and a `FormData`, and the action validates the
  raw fields with `@uniflowed/validator` before it believes any of them. Two
  callable exports of one module is also the only place the fixture exercises a
  reference module that declares more than one name.
- What it keeps off is the point. `tally.js` imports `cookies` from
  `@uniflowed/server`, which imports `node:async_hooks`: "a page that calls
  `cookies()` puts that import in the browser's graph, and nothing stops it" is
  the sentence in ubugeeei-prod/uf#252 that this replaces. It also imports
  `app/counter/_actions/ledger.js`, an ordinary module with no directive of its
  own, standing in for the database handle an action reaches for, and
  `@uniflowed/validator`, whose parse path is 9.7 kB of the server bundle and
  none of the client's.
- The action reads a cookie and returns what the ledger made of its argument,
  so a test that calls it proves the request reached the function rather than
  that something answered `200`.

## Markers

Each module carries a marker string — `ALMANAC_MARKER`, `COUNTER_MARKER`,
`TALLY_MARKER`, `LEDGER_MARKER` — because a production bundle renames
identifiers and keeps string literals, so a marker is the one thing a test can
look for in the emitted JavaScript. `TALLY_MARKER` is not exported: a
`"use server"` module may only export async functions, and a string literal in
a function body survives minification exactly as an exported one would.

`Counter.js` carries one more, `in-source-marker-no-build-may-ship-this`, and
it is the odd one out: it is inside an `if (import.meta.uf.test)` block, so it
has to be *absent* from a bundle the module it lives in is present in. That
pairing is what makes it evidence rather than a grep that happens to fail —
see ubugeeei-prod/uf#518.

`crates/uf_cli/tests/vite.rs` builds this project and asserts that the
counter's marker is in `dist/assets/*.js`, that the almanac's, the tally's, the
ledger's and the in-source block's are not, that `async_hooks` is not either,
and that the same two actions — the click and the form — answer identically
through all four deploy adapters.

The documentation site cannot answer any of that: its root layout imports a
`"use client"` theme toggle, so every route in it reaches a boundary and
nothing is dropped. That is the point of it being a separate fixture rather
than a route added to the docs to make a test pass.
