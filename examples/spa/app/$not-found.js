// @flow
import { Link } from "@uniflowed/router";
import { props, stylex } from "@uniflowed/stylex";

import { focus, tokens } from "./tokens.stylex.js";

export component NotFound() {
  return (
    <>
      <h1 {...props(styles.title)}>Nothing here yet.</h1>
      <p>
        <Link {...props(styles.back, focus.ring)} to="/">
          Return to your board
        </Link>
      </p>
    </>
  );
}

const styles = stylex.create({
  title: {
    maxWidth     : "15ch",
    margin       : "0 0 2rem",
    fontSize     : "clamp(2rem, 5vw, 3rem)",
    lineHeight   : "1.1",
    letterSpacing: "-0.04em",
  },
  back: {
    color              : tokens.accent,
    textUnderlineOffset: "0.2em",
  },
});
