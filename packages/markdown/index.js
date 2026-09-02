// @flow
//
// `@uniflowed/markdown`.

import type * as React from "@uniflowed/react";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/markdown";

export type MarkdownOptions = {
  +cache?: "opt-in",
  +rsc?: true,
};

export type MdxOptions = MarkdownOptions & {
  +components?: { +[string]: mixed },
  +jsxImportSource?: "@uniflowed/jsx-runtime",
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

export component Mdx(source: string, options?: MdxOptions) renders React.Node {
  return nativeRuntimeRequired(MODULE, "Mdx");
}

export function renderMdx(source: string, options?: MdxOptions): Promise<string> {
  return nativeRuntimeRequired(MODULE, "renderMdx");
}

export function compileMdx(
  source: string,
  options?: MdxOptions,
): Promise<React.Node> {
  return nativeRuntimeRequired(MODULE, "compileMdx");
}
