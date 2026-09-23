"use client";
// @flow
//
// Accordion: a stack of headed sections, each opened by its heading, one at a
// time or several.
//
// `uf ui add accordion` wrote this file into the project, and it is the
// project's from then on. `uf ui diff accordion` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The rules between sections, the headings' look, the chevrons and the panels'
// spacing. The pattern is imported from `@uniflowed/ui`'s `Accordion`: each section
// is a real heading holding a button with `aria-expanded`, each panel a region
// named by that button, `type="single"` closing one section as another opens,
// and closed panels kept in the document as `hidden="until-found"` so
// find-in-page still reaches them. The chevron turns from `aria-expanded`.
//
// # What to keep true when you change it
//
// * **The heading level fits the page.** `AccordionTrigger` takes `level`, 3 by
//   default; an accordion under an `<h1>` wants 2, and a skipped level is an
//   outline a reader cannot navigate.
// * **An accordion holds its own items.** `Accordion` takes
//   `renders* AccordionItem`, and an item holds triggers and panels, so a stray
//   `<div>` between sections is a Flow error.
// * **Nothing essential lives only in a closed panel.**
// * **Colour comes from tokens, in measured pairs:** `ink` on `surface` and
//   `muted` on `surface`, which `crates/uf_stylex/src/tests/preset.rs` holds to
//   4.5:1 in both themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import type { AccordionType } from "@uniflowed/ui";
import * as Primitive from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    display: "grid",
    fontFamily: ufTokens.fontSans,
    color: ufTokens.ink,
  },
  item: {
    borderBottomWidth: "1px",
    borderBottomStyle: "solid",
    borderBottomColor: ufTokens.border,
  },
  heading: {
    margin: 0,
    fontSize: "inherit",
    fontWeight: "inherit",
  },
  trigger: {
    // Read by the chevron, which cannot see the button's state.
    "--uf-accordion-turn": { default: "0deg", ":is([aria-expanded=true])": "180deg" },
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: ufTokens.space3,
    boxSizing: "border-box",
    width: "100%",
    minHeight: "44px",
    margin: 0,
    paddingBlock: ufTokens.space3,
    paddingInline: 0,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
    textAlign: "start",
    color: ufTokens.ink,
    backgroundColor: "transparent",
    borderWidth: 0,
    borderRadius: ufTokens.radiusSm,
    textDecorationLine: { default: "none", ":hover": "underline" },
    cursor: {
      default: "pointer",
      ":disabled": "not-allowed",
      ":is([aria-disabled=true])": "not-allowed",
    },
    opacity: { default: 1, ":disabled": 0.55, ":is([aria-disabled=true])": 0.55 },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  chevron: {
    flexShrink: 0,
    color: ufTokens.muted,
    transform: "rotate(var(--uf-accordion-turn))",
    transitionProperty: "transform",
    transitionDuration: {
      default: ufTokens.durationFast,
      "@media (prefers-reduced-motion: reduce)": "0s",
    },
    transitionTimingFunction: ufTokens.easing,
  },
  content: {
    paddingBottom: ufTokens.space4,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
  },
});

/**
 * The stack. `type="single"` keeps one section open, and `collapsible` lets it
 * close again; `type="multiple"` opens any number.
 */
export component Accordion(
  children: renders* AccordionItem,
  type?: AccordionType = "single",
  collapsible?: boolean = true,
  defaultValue?: $ReadOnlyArray<string>,
  value?: $ReadOnlyArray<string>,
  onValueChange?: (value: $ReadOnlyArray<string>) => void,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.Accordion.Root
      {...forwarded(rest)}
      className={classNames(props(styles.root, xstyle).className, className)}
      collapsible={collapsible}
      defaultValue={defaultValue}
      onValueChange={onValueChange}
      type={type}
      value={value}
    >
      {children}
    </Primitive.Accordion.Root>
  );
}

/** One section: its trigger and its panel. */
export component AccordionItem(
  value: string,
  children: renders* (AccordionTrigger | AccordionContent),
  disabled?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.Accordion.Item {
  return (
    <Primitive.Accordion.Item
      {...forwarded(rest)}
      className={classNames(props(styles.item, xstyle).className, className)}
      disabled={disabled}
      value={value}
    >
      {children}
    </Primitive.Accordion.Item>
  );
}

/** A section's heading, and the button inside it that opens the section. */
export component AccordionTrigger(
  children: React.Node,
  level?: number = 3,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.Accordion.Header {
  return (
    <Primitive.Accordion.Header className={props(styles.heading).className} level={level}>
      <Primitive.Accordion.Trigger
        {...forwarded(rest)}
        className={classNames(props(styles.trigger, xstyle).className, className)}
      >
        {children}
        <svg
          {...props(styles.chevron)}
          aria-hidden="true"
          fill="none"
          focusable="false"
          height="16"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth="2"
          viewBox="0 0 24 24"
          width="16"
        >
          <path d="m6 9 6 6 6-6" />
        </svg>
      </Primitive.Accordion.Trigger>
    </Primitive.Accordion.Header>
  );
}

/** A section's panel, named by its trigger. */
export component AccordionContent(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.Accordion.Content {
  return (
    <Primitive.Accordion.Content
      {...forwarded(rest)}
      className={classNames(props(styles.content, xstyle).className, className)}
    >
      {children}
    </Primitive.Accordion.Content>
  );
}

/**
 * A caller's props on their way into a part rather than onto an element. Flow
 * checks that spread against the part's own `...rest`, whose `key` is `empty`
 * where this file's indexer says `mixed`; `@uniflowed/ui` papers over the same
 * hole the same way, and nothing checked is lost, because the elements its
 * parts render have `any`-typed props in uf's library today.
 */
function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
