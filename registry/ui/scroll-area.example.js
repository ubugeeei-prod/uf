// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { ScrollArea } from "./scroll-area.js";

const RELEASES: $ReadOnlyArray<string> = Array.from(
  { length: 30 },
  (_, index) => `v1.${30 - index}.0`,
);

const styles = stylex.create({
  area: {
    width: "12rem",
    height: "14rem",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
  },
  list: {
    margin: 0,
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    listStyle: "none",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
  },
  item: {
    paddingBlock: ufTokens.space1,
  },
});

/** Thirty releases in a box that shows about ten. */
export component Example() {
  return (
    <ScrollArea label="Releases" xstyle={styles.area}>
      <ul {...props(styles.list)}>
        {RELEASES.map((release) => (
          <li key={release} {...props(styles.item)}>
            {release}
          </li>
        ))}
      </ul>
    </ScrollArea>
  );
}
