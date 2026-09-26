"use client";
// @flow
//
// Carousel: slides shown one at a time, with buttons to move between them and
// one to stop them when they turn on their own.
//
// `uf ui add carousel` wrote this file into the project, and it is the
// project's from then on. `uf ui diff carousel` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The frame, the slide that shows, and the buttons. `@uniflowed/ui`'s `Carousel`
// owns the pattern: a `role="group"` described as a carousel and named by
// `label`, each slide a group described as a slide and named such as "2 of 5",
// slides out of sight also out of reach (`inert`), previous and next buttons
// that control the slides, a polite announcement of the slide that comes in,
// kept silent while the carousel turns on its own, and a pause button that
// says whether it is stopped. Which slide shows is drawn from `data-state`, and
// the pause button's icon from `aria-pressed`.
//
// # What to keep true when you change it
//
// * **A carousel that turns on its own has a pause button.** `Carousel.Pause`
//   is how a reader stops movement they did not ask for.
// * **Name it.** `label` says what the slides are, such as "Featured guides".
// * **Keep the buttons in sight.** Previous and next are how a keyboard user
//   moves between slides; do not show them only on hover.
// * **Text stays on measured pairs.** `ink` on `surface`, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { Carousel } from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    display: "grid",
    gap: ufTokens.space2,
    fontFamily: ufTokens.fontSans,
    color: ufTokens.ink,
  },
  content: {
    overflow: "hidden",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
    backgroundColor: ufTokens.surface,
  },
  item: {
    display: { default: "block", ":is([data-state=inactive])": "none" },
  },
  controls: {
    display: "flex",
    alignItems: "center",
    justifyContent: "flex-end",
    gap: ufTokens.space2,
  },
  button: {
    // Read by the pause button's two icons, which cannot see its state.
    "--uf-carousel-playing": { default: "1", ":is([aria-pressed=true])": "0" },
    "--uf-carousel-stopped": { default: "0", ":is([aria-pressed=true])": "1" },
    position: "relative",
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: "36px",
    height: "36px",
    margin: 0,
    padding: 0,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
    cursor: { default: "pointer", ":disabled": "not-allowed" },
    opacity: { default: 1, ":disabled": 0.55 },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  whilePlaying: {
    position: "absolute",
    opacity: "var(--uf-carousel-playing, 1)",
  },
  whileStopped: {
    position: "absolute",
    opacity: "var(--uf-carousel-stopped, 0)",
  },
});

/**
 * The carousel. `count` is how many slides there are and `label` what they are;
 * `autoplay` is the milliseconds between turns, or `null` to wait for a reader.
 */
component CarouselRoot(
  children: React.Node,
  count: number,
  label: string,
  autoplay?: number | null = null,
  loop?: boolean = true,
  index?: number,
  defaultIndex?: number = 0,
  onIndexChange?: (index: number) => void,
  orientation?: "horizontal" | "vertical" = "horizontal",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Carousel.Root
      {...forwarded(rest)}
      autoplay={autoplay}
      className={classNames(props(styles.root, xstyle).className, className)}
      count={count}
      defaultIndex={defaultIndex}
      index={index}
      label={label}
      loop={loop}
      onIndexChange={onIndexChange}
      orientation={orientation}
    >
      {children}
    </Carousel.Root>
  );
}

/** The frame the slides show in. */
component CarouselContent(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Carousel.Content
      {...forwarded(rest)}
      className={classNames(props(styles.content, xstyle).className, className)}
    >
      {children}
    </Carousel.Content>
  );
}

/** One slide, shown while it is the current one. */
component CarouselItem(
  index: number,
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Carousel.Item
      {...forwarded(rest)}
      className={classNames(props(styles.item, xstyle).className, className)}
      index={index}
    >
      {children}
    </Carousel.Item>
  );
}

/** A row for the buttons, at the frame's inline end. */
component CarouselControls(children: React.Node, xstyle?: StyleArgument, className?: string) {
  return (
    <div className={classNames(props(styles.controls, xstyle).className, className)}>
      {children}
    </div>
  );
}

/** The button to the slide before. */
component CarouselPrevious(
  label?: string = "Previous slide",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Carousel.Previous
      {...forwarded(rest)}
      className={classNames(props(styles.button, xstyle).className, className)}
      label={label}
    >
      <Chevron path="m15 18-6-6 6-6" />
    </Carousel.Previous>
  );
}

/** The button to the slide after. */
component CarouselNext(
  label?: string = "Next slide",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Carousel.Next
      {...forwarded(rest)}
      className={classNames(props(styles.button, xstyle).className, className)}
      label={label}
    >
      <Chevron path="m9 18 6-6-6-6" />
    </Carousel.Next>
  );
}

/** The button that stops the slides turning on their own, and starts them again. */
component CarouselPause(
  pauseLabel?: string = "Stop the carousel",
  playLabel?: string = "Start the carousel",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Carousel.Pause
      {...forwarded(rest)}
      className={classNames(props(styles.button, xstyle).className, className)}
      pauseLabel={pauseLabel}
      playLabel={playLabel}
    >
      <svg
        {...props(styles.whilePlaying)}
        aria-hidden="true"
        fill="currentColor"
        focusable="false"
        height="14"
        viewBox="0 0 24 24"
        width="14"
      >
        <path d="M6 4h4v16H6zM14 4h4v16h-4z" />
      </svg>
      <svg
        {...props(styles.whileStopped)}
        aria-hidden="true"
        fill="currentColor"
        focusable="false"
        height="14"
        viewBox="0 0 24 24"
        width="14"
      >
        <path d="M7 4v16l13-8z" />
      </svg>
    </Carousel.Pause>
  );
}

/** An arrow for a step button. */
component Chevron(path: string) {
  return (
    <svg
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
      <path d={path} />
    </svg>
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

/**
 * The parts, under the names `import * as Carousel from "./carousel.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<Carousel.Root>`
 * and `<Carousel.Content>`, the way it writes `@uniflowed/ui`'s own parts. Each
 * is declared under its full name, so React DevTools and an error say
 * `CarouselRoot` rather than `Root`.
 */
export {
  CarouselRoot as Root,
  CarouselContent as Content,
  CarouselItem as Item,
  CarouselControls as Controls,
  CarouselPrevious as Previous,
  CarouselNext as Next,
  CarouselPause as Pause,
};
