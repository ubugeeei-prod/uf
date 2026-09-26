"use client";
// @flow
// I18nProvider: an explicit locale and direction for a region.
//
// `uf ui add i18n-provider` copies this component into the project. The headless
// primitive owns semantics and input behavior; this file owns its presentation.
// Keep accessible names, focus rings and disabled/selected states when editing.
import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { I18nProvider as HeadlessI18nProvider } from "@uniflowed/ui";

type Rest = { readonly key?: empty, readonly [string]: mixed };
const styles = stylex.create({
  root: { fontFamily: ufTokens.fontSans, color: ufTokens.ink, textAlign: "start" },
});
export component I18nProvider(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <HeadlessI18nProvider
      {...forwarded(rest)}
      className={classNames(props(styles.root, xstyle).className, className)}
    >
      {children}
    </HeadlessI18nProvider>
  );
}

function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
