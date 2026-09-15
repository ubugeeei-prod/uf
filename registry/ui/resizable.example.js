// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "./resizable.js";

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
      <ResizablePanelGroup defaultValue={30} max={80} min={20}>
        <ResizablePanel primary>
          <div {...props(styles.pane)}>Files</div>
        </ResizablePanel>
        <ResizableHandle label="Resize file list" />
        <ResizablePanel>
          <div {...props(styles.pane)}>Editor</div>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}
