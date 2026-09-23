// @flow
import { stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import * as ContextMenu from "./context-menu.js";

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
    <ContextMenu.Root>
      <ContextMenu.Trigger tabIndex={0} xstyle={styles.area}>
        Right-click here
      </ContextMenu.Trigger>
      <ContextMenu.Content aria-label="Page actions">
        <ContextMenu.Item>Back</ContextMenu.Item>
        <ContextMenu.Item disabled>Forward</ContextMenu.Item>
        <ContextMenu.Item>Reload</ContextMenu.Item>
        <ContextMenu.Separator />
        <ContextMenu.CheckboxItem defaultChecked>Show bookmarks</ContextMenu.CheckboxItem>
      </ContextMenu.Content>
    </ContextMenu.Root>
  );
}
