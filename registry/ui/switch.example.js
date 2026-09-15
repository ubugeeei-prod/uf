// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Switch } from "./switch.js";

const styles = stylex.create({
  stack: {
    display: "grid",
    justifyItems: "start",
    gap: ufTokens.space2,
  },
});

/** Three notification settings, one of them unavailable. */
export component Example() {
  return (
    <div {...props(styles.stack)}>
      <Switch defaultChecked>Email me when someone replies</Switch>
      <Switch>Show a preview of each message</Switch>
      <Switch disabled>Sync with my calendar</Switch>
    </div>
  );
}
