// @flow
import * as React from "@uniflowed/react";
import { Suspense } from "@uniflowed/react";

component Layout(children: React.Node) {
  return (
    <html lang="en">
      <head>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <title>uf docs</title>
        <meta
          name="description"
          content="Unified Toolchain for Flow and React."
        />
        <link rel="icon" href="/brand/favicon.svg" />
        <link rel="stylesheet" href="/brand/tokens.css" />
        <link rel="stylesheet" href="/docs.css" />
      </head>
      <body>
        <Suspense fallback={null}>{children}</Suspense>
      </body>
    </html>
  );
}

export default Layout;
