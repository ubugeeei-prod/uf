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
        <AvatarImage src="/people/ada.png" />
        <AvatarFallback>AL</AvatarFallback>
      </Avatar>
      <span>Ada Lovelace</span>
    </div>
  );
}
