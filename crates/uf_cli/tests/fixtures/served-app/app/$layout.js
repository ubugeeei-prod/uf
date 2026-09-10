// @flow
//
// The document, and nothing else.
//
// No styling and no navigation: every assertion made against this application
// is about *who answered*, so the markup is kept small enough that a failure
// prints the whole page.

import * as React from "@uniflowed/react";

export const metadata: {| readonly title: string |} = { title: "served-app" };

export component Layout(children: React.Node) {
  return (
    <html lang="en">
      <head>
        <meta charSet="utf-8" />
      </head>
      <body>
        <main id="content">{children}</main>
      </body>
    </html>
  );
}
