// @flow
//
// `@uniflowed/markdown`.

import type * as React from "./react.js";
import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/markdown";

export type MarkdownOptions = {
  +cache?: "opt-in",
  +rsc?: true,
};

export component Markdown(
  source: string,
  options?: MarkdownOptions,
) renders React.Node {
  return nativeRuntimeRequired(MODULE, "Markdown");
}

export function renderMarkdown(
  source: string,
  options?: MarkdownOptions,
): Promise<string> {
  return nativeRuntimeRequired(MODULE, "renderMarkdown");
}

export function compileMarkdown(
  source: string,
  options?: MarkdownOptions,
): Promise<React.Node> {
  return nativeRuntimeRequired(MODULE, "compileMarkdown");
}
