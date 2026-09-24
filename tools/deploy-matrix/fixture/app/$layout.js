// @flow
//
// The document every page of the deploy-matrix fixture renders in.
//
// The links are `<Link>`s so the browser check can navigate *without* a
// document request — the RSC payload (`__uf.flight`) is what the target has to
// serve for that — and `<Hydrated />` is the one client component on every
// page, so "the page hydrated" is a single attribute the check can wait for.

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";

import Hydrated from "./_client/Hydrated.js";

export const metadata: {| readonly title: string |} = { title: "deploy-matrix" };

export component Layout(children: React.Node) {
  return (
    <html lang="en">
      <head>
        <meta charSet="utf-8" />
      </head>
      <body>
        <nav>
          <Link to="/">home</Link> <Link to="/posts/first">first post</Link>{" "}
          <Link to="/posts/second">second post</Link>
        </nav>
        <Hydrated />
        <main id="content">{children}</main>
      </body>
    </html>
  );
}
