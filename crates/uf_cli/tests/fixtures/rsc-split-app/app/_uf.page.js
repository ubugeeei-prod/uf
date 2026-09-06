// @flow
//
// The static route: server-rendered content, no client boundary, no state.
//
// No `import()` in the client route table reaches this module, so neither it
// nor `./_content/almanac.js` is a chunk of the client bundle and neither
// one's code survives into it. The stylesheet does, and has to: the linked
// `<link rel="stylesheet">` of every document in a uf build comes from the
// client graph, so a route dropped from it outright would take its rules off
// the whole site.

import * as React from "@uniflowed/react";

import { ALMANAC_MARKER, entries, line } from "./_content/almanac.js";
import "./_content/almanac.css";

export default component Home() {
  return (
    <section>
      <h1>rsc-split-app home</h1>
      <p data-marker={ALMANAC_MARKER}>{entries().length} notes</p>
      <ul className="almanac-note">
        {entries().map((entry) => (
          <li key={entry.day}>{line(entry)}</li>
        ))}
      </ul>
    </section>
  );
}
