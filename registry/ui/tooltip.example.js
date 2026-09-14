// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "./tooltip.js";

const styles = stylex.create({
  toolbar: {
    display: "flex",
    gap: ufTokens.space1,
  },
});

/** Two icon buttons in a toolbar, sharing one clock. */
export component Example() {
  return (
    <TooltipProvider>
      <div {...props(styles.toolbar)} aria-label="Formatting" role="toolbar">
        <Tooltip>
          <TooltipTrigger aria-label="Bold" size="icon" tone="ghost">
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
          </TooltipTrigger>
          <TooltipContent>Bold (⌘B)</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger aria-label="Italic" size="icon" tone="ghost">
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
          </TooltipTrigger>
          <TooltipContent>Italic (⌘I)</TooltipContent>
        </Tooltip>
      </div>
    </TooltipProvider>
  );
}
