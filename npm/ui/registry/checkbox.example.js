// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Checkbox } from "./checkbox.js";

const styles = stylex.create({
  stack: {
    display: "grid",
    justifyItems: "start",
    gap: ufTokens.space1,
  },
});

/** A filter's answers: checked, unchecked, mixed and unavailable. */
export component Example() {
  return (
    <div {...props(styles.stack)}>
      <Checkbox indeterminate>Every project</Checkbox>
      <Checkbox defaultChecked>Include archived projects</Checkbox>
      <Checkbox>Only projects I own</Checkbox>
      <Checkbox disabled>Include deleted projects</Checkbox>
    </div>
  );
}
