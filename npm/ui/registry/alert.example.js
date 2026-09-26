// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import * as Alert from "./alert.js";

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
      <Alert.Root tone="info">
        <Alert.Title level={2}>Your trial ends in 3 days</Alert.Title>
        <Alert.Description>Choose a plan to keep your projects after Friday.</Alert.Description>
      </Alert.Root>
      <Alert.Root tone="danger">
        <Alert.Title level={2}>The last export failed</Alert.Title>
        <Alert.Description>
          The file was too large. Try exporting one project at a time.
        </Alert.Description>
      </Alert.Root>
    </div>
  );
}
