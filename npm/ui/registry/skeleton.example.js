// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import * as Skeleton from "./skeleton.js";

const styles = stylex.create({
  row: {
    display: "flex",
    alignItems: "center",
    gap: ufTokens.space3,
  },
  lines: {
    display: "grid",
    flexGrow: 1,
    gap: ufTokens.space2,
  },
  short: {
    width: "60%",
  },
});

/** A comment still on its way: an avatar, two lines and the text under them. */
export component Example() {
  return (
    <Skeleton.Root>
      <div {...props(styles.row)}>
        <Skeleton.Box shape="circle" />
        <div {...props(styles.lines)}>
          <Skeleton.Box />
          <Skeleton.Box xstyle={styles.short} />
        </div>
      </div>
      <Skeleton.Box shape="block" />
    </Skeleton.Root>
  );
}
