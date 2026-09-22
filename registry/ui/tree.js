"use client";
// @flow
// Tree: a named collection with visible selection and keyboard focus.
//
// `uf ui add tree` copies this component into the project. The headless
// primitive owns semantics and input behavior; this file owns its presentation.
// Keep accessible names, focus rings and disabled/selected states when editing.
import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

type Rest = { readonly key?: empty, readonly [string]: mixed };
import type { CollectionItem, CollectionItemState } from "@uniflowed/ui";
const styles = stylex.create({
  root: {
    boxSizing: "border-box",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderRadius: ufTokens.radiusSm,
    minWidth: "12rem",
    padding: ufTokens.space1,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  row: {
    display: "block",
    boxSizing: "border-box",
    minHeight: "28px",
    paddingBlock: ufTokens.space1,
    paddingInline: ufTokens.space2,
    borderRadius: ufTokens.radiusSm,
    color: {
      default: ufTokens.ink,
      ":is([data-selected=true])": ufTokens.accentInk,
      ":is([data-disabled=true])": ufTokens.muted,
    },
    backgroundColor: {
      default: "transparent",
      ":is([data-selected=true])": ufTokens.accent,
    },
    outlineWidth: {
      default: "0",
      ":is([data-active=true])": "2px",
    },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "-2px",
  },
});
export component Tree(
  items: $ReadOnlyArray<CollectionItem>,
  children?: (item: CollectionItem, state: CollectionItemState) => React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.Tree
      {...forwarded(rest)}
      items={items}
      className={classNames(props(styles.root, xstyle).className, className)}
    >
      {(item, state) => (
        <span
          {...props(styles.row)}
          data-selected={state.selected || undefined}
          data-disabled={state.disabled || undefined}
          data-active={state.active || undefined}
          style={{ paddingInlineStart: 8 + (state.level - 1) * 16 }}
        >
          {children?.(item, state) ?? item.textValue}
        </span>
      )}
    </Primitive.Tree>
  );
}

function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
