"use client";
// @flow

import * as React from "@uniflowed/react";
import type { ErrorProps } from "@uniflowed/router";
import { EmptyState, RetryButton } from "./ui.js";

/**
 * Offer a route reset after an unexpected loader or render defect, without exposing its details.
 */
export component Error(...{ reset }: ErrorProps) {
  return (
    <EmptyState title="Unable to load this page" action={<RetryButton onRetry={reset} />}>
      We could not load this page. Please try again.
    </EmptyState>
  );
}
