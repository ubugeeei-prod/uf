// @flow
//
// One rule about prop order, stated once.
//
// `<div {...rest} role="dialog">` and `<div role="dialog" {...rest}>` are
// different components. The second lets a caller pass `role="button"` and get
// it; the first does not. That sounds like a matter of taste until you notice
// what else arrives in `rest`:
//
//   * A caller `ref` replaced the ref the dialog uses to find its focus stops,
//     so `bodyRef.current` stayed null, the Tab handler returned early, and the
//     focus trap was *silently off* while the dialog still announced
//     `aria-modal="true"`.
//   * A caller `onClick` replaced a tab's selection handler, so clicking a tab
//     did nothing.
//   * A caller `onKeyDown` replaced the dialog's, so Escape stopped closing it.
//
// None of those fail loudly. So the rule is: the caller's props go on first and
// the component's own semantics go on last, and for the two kinds of prop where
// a caller legitimately wants *both* — event handlers and refs — they are
// composed rather than one replacing the other.
//
// # The other half of the rule: which element the props land on
//
// Everything above decides what goes onto the element. `RenderProp` is what
// decides *which element*, and it is here rather than in a module of its own
// because it is the same policy read from the other end: a part computes one
// props object, and either puts it on the element it would have chosen or hands
// it to the caller to put on theirs. Two ways of composing a caller's props
// would be two chances to get the order wrong; one shape, stated once, is what
// keeps a part that renders somebody else's element from being a weaker part.
//
// # Why this is `internal/` and not a subpath
//
// It is not a "props utils" module and there is nothing else in it. It is the
// one policy every part of this package applies, extracted so that a new
// primitive cannot quietly apply a different one. Exporting it would invite a
// consumer to build a part that spreads `rest` last, which is the failure this
// exists to prevent — so it stays unreachable from outside the package.

import type { Node } from "@uniflowed/react";

/**
 * Props on their way onto an element: what a caller hands a part, and what
 * `Field.Control` hands back for a caller to spread.
 *
 * `key` is named out of the indexer rather than left to it, and that one
 * property is the whole subtlety of this type. React takes `key` off the
 * attributes before a component is called, so a part's props never contain
 * one — but an indexer does not know that, and `{ readonly [string]: mixed }`
 * answers `mixed` for every name, `key` included. React's `key` is
 * `string | number`, so every intrinsic this package rendered was rejected for
 * a property that cannot be there:
 *
 *     error[incompatible-type]: Cannot create button element because in
 *     property key: Either unknown is incompatible with string. Or unknown is
 *     incompatible with number.
 *
 * thirty-two times, one per element, which was 32 of `@uniflowed/ui`'s 73 type
 * errors. `key?: empty` states what React already guarantees, and the errors
 * are the checker agreeing.
 *
 * # Two answers that look better than they are
 *
 * **`readonly key?: string | number`** — React's own type for the property —
 * also silences the error, and is a lie in the shape of a fix. It says a
 * caller may pass a `key` here; a part would then spread it onto its element,
 * which is the "spreading a key into JSX" mistake React 19 added a warning
 * for. `empty` is the same repair and a true sentence. It reads oddly for
 * about a second and then reads as exactly what it is: there is no value you
 * can pass under this name.
 *
 * **`React.PropsOf<"button">`** — the props of the element actually being
 * rendered, which is what this type would like to say — cannot be written
 * here. uf does not merge Flow's `jsx.js` environment, deliberately and for
 * reasons `crates/uf_check/src/upstream/environments.rs` gives, so
 * `$JSXIntrinsics` is the bare-bones table in `lib/react.js`, every
 * intrinsic's `props` is `any`, and `React.PropsOf` itself reads as an
 * any-typed value. Nothing about an element is checked here except its `key`:
 * `<button className={5} nonsenseAttr={{}} />` is not an error today. A named
 * type per element would therefore not be React's contract but a hand-written
 * copy of `jsx.js` living in a UI package, drifting from the DOM on its own
 * schedule — and it would still need an indexer for `data-*` and `aria-*`,
 * which is where this started. So it stays one `Rest`, and the day
 * `$JSXIntrinsics` is real is the day this becomes `React.PropsOf` and the
 * parts say which element they render.
 */
export type Rest = { readonly key?: empty, readonly [string]: mixed };

/**
 * The escape hatch: a caller's element in place of the part's own.
 *
 * `@uniflowed/ui` has no copy step — `packages/ui/index.js`'s header argues
 * that at length — and the thing a copy step is *for* is changing the markup. A
 * part that always renders a `<button>` cannot be the link a menu of links
 * needs; a heading fixed at `<h2>` is wrong inside an accordion. This is what
 * replaces owning the source: the part still computes every attribute, every
 * composed handler and every id, and hands them to the caller to put on
 * whatever element they wanted.
 *
 * One name and one signature everywhere, which is the point. `asChild` clones a
 * child and hopes its props survive; this hands the props over explicitly, so a
 * caller can see what they are getting, decide the order themselves, and drop
 * one deliberately. The part is still the part — `Menu.Body`'s `renders*` still
 * rejects a `<div>` where a `Menu.Item` belongs, because the escape hatch
 * changes the element the item renders and not what the item *is*. That
 * constraint is exactly what a copied source loses.
 *
 * # What is in the props, and what is not
 *
 * Everything the part would have put on its own element, in the order
 * `withProps` fixes: the caller's `rest` underneath, the part's own semantics on
 * top, handlers and refs composed rather than replaced. `children` is in there
 * too, so `render={(props) => <a href={to} {...props} />}` renders what was
 * written between the tags — a part whose children were silently dropped
 * because the caller spread the props and forgot them is the kind of quiet
 * wrongness this package exists to not have. JSX children win over a spread, so
 * `<a {...props}>Other</a>` still says what it says.
 *
 * What is *not* in there is anything true of the element rather than of the
 * part: `type="button"` is the only one in practice, and it stays on the
 * `<button>` branch. Handing it to a caller rendering an `<a>` would put an
 * attribute the HTML has no meaning for on their link.
 */
export type RenderProp = (props: Rest) => Node;

/**
 * What a part's own handler reads of the event it is handed.
 *
 * Inexact, and named rather than inferred, for the same reason `menu.js`'s
 * `MenuSelect` is: what arrives is React's synthetic event, uf does not merge
 * Flow's `jsx.js` environment so `lib/react.js` models no such thing, and these
 * are the members the handlers in this package actually read.
 *
 * It exists because of `RenderProp`. A handler written inside a JSX attribute
 * gets its parameter's type from the attribute, which for an intrinsic is
 * `any`; the escape hatch has to build the props *before* there is an element
 * to put them on, so the same handler in an object literal has an indexer's
 * `mixed` for context and Flow asks for an annotation. This is that annotation,
 * written once rather than at every handler in the package.
 *
 * One shape for keys and for presses, which is the one thing it is not honest
 * about: `key` and the modifiers belong to a keyboard event and a click has no
 * `key`. It is a parameter annotation for handlers this package writes rather
 * than a description of an event, nothing widens `mixed` into it, and the day
 * `$JSXIntrinsics` is real — the day `Rest` becomes `React.PropsOf`, which its
 * own comment is waiting for — is the day this is React's event types instead.
 */
export type PartEvent = {
  readonly defaultPrevented: boolean,
  readonly key: string,
  readonly altKey: boolean,
  readonly ctrlKey: boolean,
  readonly metaKey: boolean,
  readonly shiftKey: boolean,
  readonly currentTarget: mixed,
  readonly preventDefault: () => mixed,
  readonly stopPropagation: () => mixed,
  ...
};

/**
 * A caller's props on their way to another *part of this package*, rather than
 * onto an intrinsic element.
 *
 * `Rest` names `key` out of its indexer and types it `empty`, which is a true
 * sentence and is what stopped thirty-two intrinsics being rejected for a
 * property that cannot be there. It has a second consequence, and it only shows
 * up the first time one part of this package renders another —
 * `ToggleGroup.Root` rendering a `RadioGroup.Root`, which is how `single` mode
 * avoids being a second copy of the radio group. Creating
 * `<RadioGroup.Root {...rest} />` has Flow check the props object against that
 * component's own `...rest: Rest`, `key` included, and the indexer answers
 * `mixed` for it rather than the named `empty`:
 *
 *     error[incompatible-type]: Cannot create RadioGroupRoot element because in
 *     property key: unknown is incompatible with empty.
 *
 * So a part is spreadable onto a `<div>` and not onto a sibling part. That is a
 * hole in the type rather than a fact about the props, and this is the one
 * place it is papered over — a named function rather than an `as $FlowFixMe` at
 * the call site, so there is somewhere to say what is and is not lost.
 *
 * What is lost is nothing that was ever checked. Every element this package
 * renders has `any`-typed props today, for the reason `Rest` gives above: uf
 * does not merge Flow's `jsx.js` environment, so `$JSXIntrinsics` is the
 * bare-bones table in `lib/react.js` and `key` is the only property of an
 * element anything verifies. On the day that changes and `Rest` becomes
 * `React.PropsOf`, this function is what gets deleted.
 */
export function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}

/**
 * Call the caller's handler and then the component's.
 *
 * The caller's runs first so it can inspect the event before the component acts
 * on it, and the component's runs unless the caller stopped the event —
 * `defaultPrevented` is the caller's way of saying "I handled this", which is
 * the same contract the DOM uses.
 */
export function composeHandlers<TEvent extends { readonly defaultPrevented?: boolean, ... }>(
  theirs: mixed,
  ours: (event: TEvent) => mixed,
): (event: TEvent) => mixed {
  if (typeof theirs !== "function") {
    return ours;
  }
  return (event: TEvent) => {
    (theirs as $FlowFixMe)(event);
    if (event.defaultPrevented !== true) {
      ours(event);
    }
  };
}

/** Set both refs, whichever kinds they are. */
export function composeRefs<T>(
  theirs: mixed,
  ours: (value: T | null) => mixed,
): (value: T | null) => void {
  return (value: T | null) => {
    ours(value);
    if (typeof theirs === "function") {
      (theirs as $FlowFixMe)(value);
    } else if (theirs != null && typeof theirs === "object") {
      (theirs as $FlowFixMe).current = value;
    }
  };
}

/**
 * Two sets of props, the component's on top.
 *
 * The same rule as everywhere else in this package, applied where the element
 * is the *caller's* rather than the component's: `Tooltip.Trigger` and
 * `HoverCard.Trigger` hand their attributes to a render function so a caller
 * can put them on a link or a menu item of their own, and the attributes that
 * make the trigger work — the `aria-describedby` naming the content, the ref
 * the content is measured against — have to survive whatever the caller passed
 * alongside them.
 *
 * A spread would say this in one line and cannot be written: Flow declines to
 * compute a type for `{ ...base, name: value }` when `base` has an indexer,
 * because the indexer may overwrite the named key in a way it cannot track.
 * The loop is that spread, with `key` dropped for the reason `withoutComposed`
 * gives.
 */
export function withProps(base: Rest, ours: Rest): Rest {
  const merged: { key?: empty, [string]: mixed } = {};
  for (const name of Object.keys(base)) {
    if (name !== "key") {
      merged[name] = base[name];
    }
  }
  for (const name of Object.keys(ours)) {
    if (name !== "key") {
      merged[name] = ours[name];
    }
  }
  return merged;
}

/**
 * A caller's props with the handlers and ref removed.
 *
 * They are pulled out because they have to be composed rather than spread, and
 * leaving them in would put the caller's copy back on top of the composed one.
 */
export function withoutComposed(rest: Rest, names: $ReadOnlyArray<string>): Rest {
  const kept: { key?: empty, [string]: mixed } = {};
  for (const name of Object.keys(rest)) {
    // `key` is dropped whatever the caller asked to compose, because it is the
    // one name the indexer does not speak for: writing `rest[name]` under it
    // would put a `mixed` back where `Rest` promises nothing can be, and Flow
    // says so. Nothing is lost — React removed the `key` long before this ran,
    // so this is the type-level statement made at runtime rather than a filter
    // that ever has work to do.
    if (name !== "key" && !names.includes(name)) {
      kept[name] = rest[name];
    }
  }
  return kept;
}
