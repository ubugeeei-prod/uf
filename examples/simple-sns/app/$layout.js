// @flow
import * as React from "@uniflowed/react";
import type { Metadata } from "@uniflowed/router";
import { props, stylex } from "@uniflowed/stylex";

export const metadata: Metadata = {
  title: "Simple SNS",
  description:
    "A uf SSR example social app with React 19.3, RSC, Server Actions, SQLite, and StyleX.",
};

export component Layout(children: React.Node) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head suppressHydrationWarning>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="color-scheme" content="light" />
      </head>
      <body {...props(styles.body)}>{children}</body>
    </html>
  );
}

const styles = stylex.create({
  body: {
    margin: 0,
    minHeight: "100vh",
  },
});
