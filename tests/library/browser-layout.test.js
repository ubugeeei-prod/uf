// @flow
//
// What a real browser answers that the DOM shim cannot, and what the shim
// answers *wrongly*.
//
// This file is the reason `uf test --browser` exists, written as a test rather
// than as a paragraph in the documentation. It runs in both modes and asserts
// a different answer in each, so neither host can change its mind without this
// going red:
//
//     uf run test:lib            → on Node, over happy-dom
//     uf test --browser tests/library/browser-layout.test.js
//
// # The three differences, in order of how much they cost
//
// **`getComputedStyle` answers, and the answer is wrong.** A component with
// `width: 50%` is `"50%"` wide according to happy-dom and `"512px"` wide
// according to a browser. happy-dom resolves the cascade — which rule wins,
// what a class contributes — and stops there, because resolving a percentage
// means knowing the containing block's width, which means layout. So the value
// it hands back is the *declared* one wearing the name of the computed one.
// That is worse than a refusal: a test that asserts a component is half its
// container's width passes against a string that says nothing about width.
//
// **`getBoundingClientRect` does not answer.** Every rectangle is zero, always,
// for every element. Anything that reads a measured size — a tooltip that
// flips when it would overflow, a list that virtualises, a truncation that
// depends on the text fitting — is not merely untested under the shim, it is
// untestable: the code takes the "it has no size" branch every time, and the
// branch a user meets is never executed.
//
// **`IntersectionObserver` exists and never fires.** The constructor is there,
// `observe` accepts an element, and no callback is ever made, because
// intersection is a question about layout too. A lazy-loading component under
// the shim never loads; a test asserting that it eventually does would hang,
// and one asserting it has not loaded yet passes for the wrong reason forever.
//
// # How this file knows which host it is on
//
// `typeof process === "undefined"`. A page has no `process`; Node, Bun and Deno
// all do. It is a property of the *host* rather than a flag uf sets, which
// matters here more than anywhere else in the suite: a test that asks uf which
// mode it is in would agree with uf about the answer even when uf is wrong,
// and this file exists to disagree.
//
// The assertions are deliberately two-sided rather than skipped on Node. A
// `it.skip` under the shim would leave the shim's behaviour unpinned, and the
// shim's behaviour is half of what is being documented: if happy-dom ever
// grows a layout engine, this file should go red and be rewritten, not quietly
// start testing nothing.

import { cleanup, render, screen } from "@uniflowed/react-testing";
import { afterEach, describe, expect, it } from "@uniflowed/test";

/**
 * Whether the tests are running in a page.
 *
 * See the header: a host's own missing global, not something uf sets.
 */
const IN_A_PAGE: boolean = typeof process === "undefined";

/** A box whose width only means something once something has laid it out. */
component Half() {
  return (
    <div style={{ width: "600px" }}>
      <div data-testid="half" style={{ width: "50%", height: "40px" }} />
    </div>
  );
}

afterEach(() => {
  cleanup();
});

describe("a measured component", () => {
  it("resolves a percentage width to pixels only in a browser", () => {
    render(<Half />);
    const half = screen.getByTestId("half");
    const computed = globalThis.getComputedStyle(half).width;

    if (IN_A_PAGE) {
      // Half of six hundred, and the browser says so in pixels — the thing a
      // test about a half-width component is actually asking.
      expect(computed).toBe("300px");
    } else {
      // happy-dom hands back the declaration. Pinned as a fact about the shim,
      // because a reader who writes `expect(computed).toBe("300px")` and sees
      // it fail deserves to find this line rather than a bug report.
      expect(computed).toBe("50%");
    }
  });

  it("has a rectangle only in a browser", () => {
    render(<Half />);
    const rect = screen.getByTestId("half").getBoundingClientRect();

    if (IN_A_PAGE) {
      expect(rect.width).toBe(300);
      expect(rect.height).toBe(40);
    } else {
      // Not "approximately zero" — every rectangle from happy-dom is exactly
      // zero, for every element, whatever it was styled with.
      expect(rect.width).toBe(0);
      expect(rect.height).toBe(0);
    }
  });

  it("agrees with itself about how wide the box is", () => {
    // The claim that survives both hosts, and the one a component actually
    // depends on: whatever this host thinks the box is, the two ways of asking
    // give the same answer. Under the shim both are vacuous — `0` and `"50%"`
    // are compared as "neither is a pixel measurement" — and in a browser they
    // are the same three hundred pixels reached two different ways.
    render(<Half />);
    const half = screen.getByTestId("half");
    const rect = half.getBoundingClientRect();
    const computed = globalThis.getComputedStyle(half).width;

    expect(computed.endsWith("px")).toBe(IN_A_PAGE);
    expect(rect.width > 0).toBe(IN_A_PAGE);
    if (IN_A_PAGE) {
      expect(Math.round(rect.width)).toBe(Math.round(Number.parseFloat(computed)));
    }
  });
});

describe("a component that waits to be seen", () => {
  it("is told it is on screen only in a browser", async () => {
    render(<Half />);
    const half = screen.getByTestId("half");

    const seen = await new Promise<boolean>((resolve) => {
      // Bounded, because the whole point is that one of these two hosts never
      // calls back. An unbounded wait here would be a file that hangs until
      // `uf test`'s deadline kills it, and a deadline is not an assertion.
      const giveUp = setTimeout(() => resolve(false), 250);
      const observer: IntersectionObserver = new globalThis.IntersectionObserver(
        (entries: $ReadOnlyArray<mixed>) => {
          clearTimeout(giveUp);
          observer.disconnect();
          const first = entries[0];
          resolve(first != null && typeof first === "object" && first.isIntersecting === true);
        },
      );
      observer.observe(half);
    });

    // The constructor exists on both hosts, which is exactly the trap: a test
    // that checks `typeof IntersectionObserver === "function"` is green under
    // the shim and proves nothing.
    expect(typeof globalThis.IntersectionObserver).toBe("function");
    expect(seen).toBe(IN_A_PAGE);
  });
});
