// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { ToggleGroup, ToggleGroupItem } from "./toggle-group.js";

const styles = stylex.create({
  stack: {
    display: "grid",
    justifyItems: "start",
    gap: ufTokens.space3,
  },
});

/** Formatting, any number of which can be on, and an alignment, which is one. */
export component Example() {
  return (
    <div {...props(styles.stack)}>
      <ToggleGroup aria-label="Formatting" defaultValue={["bold"]} type="multiple">
        <ToggleGroupItem aria-label="Bold" value="bold">
          <svg
            aria-hidden="true"
            fill="none"
            focusable="false"
            height="16"
            stroke="currentColor"
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth="2.5"
            viewBox="0 0 24 24"
            width="16"
          >
            <path d="M7 5h6a3.5 3.5 0 0 1 0 7H7zM7 12h7a3.5 3.5 0 0 1 0 7H7z" />
          </svg>
        </ToggleGroupItem>
        <ToggleGroupItem aria-label="Italic" value="italic">
          <svg
            aria-hidden="true"
            fill="none"
            focusable="false"
            height="16"
            stroke="currentColor"
            strokeLinecap="round"
            strokeWidth="2"
            viewBox="0 0 24 24"
            width="16"
          >
            <path d="M14 5h-4M14 19h-4M13 5l-2 14" />
          </svg>
        </ToggleGroupItem>
      </ToggleGroup>
      <ToggleGroup aria-label="Alignment" defaultValue={["left"]} type="single">
        <ToggleGroupItem value="left">Left</ToggleGroupItem>
        <ToggleGroupItem value="center">Centre</ToggleGroupItem>
        <ToggleGroupItem value="right">Right</ToggleGroupItem>
      </ToggleGroup>
    </div>
  );
}
