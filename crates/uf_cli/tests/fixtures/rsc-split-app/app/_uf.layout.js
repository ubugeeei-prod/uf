// @flow
//
// The document, and a link to each route.
//
// No client boundary anywhere in it, which is what makes the two routes below
// differ from each other rather than from the layout. The links are real
// anchors — `Link` renders one and only takes over a plain left click — so the
// static route can be navigated *away from* with no JavaScript at all, which
// is the half of the split a file listing cannot show.

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";

export const metadata: {| readonly title: string |} = { title: "rsc-split-app" };

export component Layout(children: React.Node) {
  return (
    <html lang="en">
      <head>
        <meta charSet="utf-8" />
      </head>
      <body>
        <nav>
          <Link to="/">home</Link>
          <Link to="/counter">counter</Link>
        </nav>
        <main id="content">{children}</main>
      </body>
    </html>
  );
}
