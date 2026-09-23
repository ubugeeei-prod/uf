// @flow
//
// A page with a static shell and one hole.
//
// Everything outside the `<Suspense>` boundary reads nothing from the request,
// so `uf build` prerenders it: the heading, the article, the counter. The
// boundary's content reads a cookie, so the build leaves the boundary as a hole
// — its fallback is in the shell — and a server renders it per request. The
// article is there to give the shell some weight, which is what the
// measurement in the pull request that added this compares.

import * as React from "@uniflowed/react";

import { Counter } from "./Counter.js";
import { Session } from "./Session.js";

const PARAGRAPHS: $ReadOnlyArray<string> = Array.from(
  { length: 24 },
  (_, index) =>
    `Paragraph ${String(index + 1)} of the part of this page that is the same for everybody. ` +
    "It is written once, by the build, and sent before the server has rendered anything " +
    "for the request that asked for it.",
);

export default component Account() {
  return (
    <article>
      <h1 id="shell">Your account</h1>
      <React.Suspense fallback={<p id="hole-fallback">{"checking who you are"}</p>}>
        <Session />
      </React.Suspense>
      {PARAGRAPHS.map((text) => (
        <p key={text}>{text}</p>
      ))}
      <Counter />
    </article>
  );
}
