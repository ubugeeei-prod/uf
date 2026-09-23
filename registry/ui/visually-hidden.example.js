// @flow
import * as React from "@uniflowed/react";
import { VisuallyHidden } from "./visually-hidden.js";

/** A skip link that appears when a keyboard reaches it, and a named icon button. */
export component Example() {
  return (
    <div>
      <VisuallyHidden focusable>
        <a href="#content">Skip to content</a>
      </VisuallyHidden>
      <button type="button">
        <span aria-hidden="true">×</span>
        <VisuallyHidden>Close the panel</VisuallyHidden>
      </button>
      <main id="content">Content</main>
    </div>
  );
}
