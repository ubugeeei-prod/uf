// @flow
import { stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import {
  ContextMenu,
  ContextMenuCheckboxItem,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "./context-menu.js";

const styles = stylex.create({
  area: {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    boxSizing: "border-box",
    width: "100%",
    maxWidth: "20rem",
    height: "8rem",
    color: ufTokens.muted,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    borderWidth: "1px",
    borderStyle: "dashed",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
  },
});

/** An area whose menu opens where it is right-clicked or long-pressed. */
export component Example() {
  return (
    <ContextMenu>
      <ContextMenuTrigger tabIndex={0} xstyle={styles.area}>
        Right-click here
      </ContextMenuTrigger>
      <ContextMenuContent aria-label="Page actions">
        <ContextMenuItem>Back</ContextMenuItem>
        <ContextMenuItem disabled>Forward</ContextMenuItem>
        <ContextMenuItem>Reload</ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuCheckboxItem defaultChecked>Show bookmarks</ContextMenuCheckboxItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
