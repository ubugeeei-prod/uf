"use client";
// @flow
// NumberField: a localized numeric input with named step buttons.
//
// `uf ui add number-field` copies this component into the project. The headless
// primitive owns semantics and input behavior; this file owns its presentation.
// Keep accessible names, focus rings and disabled/selected states when editing.
import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

type Rest = { readonly key?: empty, readonly [string]: mixed };
const styles = stylex.create({
  root: { display: "inline-flex", alignItems: "center", gap: ufTokens.space1 },
  input: {
    boxSizing: "border-box",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderRadius: ufTokens.radiusMd,
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
    width: "8rem",
  },
  step: {
    boxSizing: "border-box",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderRadius: ufTokens.radiusSm,
    minWidth: "36px",
    minHeight: "36px",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    cursor: { default: "pointer", ":disabled": "not-allowed" },
    color: { default: ufTokens.ink, ":disabled": ufTokens.muted },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
});
export component NumberField(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.NumberField.Root
      {...forwarded(rest)}
      className={classNames(props(styles.root, xstyle).className, className)}
    >
      {children}
    </Primitive.NumberField.Root>
  );
}
export component NumberFieldInput(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  return (
    <Primitive.NumberField.Input
      {...forwarded(rest)}
      className={classNames(props(styles.input, xstyle).className, className)}
    />
  );
}
export component NumberFieldIncrement(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.NumberField.Increment
      {...forwarded(rest)}
      className={classNames(props(styles.step, xstyle).className, className)}
    >
      {children}
    </Primitive.NumberField.Increment>
  );
}
export component NumberFieldDecrement(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.NumberField.Decrement
      {...forwarded(rest)}
      className={classNames(props(styles.step, xstyle).className, className)}
    >
      {children}
    </Primitive.NumberField.Decrement>
  );
}

function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
