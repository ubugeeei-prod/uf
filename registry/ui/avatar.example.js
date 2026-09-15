// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Avatar, AvatarFallback, AvatarImage } from "./avatar.js";

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
      <Avatar>
        {/* No picture yet: an empty source is nothing coming, so the initials show at once. */}
        <AvatarImage src="" />
        <AvatarFallback>AL</AvatarFallback>
      </Avatar>
      <span>Ada Lovelace</span>
    </div>
  );
}
