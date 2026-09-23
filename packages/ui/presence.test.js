// @flow
//
// `internal/presence.js`: a closing part stays on the page, marked
// `data-state="closed"`, until the animations on it have finished, and no
// longer.
//
// The test DOM does not run CSS, so there are no real transitions here. A test
// that needs one gives the element a `getAnimations` that returns
// `FakeAnimation`s, and the test decides when they finish or are cancelled.
// A test that does not stub it gets the test DOM as it is, with no
// `getAnimations` at all. That is the case of a browser with nothing to wait
// for, and it has to behave exactly as the parts did before the hook existed.

import * as React from "@uniflowed/react";
import { StrictMode, useRef } from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { act, cleanup, render, screen } from "@uniflowed/react-testing";

import { usePresence } from "./internal/presence.js";

afterEach(() => {
  cleanup();
});

/** An animation whose end the test controls. */
class FakeAnimation {
  readonly finished: Promise<mixed>;
  finish: () => void;
  cancel: () => void;

  constructor() {
    let finish = () => {};
    let cancel = () => {};
    this.finished = new Promise((resolve, reject) => {
      finish = () => resolve(this);
      cancel = () => reject(new Error("AbortError"));
    });
    this.finish = finish;
    this.cancel = cancel;
  }
}

/** A part that shows while `open`, the way an overlay part would use the hook. */
component Part(open: boolean) {
  const ref = useRef<HTMLElement | null>(null);
  const { present, state } = usePresence(open, ref);
  if (!present) {
    return null;
  }
  return (
    <div
      data-state={state}
      data-testid="part"
      inert={state === "closed" ? true : undefined}
      ref={ref}
    >
      <button type="button">Inside</button>
    </div>
  );
}

/** The part if it is on the page, or null. */
function part(): Element | null {
  return screen.queryByTestId("part");
}

/** Make the part report `animations` from `getAnimations`, as a browser mid-transition would. */
function animate(animations: $ReadOnlyArray<FakeAnimation>): void {
  const element: $FlowFixMe = part();
  if (element == null) {
    throw new Error("the part is not on the page");
  }
  element.getAnimations = () => animations;
}

/** Let the promises the hook is waiting on settle, and React commit the result. */
async function settle(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

describe("usePresence", () => {
  it("renders nothing for a part that starts closed", () => {
    render(<Part open={false} />);
    expect(part()).toBe(null);
  });

  it("marks an open part open", () => {
    render(<Part open />);
    expect(part()?.getAttribute("data-state")).toBe("open");
    expect(part()?.hasAttribute("inert")).toBe(false);
  });

  it("removes a closing part at once when nothing is animating it", async () => {
    // No `getAnimations`: a test DOM, a browser with no stylesheet, or `0s`
    // under reduced motion. The part goes on the next microtask, as it did
    // before this hook.
    const { rerender } = render(<Part open />);
    rerender(<Part open={false} />);
    await settle();
    expect(part()).toBe(null);
  });

  it("removes a closing part at once when its animation list is empty", async () => {
    const { rerender } = render(<Part open />);
    animate([]);
    rerender(<Part open={false} />);
    await settle();
    expect(part()).toBe(null);
  });

  it("keeps a closing part, closed and inert, until its exit animation finishes", async () => {
    const exit = new FakeAnimation();
    const { rerender } = render(<Part open />);
    animate([exit]);
    rerender(<Part open={false} />);
    // The render that closed it already says closed. It was never removed and
    // put back, and it cannot be reached while it fades.
    expect(part()?.getAttribute("data-state")).toBe("closed");
    expect(part()?.hasAttribute("inert")).toBe(true);
    await settle();
    expect(part()?.getAttribute("data-state")).toBe("closed");

    exit.finish();
    await settle();
    expect(part()).toBe(null);
  });

  it("waits for every animation, not the first to finish", async () => {
    const fade = new FakeAnimation();
    const slide = new FakeAnimation();
    const { rerender } = render(<Part open />);
    animate([fade, slide]);
    rerender(<Part open={false} />);
    fade.finish();
    await settle();
    expect(part()).not.toBe(null);
    slide.finish();
    await settle();
    expect(part()).toBe(null);
  });

  it("treats a cancelled animation as finished", async () => {
    const exit = new FakeAnimation();
    const { rerender } = render(<Part open />);
    animate([exit]);
    rerender(<Part open={false} />);
    exit.cancel();
    await settle();
    expect(part()).toBe(null);
  });

  it("still waits for the others when one animation is cancelled", async () => {
    const cancelled = new FakeAnimation();
    const running = new FakeAnimation();
    const { rerender } = render(<Part open />);
    animate([cancelled, running]);
    rerender(<Part open={false} />);
    cancelled.cancel();
    await settle();
    expect(part()?.getAttribute("data-state")).toBe("closed");
    running.finish();
    await settle();
    expect(part()).toBe(null);
  });

  it("keeps the same element when it reopens part-way through its exit", async () => {
    const exit = new FakeAnimation();
    const { rerender } = render(<Part open />);
    const element = part();
    animate([exit]);
    rerender(<Part open={false} />);
    rerender(<Part open />);
    expect(part()).toBe(element);
    expect(part()?.getAttribute("data-state")).toBe("open");
    expect(part()?.hasAttribute("inert")).toBe(false);

    // The interrupted exit settling later must not take the reopened part.
    exit.finish();
    await settle();
    expect(part()).toBe(element);
    expect(part()?.getAttribute("data-state")).toBe("open");
  });

  it("closes again after an interrupted exit, and waits for the new one", async () => {
    const first = new FakeAnimation();
    const second = new FakeAnimation();
    const { rerender } = render(<Part open />);
    animate([first]);
    rerender(<Part open={false} />);
    await settle();
    rerender(<Part open />);
    animate([second]);
    rerender(<Part open={false} />);
    first.finish();
    await settle();
    expect(part()?.getAttribute("data-state")).toBe("closed");
    second.finish();
    await settle();
    expect(part()).toBe(null);
  });

  it("removes the part once under StrictMode's double effects", async () => {
    const exit = new FakeAnimation();
    const { rerender } = render(
      <StrictMode>
        <Part open />
      </StrictMode>,
    );
    animate([exit]);
    rerender(
      <StrictMode>
        <Part open={false} />
      </StrictMode>,
    );
    await settle();
    expect(part()?.getAttribute("data-state")).toBe("closed");
    exit.finish();
    await settle();
    expect(part()).toBe(null);
  });

  it("renders an open part as open on the server, with nothing to wait for", async () => {
    const { renderToString } = await import("react-dom/server");
    expect(String(renderToString(<Part open />))).toContain('data-state="open"');
    expect(String(renderToString(<Part open={false} />))).toBe("");
  });
});
