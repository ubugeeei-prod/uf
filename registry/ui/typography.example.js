// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import * as Typography from "./typography.js";

const styles = stylex.create({
  article: {
    display: "grid",
    gap: ufTokens.space4,
    maxWidth: "40rem",
  },
});

/** A short release note, in every part, with the heading levels in order. */
export component Example() {
  return (
    <article {...props(styles.article)}>
      <Typography.H1>Release notes</Typography.H1>
      <Typography.Lead>What changed in this version, and what to do about it.</Typography.Lead>
      <Typography.H2>Upgrading</Typography.H2>
      <Typography.P>
        Run <Typography.InlineCode>uf ui update</Typography.InlineCode> to bring an edited copy up
        to date.
      </Typography.P>
      <Typography.H3>What moved</Typography.H3>
      <Typography.List>
        <li>Spinners respect reduced motion.</li>
        <li>Native selects draw their own chevron.</li>
      </Typography.List>
      <Typography.H4>Steps</Typography.H4>
      <Typography.List ordered>
        <li>Upgrade the package.</li>
        <li>Run the update.</li>
      </Typography.List>
      <Typography.Blockquote>A heading is the outline, not a font size.</Typography.Blockquote>
      <Typography.Muted>Published 26 September 2026.</Typography.Muted>
    </article>
  );
}
