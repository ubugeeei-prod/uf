"use client";
// @flow
// ColorPicker: hex, native color and channel inputs with a preview.
//
// `uf ui add color-picker` copies this component into the project. The headless
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
    display: "inline-flex",
    flexDirection: "column",
    alignItems: "start",
    gap: ufTokens.space2,
  },
  field: {
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
    width: "10rem",
  },
  input: {
    width: "44px",
    height: "36px",
    padding: "2px",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
  },
  channel: { width: "12rem", accentColor: ufTokens.accent },
  swatch: {
    display: "inline-block",
    width: "36px",
    height: "36px",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusSm,
  },
});
export component ColorPicker(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.ColorPickerRoot
      {...forwarded(rest)}
      className={classNames(props(styles.root, xstyle).className, className)}
    >
      {children}
    </Primitive.ColorPickerRoot>
  );
}
export component ColorPickerInput(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  return (
    <Primitive.ColorPickerInput
      {...forwarded(rest)}
      className={classNames(props(styles.input, xstyle).className, className)}
    />
  );
}
export component ColorPickerField(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  return (
    <Primitive.ColorPickerField
      {...forwarded(rest)}
      className={classNames(props(styles.field, xstyle).className, className)}
    />
  );
}
export component ColorPickerChannel(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  return (
    <Primitive.ColorPickerChannel
      {...forwarded(rest)}
      className={classNames(props(styles.channel, xstyle).className, className)}
    />
  );
}
export component ColorPickerSwatch(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  return (
    <Primitive.ColorPickerSwatch
      {...forwarded(rest)}
      className={classNames(props(styles.swatch, xstyle).className, className)}
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
