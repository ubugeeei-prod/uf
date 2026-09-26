// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import * as Avatar from "./avatar.js";

const styles = stylex.create({
  row: {
    display: "flex",
    alignItems: "center",
    gap: ufTokens.space3,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
  },
});

/** A reviewer beside their name, with initials while the picture is missing. */
export component Example() {
  return (
    <div {...props(styles.row)}>
      <Avatar.Root>
        {/* No picture yet: an empty source is nothing coming, so the initials show at once. */}
        <Avatar.Image src="" />
        <Avatar.Fallback>AL</Avatar.Fallback>
      </Avatar.Root>
      <span>Ada Lovelace</span>
    </div>
  );
}
