// @flow
//
// The document, and nothing else: every assertion made against this
// application is about which bytes of it arrived when.

import * as React from "@uniflowed/react";

export const metadata: {| readonly title: string |} = { title: "ppr-app" };

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
