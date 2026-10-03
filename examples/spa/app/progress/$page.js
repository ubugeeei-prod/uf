// @flow
"use client";

import { Link } from "@uniflowed/router";
import { useAtomValue } from "@uniflowed/state";
import { props, stylex } from "@uniflowed/stylex";

import { focus, tokens } from "../tokens.stylex.js";
import { completed, tasks } from "../model.js";

export component Page() {
  const total = useAtomValue(tasks).length;
  const done  = useAtomValue(completed);
  return (
    <>
      <p {...props(styles.eyebrow)}>A little at a time</p>
      <h1 {...props(styles.title)}>Your progress.</h1>
      <p {...props(styles.count)}>
        {done}
        <span {...props(styles.countRest)}> / {total}</span>
      </p>
      <p>
        {
          match (total === 0) {
            true  => "A clear board. Start whenever you are ready.",
            false => `${total - done} tasks left to make a little space.`,
          }
        }
      </p>
      <progress
        {...props(styles.meter, focus.ring)}
        value={done}
        max={Math.max(total, 1)}
        aria-label="Completed tasks"
      />
      <p>
        <Link {...props(styles.back, focus.ring)} to="/">
          Back to the board
        </Link>
      </p>
    </>
  );
}

const styles = stylex.create({
  eyebrow: {
    color         : tokens.muted,
    fontSize      : "12px",
    letterSpacing : "0.06em",
    textTransform : "uppercase",
  },
  title: {
    maxWidth     : "15ch",
    margin       : "0 0 2rem",
    fontSize     : "clamp(2rem, 5vw, 3rem)",
    lineHeight   : "1.1",
    letterSpacing: "-0.04em",
  },
  count: {
    margin        : "0",
    fontSize      : "64px",
    letterSpacing : "-0.04em",
  },
  countRest: {
    fontSize: "32px",
    color   : tokens.muted,
  },
  meter: {
    display   : "block",
    width     : "100%",
    height    : "10px",
    margin    : "2rem 0",
    accentColor: tokens.accent,
  },
  back: {
    color              : tokens.accent,
    textUnderlineOffset: "0.2em",
  },
});
