// @flow
//
// Kbd: a key, or a combination of keys, that a reader presses, drawn as a
// keycap.
//
// `uf ui add kbd` wrote this file into the project, and it is the project's
// from then on. `uf ui diff kbd` shows how it has moved away from the
// registry in the uf you are running.
//
// # Why a component here, when `@uniflowed/ui` has none
//
// `<kbd>` is the platform's element for keyboard input. It has no role, no
// state and nothing to focus, so `@uniflowed/ui` declines a `Kbd`
// (`crates/uf_lib/src/ui.rs` records it). What an application wants is every
// shortcut in its menus, tooltips and help pages drawn the same way, and that
// is a decision about the application, so it is written here.
//
// # What to keep true when you change it
//
// * **It stays a `<kbd>`.** The element is what tells a reader, a search
//   engine and a stylesheet that this is a key rather than any short grey
//   word.
// * **A combination is keys inside one key.** The HTML specification writes
//   `Ctrl+K` as a `<kbd>` holding a `<kbd>` for each key, and `keys` renders
//   exactly that: the outer element draws nothing, the inner ones draw the
//   caps, and the `+` between them is text, so it is read as "Control plus K".
// * **A symbol is not a name.** `⌘` is read as "place of interest sign", or not
//   at all. Write the key's name ("Cmd"), or give the symbol a `title` and put
//   the name in visually hidden text beside it.
// * **It is not a control.** A keycap that runs its shortcut when clicked is a
//   `Button`, because a keyboard user has to be able to reach it.
// * **Text stays on a measured pair.** `ink` on `sunken`, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both shipped
//   themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

/**
 * Every prop a caller passes that this file does not name, on its way to the
 * element. `key` is `empty` because React takes it off before a component is
 * called.
 */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  cap: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    boxSizing: "border-box",
    minWidth: "20px",
    minHeight: "20px",
    paddingBlock: 0,
    paddingInline: ufTokens.space1,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textXs,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
    whiteSpace: "nowrap",
    verticalAlign: "middle",
    color: ufTokens.ink,
    backgroundColor: ufTokens.sunken,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusSm,
  },
  combination: {
    display: "inline-flex",
    alignItems: "center",
    gap: ufTokens.space1,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textXs,
    color: ufTokens.muted,
    whiteSpace: "nowrap",
    verticalAlign: "middle",
  },
});

/**
 * A key, or with `keys` a combination of them.
 *
 *     <Kbd>Esc</Kbd>
 *     <Kbd keys={["Ctrl", "K"]} />
 *
 * `xstyle` takes a `stylex.create` namespace and wins property by property;
 * `className` adds a class of your own beside these. Both go on the outer
 * element.
 */
export component Kbd(
  children?: React.Node,
  keys?: $ReadOnlyArray<string>,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  if (keys == null) {
    return (
      <kbd {...rest} className={classNames(props(styles.cap, xstyle).className, className)}>
        {children}
      </kbd>
    );
  }
  return (
    <kbd {...rest} className={classNames(props(styles.combination, xstyle).className, className)}>
      {keys.map((key, index) => (
        <React.Fragment key={`${index}:${key}`}>
          {index > 0 ? "+" : null}
          <kbd {...props(styles.cap)}>{key}</kbd>
        </React.Fragment>
      ))}
    </kbd>
  );
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
