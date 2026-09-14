// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Skeleton, SkeletonBox } from "./skeleton.js";

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
    <Skeleton>
      <div {...props(styles.row)}>
        <SkeletonBox shape="circle" />
        <div {...props(styles.lines)}>
          <SkeletonBox />
          <SkeletonBox xstyle={styles.short} />
        </div>
      </div>
      <SkeletonBox shape="block" />
    </Skeleton>
  );
}
