// @flow
//
// The document, and nothing else: every assertion made against this
// application is about which address answered, so the markup is kept small
// enough that a failure prints the whole page.

import * as React from "@uniflowed/react";

export const metadata: {| readonly title: string |} = { title: "based-app" };

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
