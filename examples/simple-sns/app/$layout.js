// @flow
import * as React from "@uniflowed/react";
import type { Metadata, LayoutProps } from "@uniflowed/router";
import { props, stylex } from "@uniflowed/stylex";
import "./base.css";

export const metadata: Metadata = {
  title: "Commonplace",
  description: "Commonplace community workspace. Notes, private conversations, and your profile.",
};

export component Layout(...{ children }: LayoutProps) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head suppressHydrationWarning>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="color-scheme" content="light" />
        <link rel="icon" href="/favicon.svg" type="image/svg+xml" />
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
