// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import * as Resizable from "./resizable.js";

const styles = stylex.create({
  frame: {
    width: "100%",
    maxWidth: "32rem",
    height: "12rem",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
    overflow: "hidden",
  },
  pane: {
    boxSizing: "border-box",
    height: "100%",
    padding: ufTokens.space4,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
  },
});

/** A file list and an editor, the list between a fifth and four fifths wide. */
export component Example() {
  return (
    <div {...props(styles.frame)}>
      <Resizable.PanelGroup defaultValue={30} max={80} min={20}>
        <Resizable.Panel primary>
          <div {...props(styles.pane)}>Files</div>
        </Resizable.Panel>
        <Resizable.Handle label="Resize file list" />
        <Resizable.Panel>
          <div {...props(styles.pane)}>Editor</div>
        </Resizable.Panel>
      </Resizable.PanelGroup>
    </div>
  );
}
