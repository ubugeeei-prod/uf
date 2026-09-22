"use client";
// @flow
// DateField: locale-aware segments in a bordered field.
//
// `uf ui add date-field` copies this component into the project. The headless
// primitive owns semantics and input behavior; this file owns its presentation.
// Keep accessible names, focus rings and disabled/selected states when editing.
import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

type Rest = { readonly key?: empty, readonly [string]: mixed };
const styles = stylex.create({
  root: {
    boxSizing: "border-box",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderRadius: ufTokens.radiusSm,
    display: "inline-flex",
    alignItems: "center",
    gap: ufTokens.space1,
    minHeight: "36px",
    paddingBlock: ufTokens.space1,
    paddingInline: ufTokens.space2,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: { default: ufTokens.border, ":is([aria-invalid=true])": ufTokens.danger },
    outlineWidth: { default: "0", ":focus-within": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
    fontVariantNumeric: "tabular-nums",
  },
});
export component DateField(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  return (
    <Primitive.DateField
      {...forwarded(rest)}
      className={classNames(props(styles.root, xstyle).className, className)}
    />
  );
}

function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
