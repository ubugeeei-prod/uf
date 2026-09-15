// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Alert, AlertDescription, AlertTitle } from "./alert.js";

const styles = stylex.create({
  stack: {
    display: "grid",
    gap: ufTokens.space3,
    maxWidth: "32rem",
  },
});

/** Two notices that were on the page when it loaded, so neither is live. */
export component Example() {
  return (
    <div {...props(styles.stack)}>
      <Alert tone="info">
        <AlertTitle level={2}>Your trial ends in 3 days</AlertTitle>
        <AlertDescription>Choose a plan to keep your projects after Friday.</AlertDescription>
      </Alert>
      <Alert tone="danger">
        <AlertTitle level={2}>The last export failed</AlertTitle>
        <AlertDescription>
          The file was too large. Try exporting one project at a time.
        </AlertDescription>
      </Alert>
    </div>
  );
}
