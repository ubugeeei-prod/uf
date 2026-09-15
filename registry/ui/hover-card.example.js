// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { HoverCard, HoverCardContent, HoverCardTrigger } from "./hover-card.js";

const styles = stylex.create({
  stack: {
    display: "grid",
    gap: ufTokens.space2,
  },
  name: {
    margin: 0,
    fontWeight: ufTokens.weightBold,
  },
  note: {
    margin: 0,
    color: ufTokens.muted,
  },
  link: {
    color: ufTokens.ink,
    textDecorationLine: "underline",
    textUnderlineOffset: "3px",
  },
});

/** A name in a sentence, and the profile it previews. */
export component Example() {
  return (
    <div>
      Reviewed by{" "}
      <HoverCard>
        <HoverCardTrigger href="/people/ada">Ada Lovelace</HoverCardTrigger>
        <HoverCardContent>
          <div {...props(styles.stack)}>
            <p {...props(styles.name)}>Ada Lovelace</p>
            <p {...props(styles.note)}>Writes the notes that outlive the engine.</p>
            <a {...props(styles.link)} href="/people/ada/notes">
              Read her notes
            </a>
          </div>
        </HoverCardContent>
      </HoverCard>
      .
    </div>
  );
}
