// @flow
//
// Internal to `@uniflowed/web`: the elements that exist for a screen reader.
//
// A single-page application breaks two things a browser does for free. It
// navigates without a page load, so nothing announces that the page changed;
// and it updates regions in place, so a reader who is not looking at that part
// of the screen is never told.
//
// Both are one element with the right ARIA attributes, and both are easy to
// get subtly wrong — which is why they are components rather than advice.

import * as React from "@uniflowed/react";

/**
 * The id `Page` renders and `SkipLink` points at.
 *
 * One constant rather than two string literals, because the two have to agree
 * and a skip link that points at nothing fails silently — the focus simply
 * stays where it was, and only someone navigating by keyboard ever finds out.
 */
const MAIN_ID: string = "uf-main";

/**
 * How a thing is hidden from eyes but not from a screen reader.
 *
 * `display: none` and `visibility: hidden` both take an element out of the
 * accessibility tree as well, which is the usual way this is got wrong. Inline
 * rather than a class, because a component in a library cannot assume the
 * application has loaded any particular stylesheet.
 */
const VISUALLY_HIDDEN = {
  position: "absolute",
  width: "1px",
  height: "1px",
  margin: "-1px",
  padding: 0,
  overflow: "hidden",
  // Both spellings: `clip` is deprecated and still the one older screen
  // readers honour.
  clip: "rect(0 0 0 0)",
  clipPath: "inset(50%)",
  whiteSpace: "nowrap",
  border: 0,
};

/**
 * A live region: text put here is read out when it changes.
 *
 * `polite` waits for the reader to finish what they are saying, which is right
 * for everything except an error the reader must hear now. `assertive`
 * interrupts, and interrupting someone mid-sentence to say "saved" is why it is
 * not the default.
 *
 * The element is always rendered, empty or not. A live region that appears at
 * the same moment as its text is not announced — the screen reader has to be
 * watching it *before* it changes — which is the mistake that makes an
 * announcer look like it does nothing.
 */
export component Announcer(
  children?: React.Node,
  urgency?: "polite" | "assertive" = "polite",
  atomic?: boolean = true,
) renders React.Node {
  return (
    <div aria-live={urgency} aria-atomic={atomic} style={VISUALLY_HIDDEN}>
      {children}
    </div>
  );
}

/**
 * The page's main content, as a landmark.
 *
 * `<main>` is what "skip to content" skips to and what a screen reader's
 * landmark list offers, and there must be exactly one per document — which is
 * why this is not something a layout should also render.
 */
export component Page(children?: React.Node, className?: string) renders React.Node {
  return (
    <main id={MAIN_ID} className={className}>
      {children}
    </main>
  );
}

/**
 * "Skip to content": the first thing a keyboard reaches on the page.
 *
 * Without it, tabbing into a page means tabbing through the whole navigation
 * before reaching a word of the content — on every route, every time. It is
 * the cheapest accessibility fix there is and the one most often missing,
 * because it is invisible when it works and so nobody notices it is absent.
 *
 * Render it first inside the layout, before the navigation. It is hidden until
 * it takes focus and then shows itself, which is why it does not cost a
 * design anything: the only person who ever sees it is the person using it.
 */
export component SkipLink(children?: React.Node = "Skip to content") renders React.Node {
  const [focused, setFocused] = React.useState(false);

  return (
    <a
      href={`#${MAIN_ID}`}
      onFocus={() => setFocused(true)}
      onBlur={() => setFocused(false)}
      style={
        focused
          ? {
              position: "absolute",
              top: "0.5rem",
              left: "0.5rem",
              // Above anything a page is likely to have stacked, since the
              // link has to be readable wherever it lands.
              zIndex: 1000,
              padding: "0.5rem 0.75rem",
              background: "Canvas",
              color: "CanvasText",
              // `CanvasText` follows the reader's own light or dark setting,
              // so the link stays legible without knowing the page's theme.
              border: "1px solid CanvasText",
              borderRadius: "0.25rem",
            }
          : VISUALLY_HIDDEN
      }
    >
      {children}
    </a>
  );
}

/**
 * The frame around a page: whatever is the same between routes.
 *
 * A plain grouping element rather than a landmark, because the landmarks
 * inside it — `<nav>`, `<main>`, `<footer>` — are the ones a reader navigates
 * by, and wrapping them in another one adds a level to walk through.
 */
export component Layout(children?: React.Node, className?: string) renders React.Node {
  return <div className={className}>{children}</div>;
}
