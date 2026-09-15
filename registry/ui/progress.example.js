// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Progress } from "./progress.js";

const styles = stylex.create({
  stack: {
    display: "grid",
    gap: ufTokens.space2,
    maxWidth: "24rem",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
  },
});

/** An upload that knows how far it has got, and an export that does not yet. */
export component Example() {
  return (
    <div {...props(styles.stack)}>
      <span id="upload-progress">Uploading photos</span>
      <Progress aria-labelledby="upload-progress" max={10} value={3} valueText="3 of 10 photos" />
      <span id="export-progress">Preparing the export</span>
      <Progress aria-labelledby="export-progress" />
    </div>
  );
}
