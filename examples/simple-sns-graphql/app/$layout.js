// @flow

import * as React from "@uniflowed/react";

import type { Metadata, LayoutProps } from "@uniflowed/router";

import { props, stylex } from "@uniflowed/stylex";

import { RelayRoot } from "./_shared/relay-root.client.js";

import "./_shared/base.css";

export const metadata: Metadata = {
  title: "Commonplace",
  description: "Commonplace community workspace. Notes, private conversations, and your profile.",
};

/**
 * Install shared visual styles and the browser's Relay provider, then render the router-owned
 * layout slot. Every route below is a server component that preloads for its own client islands.
 */

export component Layout(...{ children }: LayoutProps) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head suppressHydrationWarning>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="color-scheme" content="light" />
        <link rel="icon" href="/favicon.svg" type="image/svg+xml" />
      </head>
      <body {...props(styles.body)}>
        <RelayRoot>{children}</RelayRoot>
      </body>
    </html>
  );
}

const styles = stylex.create({
  body: {
    margin: 0,
    minHeight: "100vh",
  },
});
