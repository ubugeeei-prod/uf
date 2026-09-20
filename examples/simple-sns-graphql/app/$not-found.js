// @flow

import * as React from "@uniflowed/react";

import { SocialFrame } from "./_shared/social-frame.js";
import { EmptyState, ActionLink } from "./_shared/ui.js";

/** Render a recoverable destination for an unmatched route. */

export component Page() {
  return (
    <SocialFrame active="timeline" aside={false}>
      <EmptyState title="Page not found" action={<ActionLink to="/">Back to feed</ActionLink>}>
        This page may have moved, or the address may be incorrect.
      </EmptyState>
    </SocialFrame>
  );
}
