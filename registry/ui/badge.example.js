// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Badge } from "./badge.js";

const styles = stylex.create({
  row: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: ufTokens.space2,
  },
});

/** Every tone, each with the words that carry its meaning. */
export component Example() {
  return (
    <div {...props(styles.row)}>
      <Badge>Draft</Badge>
      <Badge tone="accent">Beta</Badge>
      <Badge tone="solid">New</Badge>
      <Badge tone="danger">Failed</Badge>
      <Badge tone="outline">v0.1.0</Badge>
    </div>
  );
}
