// @flow
//
// The components a caller writes, and the hooks they reach for.
//
// Three components, and the choice of which three is the whole argument of
// this first release. `Box` is a flex container that can draw a frame around
// itself; `Text` is a run of styled characters that knows how to wrap; `Input`
// is a line a reader types into. Everything else OpenTUI offers — a select, a
// scroll box, a table, a diff view — is those three plus state, and shipping
// them badly is worse than not shipping them, so they are ubugeeei-prod/uf#314
// rather than stubs that throw.
//
// # Why these are `component`s and not intrinsic elements
//
// OpenTUI's React binding gives you `<box>` and `<text>` — lowercase JSX
// intrinsics, resolved by the renderer. That is not available here and should
// not be: Flow resolves a lowercase JSX name against React's *DOM* intrinsics,
// so `<box>` is either an error or, worse, silently the HTML element of that
// name. Flow has a lint for exactly this collision — `react-intrinsic-overlap`
// — which is a good sign that the collision is real and a bad way to live with
// it.
//
// So the public surface is capitalised `component`s, and the host element
// names they create — `"uf-box"`, `"uf-text"` — are an implementation detail
// nothing outside this package writes. The gain is the one Flow exists for:
// `<Box padding="2">` is a type error at the call site rather than a padding
// of `NaN` at run time.
//
// # What a component here may assume about React
//
// Nothing that `ubugeeei-redundancy.md` forbids. Nothing below mutates during
// render, reads a ref during render, or depends on a render happening exactly
// once. `Input` keeps its cursor in state, not in a ref that a render reads;
// `useTerminalSize` subscribes with `useSyncExternalStore` and returns a
// snapshot that is stable between resizes. All three are safe under Strict
// Mode's double invocation and under the React Compiler's memoization.

import * as React from "@uniflowed/react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "@uniflowed/react";

import type { BorderStyle } from "./capability.js";
import type { KeyEvent } from "./keys.js";
import type {
  AlignItems,
  AlignSelf,
  Dimension,
  FlexDirection,
  JustifyContent,
  LayoutStyle,
  Overflow,
} from "./layout.js";
import type { WrapMode } from "./internal/paint.js";
import type { Renderer } from "./internal/host.js";
import { RendererContext } from "./internal/host.js";

/** A colour, as `"#rrggbb"`, one of the sixteen names, or a packed number. */
export type ColorValue = string | number;

/** Where a border title sits along its edge. */
export type TitleAlignment = "left" | "center" | "right";

/**
 * Everything that positions a node.
 *
 * Accepted both as individual props and inside `style`, exactly as OpenTUI
 * accepts them, because both spellings are in its documentation and a caller
 * copying an example should not have to translate.
 */
export type BoxLayoutProps = {
  readonly flexDirection?: FlexDirection,
  readonly justifyContent?: JustifyContent,
  readonly alignItems?: AlignItems,
  readonly alignSelf?: AlignSelf,
  readonly flexGrow?: number,
  readonly flexShrink?: number,
  readonly flexBasis?: Dimension,
  readonly width?: Dimension,
  readonly height?: Dimension,
  readonly minWidth?: Dimension,
  readonly minHeight?: Dimension,
  readonly maxWidth?: Dimension,
  readonly maxHeight?: Dimension,
  readonly padding?: number,
  readonly paddingX?: number,
  readonly paddingY?: number,
  readonly paddingTop?: number,
  readonly paddingRight?: number,
  readonly paddingBottom?: number,
  readonly paddingLeft?: number,
  readonly margin?: number,
  readonly marginTop?: number,
  readonly marginRight?: number,
  readonly marginBottom?: number,
  readonly marginLeft?: number,
  readonly gap?: number,
  readonly rowGap?: number,
  readonly columnGap?: number,
  readonly overflow?: Overflow,
};

/** Everything that paints a node's text. Inherited by nested `Text`. */
export type TextStyleProps = {
  readonly fg?: ColorValue,
  readonly bg?: ColorValue,
  readonly bold?: boolean,
  readonly dim?: boolean,
  readonly italic?: boolean,
  readonly underline?: boolean,
  readonly blink?: boolean,
  readonly inverse?: boolean,
  readonly strikethrough?: boolean,
};

/** Everything a `Box` accepts beyond its children. */
export type BoxProps = {
  ...BoxLayoutProps,
  ...TextStyleProps,
  readonly id?: string,
  readonly style?: BoxLayoutProps,
  readonly backgroundColor?: ColorValue,
  readonly border?: boolean,
  readonly borderStyle?: BorderStyle,
  readonly borderColor?: ColorValue,
  readonly title?: string,
  readonly titleColor?: ColorValue,
  readonly titleAlignment?: TitleAlignment,
  readonly bottomTitle?: string,
  readonly bottomTitleAlignment?: TitleAlignment,
  /** Whether this box may hold focus at all. */
  readonly focusable?: boolean,
  /** Whether it holds focus now. Focus is state, as it is in OpenTUI. */
  readonly focused?: boolean,
  /** Keys delivered to this box while it holds focus. */
  readonly onKeyDown?: (key: KeyEvent) => void,
};

/** A flex container that can draw a background, a border, and two titles. */
export component Box(children?: React.Node, ...props: BoxProps) {
  return React.createElement("uf-box", props, children);
}

/** Everything a `Text` accepts beyond its children. */
export type TextProps = {
  ...BoxLayoutProps,
  ...TextStyleProps,
  readonly id?: string,
  readonly style?: BoxLayoutProps,
  /** How lines break: at word boundaries, anywhere, or not at all. */
  readonly wrap?: WrapMode,
};

/**
 * A run of styled text.
 *
 * Nest one inside another to change part of a line without repeating the
 * style of the rest: the inner one inherits every attribute the outer one set
 * and overrides only what it names.
 */
export component Text(children?: React.Node, ...props: TextProps) {
  return React.createElement("uf-text", props, children);
}

/**
 * The renderer this tree is mounted in.
 *
 * Raises rather than returning `null` when there is none, because every way to
 * reach this hook goes through a mounted root and a `null` here means the
 * component is being rendered by something else — `react-dom`, say, which will
 * then fail much further away with a message about `uf-box` not being a valid
 * HTML element.
 */
export function useRenderer(): Renderer {
  const renderer = React.useContext(RendererContext);
  if (renderer == null) {
    throw new Error(
      "@uniflowed/tui: a component was rendered outside a TUI root. " +
        "Mount it with `render()` from @uniflowed/tui.",
    );
  }
  return renderer;
}

/**
 * Handle keys before the focused node sees them.
 *
 * Registered in mount order and removed on cleanup, so a handler belonging to
 * a component that has unmounted cannot receive a key — the leak that makes an
 * application respond to a shortcut belonging to a screen it has left.
 *
 * The handler is kept in a ref and the subscription depends on nothing, which
 * is deliberate: a caller who writes `useKeyboard((key) => …)` with an inline
 * arrow would otherwise re-subscribe on every render, and the order handlers
 * run in — which OpenTUI specifies as registration order — would silently
 * become "whichever component rendered last".
 */
export function useKeyboard(handler: (key: KeyEvent) => void): void {
  const renderer = useRenderer();
  const latest = useRef(handler);
  useEffect(() => {
    latest.current = handler;
  });
  useEffect(() => {
    const listener = (key: KeyEvent) => latest.current(key);
    renderer.keyHandlers.push(listener);
    return () => {
      const at = renderer.keyHandlers.indexOf(listener);
      if (at >= 0) {
        renderer.keyHandlers.splice(at, 1);
      }
    };
  }, [renderer]);
}

/** The terminal's current size, re-rendering the caller when it changes. */
export function useTerminalSize(): { readonly width: number, readonly height: number } {
  const renderer = useRenderer();
  const subscribe = useCallback(
    (notify: () => void) => {
      renderer.sizeListeners.add(notify);
      return () => {
        renderer.sizeListeners.delete(notify);
      };
    },
    [renderer],
  );
  const snapshot = useCallback(() => renderer.size, [renderer]);
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}

/** Everything an `Input` accepts. */
export type InputProps = {
  ...BoxLayoutProps,
  /** The current text, when the caller controls it. */
  readonly value?: string,
  /** The initial text, when it does not. */
  readonly defaultValue?: string,
  /** What to show when the value is empty. */
  readonly placeholder?: string,
  /** Whether this input has focus. */
  readonly focused?: boolean,
  /** Called with the new text on every edit. */
  readonly onInput?: (value: string) => void,
  /** Called with the text when Enter is pressed. */
  readonly onSubmit?: (value: string) => void,
  readonly fg?: ColorValue,
  readonly bg?: ColorValue,
  readonly placeholderColor?: ColorValue,
};

/**
 * One line of text a reader types into.
 *
 * Controlled when `value` is given and uncontrolled otherwise, which is
 * React's convention and the one a caller expects. The cursor is always this
 * component's own state: it is a property of the editing session and not of
 * the value, and a controlled input whose parent re-sends the same string
 * must not have its cursor jump to the end — the single most common bug in
 * hand-written terminal inputs.
 *
 * The cursor is drawn as an inverse-video cell rather than by moving the
 * terminal's real cursor. A real cursor is one per terminal and this renderer
 * has no idea whether the application wants it here, over a list selection, or
 * hidden; an inverse cell is a property of the frame, so it composes.
 */
export component Input(
  value?: string,
  defaultValue?: string = "",
  placeholder?: string = "",
  focused?: boolean = false,
  onInput?: (value: string) => void,
  onSubmit?: (value: string) => void,
  fg?: ColorValue,
  bg?: ColorValue,
  placeholderColor?: ColorValue = "gray",
  ...layout: BoxLayoutProps
) {
  const [internal, setInternal] = useState<string>(defaultValue);
  const text = value ?? internal;
  const [cursor, setCursor] = useState<number>(text.length);
  const at = Math.min(cursor, text.length);

  const change = useCallback(
    (next: string, nextCursor: number) => {
      if (value == null) {
        setInternal(next);
      }
      setCursor(nextCursor);
      if (onInput != null) {
        onInput(next);
      }
    },
    [onInput, value],
  );

  const onKeyDown = useCallback(
    (key: KeyEvent) => {
      if (key.name === "return") {
        if (onSubmit != null) {
          onSubmit(text);
        }
        return;
      }
      if (key.name === "backspace") {
        if (at > 0) {
          change(text.slice(0, at - 1) + text.slice(at), at - 1);
        }
        return;
      }
      if (key.name === "delete") {
        if (at < text.length) {
          change(text.slice(0, at) + text.slice(at + 1), at);
        }
        return;
      }
      if (key.name === "left") {
        setCursor(Math.max(0, at - 1));
        return;
      }
      if (key.name === "right") {
        setCursor(Math.min(text.length, at + 1));
        return;
      }
      if (key.name === "home") {
        setCursor(0);
        return;
      }
      if (key.name === "end") {
        setCursor(text.length);
        return;
      }
      // Anything with text behind it is an insertion. `sequence` rather than
      // `name`, because `name` is lowercased — typing a capital letter through
      // `name` produces a lowercase one.
      if (!key.ctrl && !key.meta && key.sequence !== "" && key.sequence !== "\r") {
        change(text.slice(0, at) + key.sequence + text.slice(at), at + key.sequence.length);
      }
    },
    [at, change, onSubmit, text],
  );

  const showPlaceholder = text === "" && placeholder !== "";
  const body = useMemo(() => {
    if (showPlaceholder) {
      if (!focused) {
        return React.createElement(Text, { fg: placeholderColor, wrap: "none" }, placeholder);
      }
      return React.createElement(
        Text,
        { fg: placeholderColor, wrap: "none" },
        React.createElement(Text, { inverse: true }, placeholder.slice(0, 1)),
        placeholder.slice(1),
      );
    }
    if (!focused) {
      return React.createElement(Text, { fg, bg, wrap: "none" }, text);
    }
    // Three runs: what is before the cursor, the cell under it, and what is
    // after. The cell under the cursor is a space when the cursor sits past
    // the end of the text, which is where it is while somebody is typing.
    return React.createElement(
      Text,
      { fg, bg, wrap: "none" },
      text.slice(0, at),
      React.createElement(Text, { inverse: true }, at < text.length ? text.slice(at, at + 1) : " "),
      text.slice(at + 1),
    );
  }, [at, bg, fg, focused, placeholder, placeholderColor, showPlaceholder, text]);

  return React.createElement(Box, { ...layout, focusable: true, focused, onKeyDown }, body);
}
