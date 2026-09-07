// @flow
//
// The components a caller writes, and the hooks they reach for.
//
// Four components, and the choice of which four is most of the argument of the
// first two releases. `Box` is a flex container that can draw a frame around
// itself; `Text` is a run of styled characters that knows how to wrap; `Input`
// is a line a reader types into; `ScrollBox` is a window onto content taller
// than it. Everything else OpenTUI offers — a select, a table, a diff view — is
// those plus state, and shipping them badly is worse than not shipping them, so
// they are ubugeeei-prod/uf#314 rather than stubs that throw.
//
// `ScrollBox` is the exception to "plus state", which is why it is a component
// here rather than something a caller writes: which children are laid out and
// painted depends on where the window is, and nothing above the renderer can
// decide that.
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
// snapshot that is stable between resizes; `ScrollBox` owns no scroll state at
// all. All four are safe under Strict Mode's double invocation and under the
// React Compiler's memoization.

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
import type { MouseEvent } from "./mouse.js";
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

/**
 * The mouse handlers a node can carry, under OpenTUI's names.
 *
 * Every one of them receives events that started on this box *or on anything
 * inside it*, because a mouse event bubbles: a panel can handle a click
 * anywhere in it without every child forwarding one. `event.target` says which
 * box the pointer is over and `event.currentTarget` says which box is handling
 * it, exactly as they do in a browser, and `event.stopPropagation()` is how a
 * child keeps one to itself.
 *
 * A captured drag is the one case where those two are not on the same path:
 * the event goes to the box the press landed on — `event.source` — while
 * `event.target` keeps naming what the pointer has since moved over, because
 * that is what a drag handler needs in order to know what it would drop onto.
 *
 * Nothing arrives unless the application asked `render` for the mouse. A tree
 * with handlers on it and `mouse: false` is not an error and is not silently
 * broken either — it is an application that has not turned the device on, and
 * `testRender` turns it on by default so that a test does not have to.
 */
export type MouseProps = {
  /** Every mouse event, after the handler for its own type. */
  readonly onMouse?: (event: MouseEvent) => void,
  readonly onMouseDown?: (event: MouseEvent) => void,
  readonly onMouseUp?: (event: MouseEvent) => void,
  /** The pointer moved over this box with nothing held down. */
  readonly onMouseMove?: (event: MouseEvent) => void,
  /** The pointer moved with a button held, since it was pressed on this box. */
  readonly onMouseDrag?: (event: MouseEvent) => void,
  /** That drag ended, wherever the pointer had reached. */
  readonly onMouseDragEnd?: (event: MouseEvent) => void,
  /** A drag that began somewhere else ended here; `event.source` says where. */
  readonly onMouseDrop?: (event: MouseEvent) => void,
  /** The pointer entered this box, or a box inside it. */
  readonly onMouseOver?: (event: MouseEvent) => void,
  /** And left it. */
  readonly onMouseOut?: (event: MouseEvent) => void,
  /** The wheel turned; `event.scroll` says which way. */
  readonly onMouseScroll?: (event: MouseEvent) => void,
};

/** Everything a `Box` accepts beyond its children. */
export type BoxProps = {
  ...MouseProps,
  ...BoxLayoutProps,
  ...TextStyleProps,
  /**
   * A name for this box, carried by the mouse events it is involved in.
   *
   * `event.target`, `event.currentTarget` and `event.source` are ids rather
   * than nodes, so a box that a drop has to be able to name needs one. Nothing
   * else reads it, and two boxes with the same id are not an error — the
   * events simply cannot tell them apart.
   */
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

/** Everything a `ScrollBox` accepts beyond its children. */
export type ScrollBoxProps = {
  ...BoxProps,
  /**
   * The first content row to show.
   *
   * Clamped by layout to the range the content actually has, which is what
   * makes `Number.MAX_SAFE_INTEGER` mean "the bottom" — a log that has just
   * grown by a line does not have to know how long it is to keep following
   * it.
   */
  readonly scrollTop?: number,
  /** Whether to draw the bar. On by default; it costs a column. */
  readonly scrollbar?: boolean,
  /** The bar's colour. Falls back to `borderColor`. */
  readonly scrollbarColor?: ColorValue,
};

/**
 * A window onto content taller than itself.
 *
 * Give it a height — an explicit one, or `flexGrow` inside a parent that has
 * one. A `ScrollBox` with neither is as tall as its content and scrolls
 * nothing, which is flexbox behaving correctly and not what anybody meant; and
 * between a header and a footer those two want `flexShrink={0}`, because a
 * box asking for ten thousand rows shrinks whatever is allowed to shrink.
 *
 * ```js
 * const [top, setTop] = useState<number>(Number.MAX_SAFE_INTEGER);
 * useKeyboard((key) => {
 *   if (key.name === "up") setTop((row) => Math.max(0, row - 1));
 *   if (key.name === "down") setTop((row) => row + 1);
 * });
 * return (
 *   <ScrollBox height={10} scrollTop={top}>
 *     {lines.map((line) => <Text key={line.id}>{line.text}</Text>)}
 *   </ScrollBox>
 * );
 * ```
 *
 * # Why the offset is the caller's and the keys are not bound
 *
 * OpenTUI's rule for focus is that it is a prop rather than something the
 * library moves for you, and scrolling is the same question one level down:
 * what an arrow key should do inside a scrolling region is the application's
 * business — a log follows its tail, a file viewer does not, and a list moves
 * a selection and lets the box follow *that*. A component that owned the
 * offset would also have to own "how far is a page", which is the viewport's
 * height, which it does not know until after layout has run. Clamping in
 * layout is what lets the caller ask for the bottom without knowing where the
 * bottom is.
 *
 * # The wheel is an event, not a behaviour
 *
 * A wheel over this box arrives as `onMouseScroll`, and moving the offset is
 * still the caller's — the same rule as the keys, for the same reason. Three
 * lines is the whole of it:
 *
 * ```js
 * <ScrollBox
 *   height={10}
 *   scrollTop={top}
 *   onMouseScroll={(event) => {
 *     setTop((row) => Math.max(0, row + (event.scroll?.direction === "up" ? -3 : 3)));
 *   }}
 * />
 * ```
 *
 * Three rows a notch is this example's choice, not this component's: how far a
 * notch goes is a question about the content — a log, a form, a picture — and
 * a component that answered it would be answering it for all three.
 *
 * There is still no horizontal scrolling: a terminal column is not a pixel,
 * and content wider than the window is nearly always content that should have
 * wrapped.
 */
export component ScrollBox(
  children?: React.Node,
  scrollTop?: number = 0,
  scrollbar?: boolean = true,
  scrollbarColor?: ColorValue,
  ...props: BoxProps
) {
  // The bar is drawn in the column the box reserves for it, so wrapped content
  // never reaches it. Reserving it here rather than in the painter is what
  // keeps layout and paint agreeing about how wide a row is.
  //
  // The caller's own right padding is read through both spellings a `Box`
  // accepts, and added to rather than replaced: a `ScrollBox` with `padding={1}`
  // is padded by one and has a bar, not padded by nothing and has a bar.
  const own = props.style ?? props;
  const asked =
    props.paddingRight ??
    own.paddingRight ??
    props.paddingX ??
    own.paddingX ??
    props.padding ??
    own.padding ??
    0;
  const paddingRight = scrollbar ? asked + 1 : props.paddingRight;
  return React.createElement(
    "uf-box",
    {
      ...props,
      paddingRight,
      overflow: "scroll",
      scrollTop,
      scrollbar,
      scrollbarColor,
    },
    children,
  );
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
      // A paste is text somebody had on a clipboard, which is why it is worth
      // knowing it was a paste: this is one line and the clipboard is not, so
      // the first line goes in and the rest is dropped rather than pasted as a
      // series of Enters. Control characters go with it — a pasted `\u0007`
      // is a bell somebody would otherwise hear every time the frame redrew.
      if (key.name === "paste") {
        const first = key.sequence.split(/\r\n|\r|\n/)[0] ?? "";
        const clean = first.replace(/[\u0000-\u001f\u007f]/g, "");
        if (clean !== "") {
          change(text.slice(0, at) + clean + text.slice(at), at + clean.length);
        }
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
