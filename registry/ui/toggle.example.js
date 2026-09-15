// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Toggle } from "./toggle.js";

const styles = stylex.create({
  row: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: ufTokens.space2,
  },
});

/** An icon toggle that is on, an outlined one that is off, and one with words. */
export component Example() {
  return (
    <div {...props(styles.row)}>
      <Toggle aria-label="Bold" defaultPressed size="icon">
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
      </Toggle>
      <Toggle aria-label="Italic" size="icon" tone="outline">
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
      </Toggle>
      <Toggle tone="outline">Show hidden files</Toggle>
    </div>
  );
}
