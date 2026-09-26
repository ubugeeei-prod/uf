// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import * as Tooltip from "./tooltip.js";

const styles = stylex.create({
  toolbar: {
    display: "flex",
    gap: ufTokens.space1,
  },
});

/** Two icon buttons in a toolbar, sharing one clock. */
export component Example() {
  return (
    <Tooltip.Provider>
      <div {...props(styles.toolbar)} aria-label="Formatting" role="toolbar">
        <Tooltip.Root>
          <Tooltip.Trigger aria-label="Bold" size="icon" tone="ghost">
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
          </Tooltip.Trigger>
          <Tooltip.Content>Bold (⌘B)</Tooltip.Content>
        </Tooltip.Root>
        <Tooltip.Root>
          <Tooltip.Trigger aria-label="Italic" size="icon" tone="ghost">
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
          </Tooltip.Trigger>
          <Tooltip.Content>Italic (⌘I)</Tooltip.Content>
        </Tooltip.Root>
      </div>
    </Tooltip.Provider>
  );
}
