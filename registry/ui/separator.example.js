// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Separator } from "./separator.js";

const styles = stylex.create({
  text: {
    margin: 0,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
  },
  row: {
    display: "flex",
    alignItems: "center",
    height: "24px",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
  },
  link: {
    color: ufTokens.ink,
  },
});

/** A boundary between two sections, and decorative rules between links. */
export component Example() {
  return (
    <div>
      <p {...props(styles.text)}>Your name, your email and your password.</p>
      <Separator />
      <nav {...props(styles.row)} aria-label="Account">
        <a {...props(styles.link)} href="?section=profile">
          Profile
        </a>
        <Separator decorative orientation="vertical" />
        <a {...props(styles.link)} href="?section=billing">
          Billing
        </a>
      </nav>
    </div>
  );
}
