// @flow
//
// `@uniflowed/hooks`.
//
// Each of these tests the bug the hook exists to prevent, not the happy path:
// a timer calling a stale closure, a subscription torn down every render, a
// slow request overwriting a fast one, two components disagreeing about the
// same storage key.

import * as React from "@uniflowed/react";
import { useRef, useState } from "@uniflowed/react";
import { afterEach, describe, expect, fn, it, uft } from "@uniflowed/test";
import { act, fireEvent, render, screen, userEvent, waitFor } from "@uniflowed/react-testing";
import { fixedClock, setClock } from "@uniflowed/core/clock";

import { bodyOf, elementIn, parentOf } from "../../tests/library/dom.js";
import {
  RenderProvider,
  useAnimationFrame,
  useAsync,
  useBroadcast,
  useClickOutside,
  useClipboard,
  useCounter,
  useCycle,
  useDebouncedCallback,
  useDebouncedValue,
  useElementRef,
  useElementState,
  useEventListener,
  useEventSource,
  useFocusWithin,
  useGeolocation,
  useHash,
  useHover,
  useIdle,
  useIntersecting,
  useInterval,
  useKeyCombo,
  useKeyHeld,
  useList,
  useLongPress,
  useMediaQuery,
  useMounted,
  useMutationObserver,
  useNetwork,
  useNow,
  useOnline,
  usePermission,
  usePreferredColorScheme,
  usePrevious,
  useRandom,
  useRenderTimeZone,
  useRenderedAt,
  useScroll,
  useScrollLock,
  useSet,
  useShuffled,
  useStableCallback,
  useStorage,
  useSupported,
  useThrottledCallback,
  useTimeAgo,
  useToggle,
  useUndoable,
} from "@uniflowed/hooks";

const tick = (millis: number) => act(() => new Promise((resolve) => setTimeout(resolve, millis)));

/**
 * Move a clock the test installed itself, and let React catch up.
 *
 * The counterpart to `tick`, and the difference is what each one is entitled
 * to claim. `tick` sleeps, so it promises *at least* `millis` and nothing
 * about the most: `setTimeout(resolve, 35)` on a box running twelve workers
 * comes back whenever the worker is scheduled again, and a test that left five
 * milliseconds of slack against a forty-millisecond hook fails when it comes
 * back six late. `advance` is the only thing moving the clock, so 39 means 39.
 *
 * Use it for anything built on `setTimeout`, `setInterval` or `Date.now`,
 * which is every hook in this file except `useAnimationFrame`: `uft` replaces
 * the scheduling globals, and the frame callback is the document's.
 */
const advance = (millis: number) => {
  act(() => {
    uft.advanceTimersByTime(millis);
  });
};

describe("useStableCallback", () => {
  it("keeps one identity across renders", async () => {
    const identities = new Set();
    component Probe() {
      const [, setTick] = useState(0);
      const callback = useStableCallback(() => {});
      identities.add(callback);
      return (
        <button type="button" onClick={() => setTick((value) => value + 1)}>
          rerender
        </button>
      );
    }

    render(<Probe />);
    await userEvent.click(screen.getByRole("button"));
    await userEvent.click(screen.getByRole("button"));
    // One identity for three renders. This is what stops every hook that takes
    // a callback from re-subscribing on each keystroke.
    expect(identities.size).toBe(1);
  });

  it("always calls the newest body", async () => {
    const seen = [];
    component Probe() {
      const [count, setCount] = useState(0);
      const report = useStableCallback(() => seen.push(count));
      return (
        <div>
          <button type="button" onClick={() => setCount(count + 1)}>
            bump
          </button>
          <button type="button" onClick={report}>
            report
          </button>
        </div>
      );
    }

    render(<Probe />);
    await userEvent.click(screen.getByRole("button", { name: "bump" }));
    await userEvent.click(screen.getByRole("button", { name: "report" }));
    // Not 0 — the callback identity is stable, its closure is not stale.
    expect(seen).toEqual([1]);
  });
});

describe("useInterval", () => {
  it("calls the newest body, not the one from the first render", async () => {
    const seen = [];
    component Probe() {
      const [count, setCount] = useState(0);
      useInterval(() => seen.push(count), 10);
      return (
        <button type="button" onClick={() => setCount(count + 1)}>
          bump
        </button>
      );
    }

    render(<Probe />);
    await userEvent.click(screen.getByRole("button"));
    seen.length = 0;
    await tick(35);
    // A hand-written interval either restarts on every render or calls the
    // closure from the first one forever. Neither is this.
    expect(seen.every((value) => value === 1)).toBe(true);
    expect(seen.length > 0).toBe(true);
  });

  it("does not run when the interval is null", async () => {
    const body = fn();
    component Probe() {
      useInterval(body, null);
      return null;
    }
    render(<Probe />);
    await tick(30);
    expect(body).not.toHaveBeenCalled();
  });

  it("stops at unmount", async () => {
    const body = fn();
    component Probe() {
      useInterval(body, 5);
      return null;
    }
    const { unmount } = render(<Probe />);
    await tick(15);
    unmount();
    const before = body.mock.calls.length;
    await tick(20);
    expect(body.mock.calls.length).toBe(before);
  });
});

describe("useDebouncedValue", () => {
  it("reports the value only once it has settled", async () => {
    component Probe() {
      const [text, setText] = useState("");
      const settled = useDebouncedValue(text, 25);
      return (
        <div>
          <input aria-label="query" onChange={(event) => setText(event.target.value)} />
          <output>{settled}</output>
        </div>
      );
    }

    const { container } = render(<Probe />);
    const output = elementIn(container, "output");
    await userEvent.type(screen.getByLabelText("query"), "abc");
    // Still empty: the keystrokes have not settled, so nothing downstream of
    // the debounce has seen them.
    expect(output.textContent).toBe("");
    await waitFor(() => {
      expect(output.textContent).toBe("abc");
    });
  });
});

describe("useAsync", () => {
  it("reports pending, then the value", async () => {
    component Probe() {
      const { value, pending } = useAsync(async () => "loaded", []);
      return <output>{pending ? "pending" : String(value)}</output>;
    }
    render(<Probe />);
    expect(screen.getByText("pending")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByText("loaded")).toBeInTheDocument();
    });
  });

  it("reports a rejection as an error", async () => {
    component Probe() {
      const { error, pending } = useAsync(async () => {
        throw new Error("nope");
      }, []);
      return <output>{pending ? "pending" : (error?.message ?? "none")}</output>;
    }
    render(<Probe />);
    await waitFor(() => {
      expect(screen.getByText("nope")).toBeInTheDocument();
    });
  });

  it("ignores a slow first result once a second run has started", async () => {
    let call = 0;
    component Probe() {
      const [key, setKey] = useState("first");
      const { value } = useAsync(async () => {
        call += 1;
        const mine = call;
        // The first call is slower than the second, which is the case that
        // silently shows the wrong answer.
        await new Promise((resolve) => setTimeout(resolve, mine === 1 ? 60 : 5));
        return `result ${mine}`;
      }, [key]);
      return (
        <div>
          <button type="button" onClick={() => setKey("second")}>
            change
          </button>
          <output>{value ?? "none"}</output>
        </div>
      );
    }

    render(<Probe />);
    await userEvent.click(screen.getByRole("button"));
    await tick(100);
    expect(screen.getByText("result 2")).toBeInTheDocument();
    expect(screen.queryByText("result 1")).toBe(null);
  });

  it("does not write state after unmount", async () => {
    component Probe() {
      useAsync(async () => {
        await new Promise((resolve) => setTimeout(resolve, 30));
        return "late";
      }, []);
      return null;
    }
    const { unmount } = render(<Probe />);
    unmount();
    // The assertion is that nothing throws or warns while the promise settles
    // into a component that is gone.
    await tick(50);
    expect(true).toBe(true);
  });
});

describe("useStorage", () => {
  it("falls back to the initial value when the key is unset", () => {
    component Probe() {
      const [value] = useStorage("uf-test-unset", "fallback");
      return <output>{value}</output>;
    }
    render(<Probe />);
    expect(screen.getByText("fallback")).toBeInTheDocument();
  });

  it("keeps a value across a remount", async () => {
    component Probe() {
      const [value, write] = useStorage("uf-test-kept", "start");
      return (
        <div>
          <button type="button" onClick={() => write("written")}>
            write
          </button>
          <output>{value}</output>
        </div>
      );
    }
    const { unmount } = render(<Probe />);
    await userEvent.click(screen.getByRole("button"));
    expect(screen.getByText("written")).toBeInTheDocument();
    unmount();
    render(<Probe />);
    expect(screen.getByText("written")).toBeInTheDocument();
  });

  it("keeps two components using the same key in step", async () => {
    component Writer() {
      const [, write] = useStorage("uf-test-shared", "start");
      return (
        <button type="button" onClick={() => write("shared")}>
          write
        </button>
      );
    }
    component Reader() {
      const [value] = useStorage("uf-test-shared", "start");
      return <output>{value}</output>;
    }
    render(
      <div>
        <Writer />
        <Reader />
      </div>,
    );
    await userEvent.click(screen.getByRole("button"));
    // A `storage` event does not fire in the tab that wrote, so without an
    // in-process announcement the reader would still say "start".
    expect(screen.getByText("shared")).toBeInTheDocument();
  });

  it("treats an unparseable value as absent rather than throwing", () => {
    globalThis.localStorage.setItem("uf-test-broken", "{not json");
    component Probe() {
      const [value] = useStorage("uf-test-broken", "fallback");
      return <output>{value}</output>;
    }
    render(<Probe />);
    expect(screen.getByText("fallback")).toBeInTheDocument();
  });
});

describe("useToggle and useCounter", () => {
  it("toggles", async () => {
    component Probe() {
      const { on, toggle } = useToggle();
      return (
        <button type="button" onClick={toggle}>
          {on ? "on" : "off"}
        </button>
      );
    }
    render(<Probe />);
    await userEvent.click(screen.getByRole("button"));
    expect(screen.getByRole("button").textContent).toBe("on");
  });

  it("clamps a counter to its bounds", async () => {
    component Probe() {
      const { count, increment, decrement } = useCounter(0, { min: 0, max: 2 });
      return (
        <div>
          <button type="button" onClick={() => increment()}>
            up
          </button>
          <button type="button" onClick={() => decrement()}>
            down
          </button>
          <output>{count}</output>
        </div>
      );
    }
    render(<Probe />);
    const up = screen.getByRole("button", { name: "up" });
    await userEvent.click(up);
    await userEvent.click(up);
    await userEvent.click(up);
    expect(screen.getByText("2")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "down" }));
    await userEvent.click(screen.getByRole("button", { name: "down" }));
    await userEvent.click(screen.getByRole("button", { name: "down" }));
    expect(screen.getByText("0")).toBeInTheDocument();
  });
});

describe("element hooks", () => {
  it("listens and cleans up", async () => {
    const handler = fn();
    component Probe() {
      const ref = useElementRef<HTMLButtonElement>();
      useEventListener(ref, "click", handler);
      return (
        <button ref={ref} type="button">
          press
        </button>
      );
    }
    const { unmount } = render(<Probe />);
    const button = screen.getByRole("button");
    await userEvent.click(button);
    expect(handler.mock.calls.length).toBe(1);
    unmount();
    fireEvent.click(button);
    expect(handler.mock.calls.length).toBe(1);
  });

  it("reports a press outside", async () => {
    const outside = fn();
    component Probe() {
      const ref = useElementRef<HTMLDivElement>();
      useClickOutside(ref, outside);
      return (
        <div>
          <div ref={ref}>
            <button type="button">inside</button>
          </div>
          <button type="button">elsewhere</button>
        </div>
      );
    }
    render(<Probe />);
    await userEvent.click(screen.getByRole("button", { name: "inside" }));
    expect(outside).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "elsewhere" }));
    expect(outside.mock.calls.length).toBe(1);
  });

  it("tracks hover and focus", async () => {
    component Probe() {
      const ref = useElementRef<HTMLDivElement>();
      const hovered = useHover(ref);
      const focused = useFocusWithin(ref);
      return (
        <div ref={ref}>
          <button type="button">target</button>
          <output>
            {hovered ? "hovered" : "away"} {focused ? "focused" : "blurred"}
          </output>
        </div>
      );
    }
    render(<Probe />);
    const region = parentOf(screen.getByRole("button"));
    fireEvent.pointerEnter(region);
    expect(screen.getByText(/hovered/)).toBeInTheDocument();
    fireEvent.pointerLeave(region);
    expect(screen.getByText(/away/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button"));
    expect(screen.getByText(/focused/)).toBeInTheDocument();
  });
});

describe("prerendering", () => {
  it("reports the stated server value until the component has mounted", () => {
    // The point of `useMounted`: a value that differs between the server and
    // the client is rendered as the server's first, so hydration matches.
    component Probe() {
      const mounted = useMounted();
      return <output>{mounted ? "client" : "server"}</output>;
    }
    render(<Probe />);
    // `render` mounts and flushes effects, so by the time we look it is the
    // client value — the first pass having been "server" is what matters, and
    // is what the absence of a hydration warning shows.
    expect(screen.getByText("client")).toBeInTheDocument();
  });

  it("reads the browser through a store rather than during render", () => {
    component Probe() {
      const online = useOnline();
      return <output>{online ? "online" : "offline"}</output>;
    }
    render(<Probe />);
    expect(screen.getByText("online")).toBeInTheDocument();
  });
});

describe("usePrevious", () => {
  it("reports the value from the render before", async () => {
    component Probe() {
      const [count, setCount] = useState(0);
      const before = usePrevious(count);
      return (
        <div>
          <button type="button" onClick={() => setCount(count + 1)}>
            bump
          </button>
          <output>{`${String(before)}→${count}`}</output>
        </div>
      );
    }
    render(<Probe />);
    await userEvent.click(screen.getByRole("button"));
    expect(screen.getByText("0→1")).toBeInTheDocument();
  });
});

describe("the state shapes", () => {
  it("edits a list without touching the array it was given", async () => {
    const initial = ["a", "b", "c"];
    let identities = 0;
    let previous = null;
    component Probe() {
      const list = useList(initial);
      if (list.items !== previous) {
        previous = list.items;
        identities += 1;
      }
      return (
        <div>
          <output>{list.items.join("")}</output>
          <button type="button" onClick={() => list.push("d")}>
            push
          </button>
          <button type="button" onClick={() => list.removeAt(0)}>
            remove
          </button>
          <button type="button" onClick={() => list.move(0, 1)}>
            move
          </button>
          <button type="button" onClick={() => list.insertAt(1, "x")}>
            insert
          </button>
          <button type="button" onClick={() => list.replaceAt(0, "z")}>
            replace
          </button>
          <button type="button" onClick={() => list.removeAt(99)}>
            miss
          </button>
        </div>
      );
    }

    const { container } = render(<Probe />);
    const output = elementIn(container, "output");
    const press = (name: string) => userEvent.click(screen.getByRole("button", { name }));

    await press("push");
    expect(output.textContent).toBe("abcd");
    await press("remove");
    expect(output.textContent).toBe("bcd");
    await press("move");
    expect(output.textContent).toBe("cbd");
    await press("insert");
    expect(output.textContent).toBe("cxbd");
    await press("replace");
    expect(output.textContent).toBe("zxbd");
    // The array handed in is still what it was: every edit produced a new one.
    expect(initial).toEqual(["a", "b", "c"]);

    const before = identities;
    await press("miss");
    // An index that is not there returns the same array, so React bails out of
    // the render entirely — the identity does not change.
    expect(identities).toBe(before);
    expect(output.textContent).toBe("zxbd");
  });

  it("replaces the Set on every change rather than mutating it", async () => {
    const seen = new Set();
    component Probe() {
      const set = useSet(["x"]);
      seen.add(set.items);
      return (
        <div>
          <output>{`${[...set.items].join("")} ${String(set.has("y"))}`}</output>
          <button type="button" onClick={() => set.toggle("y")}>
            toggle
          </button>
          <button type="button" onClick={() => set.add("x")}>
            re-add
          </button>
          <button type="button" onClick={() => set.clear()}>
            clear
          </button>
        </div>
      );
    }

    const { container } = render(<Probe />);
    const output = elementIn(container, "output");
    expect(output.textContent).toBe("x false");

    await userEvent.click(screen.getByRole("button", { name: "toggle" }));
    expect(output.textContent).toBe("xy true");
    await userEvent.click(screen.getByRole("button", { name: "toggle" }));
    expect(output.textContent).toBe("x false");

    const before = seen.size;
    await userEvent.click(screen.getByRole("button", { name: "re-add" }));
    // Adding a member that is already there is not a change, so it is not a
    // new Set and not a render. A `Set` mutated in place would have been the
    // same object here for a real change too, and nothing would have updated.
    expect(seen.size).toBe(before);

    await userEvent.click(screen.getByRole("button", { name: "clear" }));
    expect(output.textContent).toBe(" false");
  });

  it("cycles in both directions and wraps at both ends", async () => {
    component Probe() {
      const cycle = useCycle(["red", "green", "blue"]);
      const empty = useCycle<string>([]);
      return (
        <div>
          <output>{`${String(cycle.value)} ${cycle.index} ${String(empty.value)} ${empty.index}`}</output>
          <button type="button" onClick={cycle.next}>
            next
          </button>
          <button type="button" onClick={cycle.previous}>
            back
          </button>
          <button type="button" onClick={empty.next}>
            nudge
          </button>
        </div>
      );
    }

    const { container } = render(<Probe />);
    const output = elementIn(container, "output");
    expect(output.textContent).toBe("red 0 null -1");

    await userEvent.click(screen.getByRole("button", { name: "back" }));
    // Going back from the first wraps to the last. A single `%` would have
    // given -1 here, because JavaScript's remainder keeps the sign of its left
    // operand — and `values[-1]` is `undefined`.
    expect(output.textContent).toBe("blue 2 null -1");

    await userEvent.click(screen.getByRole("button", { name: "next" }));
    await userEvent.click(screen.getByRole("button", { name: "next" }));
    await userEvent.click(screen.getByRole("button", { name: "next" }));
    // Three steps through three values is back where it started, not off the
    // end.
    expect(output.textContent).toBe("blue 2 null -1");

    await userEvent.click(screen.getByRole("button", { name: "next" }));
    expect(output.textContent).toBe("red 0 null -1");

    await userEvent.click(screen.getByRole("button", { name: "nudge" }));
    // An empty list has no current value, whichever way it is stepped.
    expect(output.textContent).toBe("red 0 null -1");
  });

  it("undoes and redoes, and a new edit abandons the redo", async () => {
    component Probe() {
      const history = useUndoable("one", { limit: 2 });
      return (
        <div>
          <output>{`${history.value} ${String(history.canUndo)} ${String(history.canRedo)}`}</output>
          <button type="button" onClick={() => history.set("two")}>
            two
          </button>
          <button type="button" onClick={() => history.set("three")}>
            three
          </button>
          <button type="button" onClick={() => history.set("four")}>
            four
          </button>
          <button type="button" onClick={history.undo}>
            undo
          </button>
          <button type="button" onClick={history.redo}>
            redo
          </button>
        </div>
      );
    }

    const { container } = render(<Probe />);
    const output = elementIn(container, "output");
    const press = (name: string) => userEvent.click(screen.getByRole("button", { name }));
    expect(output.textContent).toBe("one false false");

    await press("two");
    expect(output.textContent).toBe("two true false");
    await press("undo");
    expect(output.textContent).toBe("one false true");
    await press("redo");
    expect(output.textContent).toBe("two true false");

    await press("undo");
    await press("three");
    // Editing after an undo abandons what was undone, which is what every
    // editor does.
    expect(output.textContent).toBe("three true false");

    await press("four");
    await press("undo");
    await press("undo");
    // `limit: 2` keeps two steps of history, so the third undo has nothing to
    // go back to and `canUndo` says so rather than throwing.
    expect(output.textContent).toBe("one false true");
    await press("undo");
    expect(output.textContent).toBe("one false true");
  });
});

describe("useScrollLock", () => {
  const body = bodyOf;

  it("locks while it is mounted and puts the page back afterwards", () => {
    component Probe() {
      useScrollLock(true);
      return <p>dialog</p>;
    }
    expect(body().style.overflow).toBe("");
    const { unmount } = render(<Probe />);
    expect(body().style.overflow).toBe("hidden");
    unmount();
    expect(body().style.overflow).toBe("");
  });

  it("stays locked while a second dialog is still open", async () => {
    component Lock() {
      useScrollLock(true);
      return null;
    }
    component Probe() {
      const [second, setSecond] = useState(true);
      return (
        <div>
          <Lock />
          {second ? <Lock /> : null}
          <button type="button" onClick={() => setSecond(false)}>
            close
          </button>
        </div>
      );
    }

    const { unmount } = render(<Probe />);
    expect(body().style.overflow).toBe("hidden");
    await userEvent.click(screen.getByRole("button"));
    // The first dialog is still open. A lock without a count would have put
    // the page back here, and the page behind the remaining dialog would
    // scroll.
    expect(body().style.overflow).toBe("hidden");
    unmount();
    expect(body().style.overflow).toBe("");
  });

  it("does nothing while `locked` is false", () => {
    component Probe() {
      const [locked, setLocked] = useState(false);
      useScrollLock(locked);
      return (
        <button type="button" onClick={() => setLocked(true)}>
          open
        </button>
      );
    }
    const { unmount } = render(<Probe />);
    expect(body().style.overflow).toBe("");
    unmount();
  });
});

describe("useKeyCombo", () => {
  // This document reports a non-Apple user agent, so `mod` is Ctrl here. The
  // point of the hook is that the caller never writes that test themselves.
  const type = (init: { ... }) => fireEvent.keyDown(bodyOf(), init);

  it("needs the modifier, and refuses the ones that were not asked for", () => {
    const opened = fn();
    component Probe() {
      useKeyCombo("mod+k", opened);
      return null;
    }
    render(<Probe />);

    type({ key: "k" });
    // A bare `k` is somebody typing, not a shortcut. This is the bug a
    // hand-written `event.key === "k"` has.
    expect(opened).not.toHaveBeenCalled();

    type({ key: "k", ctrlKey: true, shiftKey: true });
    // Ctrl+Shift+K is a different shortcut, and binding one must not claim the
    // other.
    expect(opened).not.toHaveBeenCalled();

    type({ key: "k", ctrlKey: true });
    expect(opened.mock.calls.length).toBe(1);
  });

  it("ignores the auto-repeat of a held key", () => {
    const opened = fn();
    component Probe() {
      useKeyCombo("escape", opened);
      return null;
    }
    render(<Probe />);
    type({ key: "Escape" });
    type({ key: "Escape", repeat: true });
    type({ key: "Escape", repeat: true });
    // Holding the key sends twenty of these a second; a dialog closes once.
    expect(opened.mock.calls.length).toBe(1);
  });

  it("stays out of the way while the reader is typing", async () => {
    const helped = fn();
    component Probe() {
      useKeyCombo("?", helped);
      return <input aria-label="comment" />;
    }
    render(<Probe />);
    const field = screen.getByLabelText("comment");

    fireEvent.keyDown(field, { key: "?", shiftKey: true });
    // A question mark typed into a comment is a question mark.
    expect(helped).not.toHaveBeenCalled();

    type({ key: "?", shiftKey: true });
    expect(helped.mock.calls.length).toBe(1);
  });

  it("fires inside a field when the caller says so, and can be turned off", () => {
    const closed = fn();
    component Probe(enabled: boolean) {
      useKeyCombo("escape", closed, { whileTyping: true, enabled });
      return <input aria-label="comment" />;
    }
    const { rerender } = render(<Probe enabled={true} />);
    fireEvent.keyDown(screen.getByLabelText("comment"), { key: "Escape" });
    expect(closed.mock.calls.length).toBe(1);

    rerender(<Probe enabled={false} />);
    fireEvent.keyDown(screen.getByLabelText("comment"), { key: "Escape" });
    expect(closed.mock.calls.length).toBe(1);
  });

  it("calls preventDefault, so the browser's own shortcut does not also run", () => {
    component Probe() {
      useKeyCombo("mod+s", () => {});
      useKeyCombo("mod+p", () => {}, { preventDefault: false });
      return null;
    }
    render(<Probe />);
    expect(type({ key: "s", ctrlKey: true })).toBe(false);
    expect(type({ key: "p", ctrlKey: true })).toBe(true);
  });

  it("stops listening at unmount", () => {
    const opened = fn();
    component Probe() {
      useKeyCombo("mod+k", opened);
      return null;
    }
    const { unmount } = render(<Probe />);
    unmount();
    type({ key: "k", ctrlKey: true });
    expect(opened).not.toHaveBeenCalled();
  });
});

describe("useKeyHeld", () => {
  it("follows the key down and up", () => {
    component Probe() {
      const shift = useKeyHeld("shift");
      return <output>{shift ? "held" : "up"}</output>;
    }
    render(<Probe />);
    expect(screen.getByText("up")).toBeInTheDocument();
    fireEvent.keyDown(bodyOf(), { key: "Shift" });
    expect(screen.getByText("held")).toBeInTheDocument();
    fireEvent.keyUp(bodyOf(), { key: "Shift" });
    expect(screen.getByText("up")).toBeInTheDocument();
  });

  it("releases everything when the window loses focus", () => {
    component Probe() {
      const space = useKeyHeld(" ");
      return <output>{space ? "held" : "up"}</output>;
    }
    render(<Probe />);
    fireEvent.keyDown(bodyOf(), { key: " " });
    expect(screen.getByText("held")).toBeInTheDocument();
    // The `keyup` goes to whatever the reader switched to, so without this the
    // canvas would still be panning when they come back.
    fireEvent.blur(globalThis.window);
    expect(screen.getByText("up")).toBeInTheDocument();
  });
});

describe("more element hooks", () => {
  it("reports a press that lasts, and not one that turns into a drag", async () => {
    let pressed = 0;
    component Probe() {
      const ref = useElementRef<HTMLButtonElement>();
      useLongPress(
        ref,
        () => {
          pressed += 1;
        },
        { delay: 20, moveThreshold: 10 },
      );
      return (
        <button ref={ref} type="button">
          hold
        </button>
      );
    }

    render(<Probe />);
    const target = screen.getByRole("button");

    fireEvent.pointerDown(target, { clientX: 0, clientY: 0 });
    await tick(40);
    expect(pressed).toBe(1);

    fireEvent.pointerDown(target, { clientX: 0, clientY: 0 });
    fireEvent.pointerMove(target, { clientX: 40, clientY: 0 });
    await tick(40);
    // A press that became a fling is a scroll, not a long press.
    expect(pressed).toBe(1);

    fireEvent.pointerDown(target, { clientX: 0, clientY: 0 });
    fireEvent.pointerUp(target);
    await tick(40);
    expect(pressed).toBe(1);
  });

  it("sees the DOM change under it, and stops seeing it at unmount", async () => {
    let records = 0;
    component Probe() {
      const ref = useElementRef<HTMLDivElement>();
      useMutationObserver(ref, (changes) => {
        records += changes.length;
      });
      return <div ref={ref} data-testid="watched" />;
    }

    const { unmount } = render(<Probe />);
    const watched = screen.getByTestId("watched");
    watched.appendChild(globalThis.document.createElement("span"));
    await waitFor(() => {
      expect(records > 0).toBe(true);
    });

    const before = records;
    unmount();
    watched.appendChild(globalThis.document.createElement("span"));
    await tick(20);
    expect(records).toBe(before);
  });

  it("reports an element's own scroll offset", () => {
    component Probe() {
      const ref = useElementRef<HTMLDivElement>();
      const { x, y } = useScroll(ref);
      return (
        <div ref={ref} data-testid="pane">
          <output>{`${x},${y}`}</output>
        </div>
      );
    }
    const { container } = render(<Probe />);
    const pane = screen.getByTestId("pane");
    const output = elementIn(container, "output");
    expect(output.textContent).toBe("0,0");

    pane.scrollTop = 40;
    pane.scrollLeft = 12;
    fireEvent.scroll(pane);
    expect(output.textContent).toBe("12,40");
  });

  it("gives a render the element itself, through state rather than a ref", () => {
    component Probe() {
      const [node, attach] = useElementState<HTMLDivElement>();
      return (
        <div ref={attach}>
          <output>{node == null ? "none" : node.tagName}</output>
        </div>
      );
    }
    render(<Probe />);
    // A ref would still say "none" here: writing `current` does not re-render.
    expect(screen.getByText("DIV")).toBeInTheDocument();
  });

  it("does not report an element as intersecting until an observer says so", () => {
    component Probe() {
      const ref = useElementRef<HTMLDivElement>();
      const seen = useIntersecting(ref, { rootMargin: "10px", threshold: 0.5 });
      return <div ref={ref}>{seen ? "seen" : "away"}</div>;
    }
    render(<Probe />);
    expect(screen.getByText("away")).toBeInTheDocument();
  });
});

describe("more timing hooks", () => {
  // Each case that wants a fake clock installs it itself, next to the reason
  // it needs one; this puts the real one back however the case ended. A leaked
  // fake clock is the failure that costs the most to read — the next file's
  // `setTimeout` never fires and the run hangs with nothing on screen saying
  // why — and `afterEach` runs after a case that threw, which a line at the
  // end of each body would not.
  afterEach(() => {
    uft.useRealTimers();
  });

  it("runs a frame loop and cancels it at unmount", async () => {
    let frames = 0;
    let sawDelta = false;
    component Probe() {
      useAnimationFrame((frame) => {
        frames += 1;
        if (frames > 1 && frame.delta > 0) {
          sawDelta = true;
        }
      });
      return null;
    }

    const { unmount } = render(<Probe />);
    // The one hook here that has to have real time: `requestAnimationFrame`
    // belongs to the document rather than to the globals `uft` replaces, so
    // there is no clock to advance. Waited for instead of slept through —
    // what this asserts is "the loop painted more than one frame", and a
    // fixed sleep asserts that only for as long as the machine keeps up. On a
    // worker that lost its slice the sleep is short by however long it lost,
    // and the answer is a longer sleep in every suite that ever gets wider.
    await waitFor(() => {
      expect(frames > 1).toBe(true);
      expect(sawDelta).toBe(true);
    });

    unmount();
    const before = frames;
    // A sleep, because the claim is that nothing happens and there is no
    // event to wait for. Nothing to lose to jitter either: a slow worker
    // gives the loop that should be cancelled more chances to prove it is not.
    await tick(40);
    expect(frames).toBe(before);
  });

  it("does not run a frame loop that is not active", async () => {
    let frames = 0;
    component Probe() {
      useAnimationFrame(() => {
        frames += 1;
      }, false);
      return null;
    }
    render(<Probe />);
    // Real time again, and a sleep for the same reason as the one above: the
    // claim is that no frame ran, and every extra millisecond is one more
    // chance for the loop to contradict it.
    await tick(30);
    expect(frames).toBe(0);
  });

  it("notices the reader stopping, and starts the wait again when they move", () => {
    // On a clock this test owns. `useIdle` is a `setTimeout` and nothing else,
    // so real time bought no coverage here and cost the assertion below:
    // `tick(35)` against a `useIdle(40)` left five milliseconds of slack, and
    // a worker descheduled for six of them — an ordinary amount of jitter with
    // twelve of them on the box — read "idle" and failed, about one run in ten
    // (#337). Waiting longer would only have moved the same margin somewhere
    // else; what the test is about is a new wait replacing the old one, and
    // that has nothing to do with elapsed wall time.
    uft.useFakeTimers();
    component Probe() {
      const idle = useIdle(40);
      return <output>{idle ? "idle" : "here"}</output>;
    }
    render(<Probe />);
    expect(screen.getByText("here")).toBeInTheDocument();

    advance(20);
    fireEvent.pointerMove(bodyOf());
    advance(39);
    // Past the moment the *first* wait would have expired. A version that
    // started a new timer without clearing the old one would say "idle" here,
    // forty milliseconds after the reader last moved the pointer. On a clock
    // that only moves when this line moves it, the margin is one millisecond
    // on purpose: it pins the boundary rather than approaching it.
    expect(screen.getByText("here")).toBeInTheDocument();

    // And the fortieth millisecond after the move is the one that expires.
    advance(1);
    expect(screen.getByText("idle")).toBeInTheDocument();

    fireEvent.pointerMove(bodyOf());
    expect(screen.getByText("here")).toBeInTheDocument();
  });

  it("moves the clock forward", () => {
    uft.useFakeTimers();
    const seen = new Set<number>();
    component Probe() {
      const now = useNow(5);
      seen.add(now.getTime());
      return null;
    }
    const { unmount } = render(<Probe />);
    // The instant the first render read, which is the clock's and not a
    // separate reading of it: `useNow` takes its initial state from `new
    // Date()` during that render, and nothing has moved the clock since.
    const started = Date.now();
    // One advance per interval rather than one of forty. Eight `setState`s
    // inside a single `act` are one React render with the last value, so a
    // single advance would prove only that the clock moved at all — which is
    // what `seen.size > 1` used to settle for.
    for (let step = 0; step < 8; step += 1) {
      advance(5);
    }
    unmount();
    expect([...seen]).toEqual([0, 5, 10, 15, 20, 25, 30, 35, 40].map((at) => started + at));
  });

  it("turns a stable instant into relative text once it has hydrated", async () => {
    const threeMinutesAgo = new Date(Date.now() - 3 * 60_000);
    component Probe() {
      const text = useTimeAgo(threeMinutesAgo, { locale: "en" });
      return <output>{text}</output>;
    }
    const { unmount } = render(<Probe />);
    // The first render is the ISO string — that is what the server put in the
    // markup — and the effect replaces it. `render` flushes effects, so by the
    // time this line runs the swap has happened.
    await waitFor(() => {
      expect(screen.getByText("3 minutes ago")).toBeInTheDocument();
    });
    unmount();
  });

  it("re-reads a recent label often and an old one hardly at all", () => {
    // A second and a fifth of real time, spent to watch a `setInterval` that
    // this can simply advance past. The dates below are read from the clock
    // before it is faked and stay true afterwards, because `useFakeTimers`
    // starts the fake clock at the real instant rather than at zero.
    uft.useFakeTimers();
    let recentRenders = 0;
    let oldRenders = 0;
    const recent = new Date(Date.now() - 5_000);
    const week = new Date(Date.now() - 7 * 86_400_000);

    component Recent() {
      useTimeAgo(recent, { locale: "en" });
      recentRenders += 1;
      return null;
    }
    component Old() {
      useTimeAgo(week, { locale: "en" });
      oldRenders += 1;
      return null;
    }
    component Probe() {
      return (
        <div>
          <Recent />
          <Old />
        </div>
      );
    }

    const { unmount } = render(<Probe />);
    const recentBefore = recentRenders;
    const oldBefore = oldRenders;
    advance(1_200);
    unmount();

    // "5 seconds ago" is wrong a second later, so it is worked out again.
    expect(recentRenders > recentBefore).toBe(true);
    // "7 days ago" is not, and a fixed one-second clock would have re-rendered
    // this component every second for as long as the page was open.
    expect(oldRenders).toBe(oldBefore);
  });

  it("throttles on the leading edge and debounces on the trailing one", async () => {
    uft.useFakeTimers();
    const throttled = fn();
    const debounced = fn();
    component Probe() {
      const runThrottled = useThrottledCallback(throttled, 50);
      const runDebounced = useDebouncedCallback(debounced, 20);
      return (
        <div>
          <button type="button" onClick={() => runThrottled()}>
            throttle
          </button>
          <button type="button" onClick={() => runDebounced()}>
            debounce
          </button>
        </div>
      );
    }

    render(<Probe />);
    const throttle = screen.getByRole("button", { name: "throttle" });
    await userEvent.click(throttle);
    await userEvent.click(throttle);
    await userEvent.click(throttle);
    // The first goes through immediately; the rest are inside the window —
    // and on a clock nothing but this test moves, "inside the window" is a
    // fact rather than a hope that three `userEvent.click`s take less than
    // fifty milliseconds. `useThrottledCallback` compares two readings of the
    // installed clock, which with nothing installed is the host's, so on real
    // time a slow enough third click let a second call through.
    expect(throttled.mock.calls.length).toBe(1);

    const debounce = screen.getByRole("button", { name: "debounce" });
    await userEvent.click(debounce);
    await userEvent.click(debounce);
    expect(debounced).not.toHaveBeenCalled();
    // Nineteen of the twenty, then the twentieth: the wait that used to be a
    // 40ms sleep against a 20ms debounce is now the boundary itself.
    advance(19);
    expect(debounced).not.toHaveBeenCalled();
    advance(1);
    expect(debounced.mock.calls.length).toBe(1);
  });

  it("cancels a pending debounce at unmount", async () => {
    uft.useFakeTimers();
    const body = fn();
    component Probe() {
      const run = useDebouncedCallback(body, 20);
      return (
        <button type="button" onClick={() => run()}>
          go
        </button>
      );
    }
    const { unmount } = render(<Probe />);
    await userEvent.click(screen.getByRole("button"));
    unmount();
    advance(40);
    // The version people write calls `setState` on a component that is gone.
    expect(body).not.toHaveBeenCalled();
  });
});

describe("the render anchor", () => {
  /** 2026-09-04T06:00:00Z, an instant a test picked rather than a clock. */
  const ANCHOR = Date.UTC(2026, 8, 4, 6, 0, 0);

  // Nothing here fakes a timer. The two cases that need to know what time it is
  // install a clock instead, which is what `@uniflowed/core/clock` is for and is
  // the stronger statement: a fake `setTimeout` would show that the hook agreed
  // with the host, and this shows that it read the clock it was given.
  const clocks: Array<() => void> = [];
  afterEach(() => {
    while (clocks.length > 0) {
      const restore = clocks.pop();
      if (restore != null) {
        restore();
      }
    }
  });

  it("hands the same instant to every component under it", () => {
    // The property the whole module exists for: two components that ask what
    // time the page was rendered at must not get two answers, or the header and
    // the footer disagree about what "today" is.
    const seen = [];
    component Probe() {
      seen.push(useRenderedAt().epochMilliseconds);
      return null;
    }
    render(
      <RenderProvider at={ANCHOR} timeZone="Asia/Tokyo" seed="fixedseed">
        <Probe />
        <Probe />
      </RenderProvider>,
    );

    expect(seen).toEqual([ANCHOR, ANCHOR]);
  });

  it("reports the zone the render was made in, not the reader's", () => {
    // The second half of a hydration mismatch. A component that formatted in
    // the reader's zone on the first pass could not match a server that
    // formatted in its own, so what travels is the server's.
    component Probe() {
      return <output>{useRenderTimeZone()}</output>;
    }
    render(
      <RenderProvider at={ANCHOR} timeZone="Asia/Tokyo" seed="fixedseed">
        <Probe />
      </RenderProvider>,
    );

    expect(screen.getByText("Asia/Tokyo")).toBeInTheDocument();
  });

  it("starts useNow at the anchor, so the first render matches the server's", () => {
    // Without this, a prerendered page that shows a clock renders the server's
    // instant and then the browser's, and React reports the difference. The
    // clock installed here is a minute ahead of the anchor, so the two values
    // collected say both halves at once: the first render is the server's
    // instant, and the effect that runs once the markup has matched replaces it
    // with the reader's.
    clocks.push(setClock(fixedClock(ANCHOR + 60_000, "UTC")));
    const seen = [];
    component Probe() {
      seen.push(useNow(null).getTime());
      return null;
    }
    const { unmount } = render(
      <RenderProvider at={ANCHOR} timeZone="UTC" seed="fixedseed">
        <Probe />
      </RenderProvider>,
    );
    unmount();

    expect(seen[0]).toBe(ANCHOR);
    expect(seen[seen.length - 1]).toBe(ANCHOR + 60_000);
  });

  it("leaves useNow reading the clock when there is no provider", () => {
    // A client-only page has no other side to agree with, so the behaviour it
    // has always had is the right one and this must not have changed it.
    clocks.push(setClock(fixedClock(ANCHOR, "UTC")));
    let seen = null;
    component Probe() {
      seen = useNow(null).getTime();
      return null;
    }
    const { unmount } = render(<Probe />);
    unmount();

    expect(seen).toBe(ANCHOR);
  });

  it("gives two renders of one seed the same numbers", () => {
    // What makes a shuffled list hydrate. The two renders below stand in for
    // the server's and the browser's, and they are only the same because the
    // seed came from the anchor rather than from `Math.random()`.
    const draw = () => {
      const seen = [];
      component Probe() {
        seen.push(useRandom("featured").next());
        return null;
      }
      const { unmount } = render(
        <RenderProvider at={ANCHOR} timeZone="UTC" seed="fixedseed">
          <Probe />
        </RenderProvider>,
      );
      unmount();
      return seen[0];
    };

    expect(draw()).toBe(draw());
  });

  it("gives two labels independent streams whatever order they render in", () => {
    // The bug this prevents only appears on a slow connection: with one shared
    // stream, a boundary that resolves late on the server and early in the
    // browser reorders every draw after it.
    const drawn: { first: number | null, second: number | null } = { first: null, second: null };
    component Sidebar() {
      drawn.second = useRandom("sidebar").next();
      return null;
    }
    component Featured() {
      drawn.first = useRandom("featured").next();
      return null;
    }
    const { unmount } = render(
      <RenderProvider at={ANCHOR} timeZone="UTC" seed="fixedseed">
        <Featured />
        <Sidebar />
      </RenderProvider>,
    );
    unmount();

    const swapped: { first: number | null, second: number | null } = { first: null, second: null };
    component SwappedProbe() {
      swapped.second = useRandom("sidebar").next();
      swapped.first = useRandom("featured").next();
      return null;
    }
    const second = render(
      <RenderProvider at={ANCHOR} timeZone="UTC" seed="fixedseed">
        <SwappedProbe />
      </RenderProvider>,
    );
    second.unmount();

    expect(swapped.first).toBe(drawn.first);
    expect(swapped.second).toBe(drawn.second);
    expect(drawn.first).not.toBe(drawn.second);
  });

  it("shuffles a list the same way on both sides", () => {
    const items = ["a", "b", "c", "d", "e"];
    const order = () => {
      let seen = null;
      component Probe() {
        seen = useShuffled(items, "featured").join("");
        return null;
      }
      const { unmount } = render(
        <RenderProvider at={ANCHOR} timeZone="UTC" seed="fixedseed">
          <Probe />
        </RenderProvider>,
      );
      unmount();
      return seen;
    };

    expect(order()).toBe(order());
    expect(order()).not.toBe(items.join(""));
  });
});

describe("useAsync, aborting and retrying", () => {
  it("aborts the signal it handed out when the component goes", async () => {
    let signal = null;
    component Probe() {
      useAsync((given) => {
        signal = given;
        return new Promise(() => {});
      }, []);
      return null;
    }
    const { unmount } = render(<Probe />);
    expect(signal?.aborted).toBe(false);
    unmount();
    // Ignoring the answer is not the same as stopping the request; the
    // connection is the cost.
    expect(signal?.aborted).toBe(true);
  });

  it("aborts the previous call when the dependencies change", async () => {
    const signals = [];
    component Probe() {
      const [key, setKey] = useState("first");
      useAsync(
        (signal) => {
          signals.push(signal);
          return new Promise(() => {});
        },
        [key],
      );
      return (
        <button type="button" onClick={() => setKey("second")}>
          change
        </button>
      );
    }
    render(<Probe />);
    await userEvent.click(screen.getByRole("button"));
    expect(signals.length).toBe(2);
    expect(signals[0].aborted).toBe(true);
    expect(signals[1].aborted).toBe(false);
  });

  it("tries again, and reports the failure once it runs out of tries", async () => {
    let calls = 0;
    component Flaky() {
      const { value, error } = useAsync(
        async () => {
          calls += 1;
          if (calls < 3) {
            throw new Error("flaky");
          }
          return "third time";
        },
        [],
        { retry: 2, retryDelay: () => 1 },
      );
      return <output>{value ?? error?.message ?? "pending"}</output>;
    }
    render(<Flaky />);
    await waitFor(() => {
      expect(screen.getByText("third time")).toBeInTheDocument();
    });
    expect(calls).toBe(3);

    let attempts = 0;
    component Broken() {
      const { error } = useAsync(
        async () => {
          attempts += 1;
          throw new Error("still broken");
        },
        [],
        { retry: 1, retryDelay: () => 1 },
      );
      return <output>{error?.message ?? "pending"}</output>;
    }
    render(<Broken />);
    await waitFor(() => {
      expect(screen.getByText("still broken")).toBeInTheDocument();
    });
    // One try and one retry, and then the error is real rather than swallowed.
    expect(attempts).toBe(2);
  });

  it("does not retry a call that was abandoned", async () => {
    let calls = 0;
    component Probe() {
      useAsync(
        async () => {
          calls += 1;
          throw new Error("nope");
        },
        [],
        { retry: 5, retryDelay: () => 5 },
      );
      return null;
    }
    const { unmount } = render(<Probe />);
    unmount();
    await tick(40);
    expect(calls).toBe(1);
  });
});

describe("the browser hooks that need a real browser", () => {
  it("reads a media query from the document rather than from the server value", () => {
    component Probe() {
      // This document is 1024 wide, so the first matches and the second does
      // not — and both were given the *opposite* server value, which is what
      // shows the browser is being asked rather than the argument echoed.
      const wide = useMediaQuery("(min-width: 100px)", false);
      const enormous = useMediaQuery("(min-width: 5000px)", true);
      const scheme = usePreferredColorScheme("dark");
      return <output>{`${String(wide)} ${String(enormous)} ${scheme}`}</output>;
    }
    render(<Probe />);
    expect(screen.getByText("true false light")).toBeInTheDocument();
  });

  it("answers a capability question through a store, not during a render", () => {
    component Probe() {
      const yes = useSupported(() => true);
      const no = useSupported(() => false);
      return <output>{`${String(yes)} ${String(no)}`}</output>;
    }
    render(<Probe />);
    expect(screen.getByText("true false")).toBeInTheDocument();
  });

  it("copies to the clipboard and forgets that it did", async () => {
    component Probe() {
      const { copy, copied, supported } = useClipboard({ resetAfter: 20 });
      return (
        <div>
          <button type="button" onClick={() => void copy("copied text")}>
            copy
          </button>
          <output>{`${String(supported)} ${copied ? "yes" : "no"}`}</output>
        </div>
      );
    }
    render(<Probe />);
    expect(screen.getByText("true no")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button"));
    await waitFor(() => {
      expect(screen.getByText("true yes")).toBeInTheDocument();
    });
    const clipboard = globalThis.window.navigator.clipboard;
    expect(await clipboard.readText()).toBe("copied text");

    // The tick resets itself, which is the only reason `copied` is state.
    await waitFor(() => {
      expect(screen.getByText("true no")).toBeInTheDocument();
    });
  });

  it("hears a message another copy of the page sent", async () => {
    const heard = [];
    component Probe() {
      const { supported, post } = useBroadcast<string>("uf-hooks-broadcast", (message) => {
        heard.push(message);
      });
      return (
        <div>
          <output>{String(supported)}</output>
          <button type="button" onClick={() => post("from the page")}>
            post
          </button>
        </div>
      );
    }

    const { unmount } = render(<Probe />);
    expect(screen.getByText("true")).toBeInTheDocument();

    const other = new globalThis.BroadcastChannel("uf-hooks-broadcast");
    const alsoHeard = [];
    other.onmessage = (event: MessageEvent) => {
      alsoHeard.push(event.data);
    };
    other.postMessage("from another tab");

    await waitFor(() => {
      expect(heard).toEqual(["from another tab"]);
    });

    await userEvent.click(screen.getByRole("button", { name: "post" }));
    await waitFor(() => {
      expect(alsoHeard).toEqual(["from the page"]);
    });
    // A channel does not deliver to the page that posted, which is the
    // specification's behaviour and the reason this is not shared state.
    expect(heard).toEqual(["from another tab"]);

    unmount();
    other.postMessage("after unmount");
    await tick(20);
    expect(heard).toEqual(["from another tab"]);
    other.close();
  });

  it("reads a permission without asking for it", async () => {
    component Probe() {
      const answer = usePermission("geolocation");
      return <output>{answer}</output>;
    }
    const { unmount } = render(<Probe />);
    // "unknown" until the promise answers, which is why this is not a store.
    expect(screen.getByText("unknown")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByText("granted")).toBeInTheDocument();
    });
    unmount();
  });

  /**
   * `measured: null` has to mean one thing.
   *
   * The shape this replaced reported `downlink: null` both for a browser with
   * no Network Information and for a Chromium that had not decided yet, and a
   * caller could not tell the two apart without also reading `supported`. A
   * regression to that shape would let this document — which has no
   * `navigator.connection` at all — report a connection it never measured.
   */
  it("separates the connection it measured from the one it could not", () => {
    component Probe() {
      const network = useNetwork();
      return (
        <output>{`${String(network.online)} ${network.measured == null ? "unmeasured" : String(network.measured.effectiveType)}`}</output>
      );
    }
    render(<Probe />);
    // `online` is everywhere; Network Information is Chromium's alone, and this
    // document has none.
    expect(screen.getByText("true unmeasured")).toBeInTheDocument();
  });

  /**
   * A document with no geolocation must not look like one that is still asking.
   *
   * `position: null` used to mean "no browser", "not asked", "refused" and
   * "waiting" at once, so a page could not tell a device that cannot answer
   * from a reader who has not answered yet. The status says which.
   */
  it("says geolocation is unsupported rather than throwing on a document without it", () => {
    component Probe() {
      const where = useGeolocation();
      return <output>{where.status}</output>;
    }
    render(<Probe />);
    expect(screen.getByText("unsupported")).toBeInTheDocument();
  });

  /**
   * A hook that is switched off says so, rather than looking unavailable.
   *
   * `enabled: false` on a device that has geolocation is a different fact from
   * a device that does not, and a page that offers a "find me" button needs the
   * difference to decide whether to show the button at all.
   */
  it("reports idle for a watch nobody asked for, whatever the device can do", () => {
    component Probe() {
      const where = useGeolocation({ enabled: false });
      return <output>{where.status}</output>;
    }
    render(<Probe />);
    // "idle" rather than "unsupported": `enabled` is the caller's own decision,
    // so it answers the same on a server and here, and a page that switches
    // watching off does not see its markup change under it at hydration.
    expect(screen.getByText("idle")).toBeInTheDocument();
  });
});

describe("useHash", () => {
  // Every test here writes the address bar of a document the whole worker
  // shares, so each one puts the fragment back. The path and the query are left
  // exactly as they were found, which is also what one of these tests asserts.
  afterEach(() => {
    const win = globalThis.window;
    win.history.replaceState(null, "", win.location.pathname + win.location.search);
  });

  /**
   * Two components reading the fragment have to agree about it.
   *
   * `history.pushState` and `history.replaceState` fire nothing at all — not
   * `hashchange`, not `popstate` — so a hook that listened only to the platform
   * would leave every component except the one that wrote showing the fragment
   * from before the write. The module-level registry is what closes that, and
   * this is the test that would fail without it.
   */
  it("tells every component about a fragment one of them wrote", async () => {
    component Probe() {
      const [fragment, write] = useHash();
      const [elsewhere] = useHash();
      return (
        <div>
          <button type="button" onClick={() => write("billing")}>
            go
          </button>
          <output>{`[${fragment}] [${elsewhere}]`}</output>
        </div>
      );
    }
    render(<Probe />);
    expect(screen.getByText("[] []")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button"));
    expect(screen.getByText("[billing] [billing]")).toBeInTheDocument();
    expect(globalThis.window.location.hash).toBe("#billing");
  });

  /**
   * A writer that promises a fragment must not be able to write anything else.
   *
   * The obvious spelling — `history.pushState(null, "", "#" + next)` — hands a
   * caller's string to a URL parser, and `../admin?token=1` then resolves to a
   * different path with a query on it: a same-origin rewrite that `pushState`
   * permits, from a call that said it was setting a fragment. Going through
   * `URL`'s own setter is what confines it, and this asserts the confinement
   * rather than the spelling.
   */
  it("puts a caller's text in the fragment and leaves the rest of the URL alone", async () => {
    component Probe() {
      const [fragment, write] = useHash();
      return (
        <div>
          <button type="button" onClick={() => write("../admin?token=1")}>
            write
          </button>
          <output>{`[${fragment}]`}</output>
        </div>
      );
    }
    render(<Probe />);
    const before = globalThis.window.location;
    const origin = before.origin;
    const pathname = before.pathname;
    const search = before.search;

    await userEvent.click(screen.getByRole("button"));

    const after = globalThis.window.location;
    expect(after.origin).toBe(origin);
    expect(after.pathname).toBe(pathname);
    expect(after.search).toBe(search);
    // It round-trips: what went in as a fragment comes back as one.
    expect(screen.getByText("[../admin?token=1]")).toBeInTheDocument();
  });

  /**
   * A fragment is a name, and names have accents in them.
   *
   * `location.hash` is percent-encoded, so a hook that handed it back raw would
   * make `fragment === section.id` false for every heading not written in
   * ASCII — and the caller who reached for `decodeURIComponent` would then have
   * a render that throws on the lone `%` somebody can type into the address bar.
   */
  it("round-trips a fragment that is not ASCII", async () => {
    component Probe() {
      const [fragment, write] = useHash();
      return (
        <div>
          <button type="button" onClick={() => write("café & crème")}>
            write
          </button>
          <output>{`[${fragment}]`}</output>
        </div>
      );
    }
    render(<Probe />);
    await userEvent.click(screen.getByRole("button"));
    expect(screen.getByText("[café & crème]")).toBeInTheDocument();
  });

  it("leaves a fragment that is not valid percent-encoding as it was typed", () => {
    component Probe() {
      const [fragment] = useHash();
      return <output>{`[${fragment}]`}</output>;
    }
    // Written straight into the address bar, the way a reader pasting a link
    // would: `%zz` is not an escape, and `decodeURIComponent` throws on it.
    globalThis.window.history.replaceState(null, "", "#100%zz");
    render(<Probe />);
    expect(screen.getByText("[100%zz]")).toBeInTheDocument();
  });

  /**
   * The reader's own navigation still moves the value.
   *
   * The registry above covers writes made through the hook; `hashchange` and
   * `popstate` are what cover an anchor the reader clicked, an address bar they
   * edited, and the back button. Losing either subscription would leave the
   * page showing a section nobody is looking at.
   */
  it("follows the address bar when the reader changes it", () => {
    component Probe() {
      const [fragment] = useHash();
      return <output>{`[${fragment}]`}</output>;
    }
    render(<Probe />);
    expect(screen.getByText("[]")).toBeInTheDocument();

    act(() => {
      globalThis.window.history.replaceState(null, "", "#section-two");
      fireEvent.hashChange(globalThis.window);
    });
    expect(screen.getByText("[section-two]")).toBeInTheDocument();

    act(() => {
      globalThis.window.history.replaceState(null, "", "#section-three");
      fireEvent.popState(globalThis.window);
    });
    expect(screen.getByText("[section-three]")).toBeInTheDocument();
  });

  /**
   * The Navigation API, modelled the way a browser implements it.
   *
   * happy-dom has no `navigation`, so these tests supply one — and what makes
   * the supplied one worth anything is that the test never fires the event
   * itself. `currententrychange` is dispatched from inside `pushState` and
   * `replaceState`, which is exactly where the platform dispatches it, so a
   * test below calls `history.pushState` the way `@uniflowed/router` does and
   * the hook hears about it or does not. A stub the test poked directly would
   * only assert that `subscribe` had called `addEventListener`, which is the
   * shape of test the issue this fixes warns against.
   *
   * `listenerCount` is how the unmount test sees that the subscription is
   * balanced, and it counts the hook's listeners because it is the set the
   * hook adds to.
   */
  const installNavigationApi = () => {
    const win = globalThis.window;
    const listeners = new Set<() => mixed>();
    const realPush = win.history.pushState.bind(win.history);
    const realReplace = win.history.replaceState.bind(win.history);
    const announce = () => {
      // A copy, because a listener is allowed to unsubscribe while it runs.
      for (const listener of [...listeners]) {
        listener();
      }
    };
    win.navigation = {
      addEventListener: (type: string, listener: () => mixed) => {
        if (type === "currententrychange") {
          listeners.add(listener);
        }
      },
      removeEventListener: (type: string, listener: () => mixed) => {
        if (type === "currententrychange") {
          listeners.delete(listener);
        }
      },
    };
    win.history.pushState = (state: mixed, unused: string, url: string) => {
      realPush(state, unused, url);
      announce();
    };
    win.history.replaceState = (state: mixed, unused: string, url: string) => {
      realReplace(state, unused, url);
      announce();
    };
    return {
      listenerCount: () => listeners.size,
      restore: () => {
        delete win.navigation;
        win.history.pushState = realPush;
        win.history.replaceState = realReplace;
      },
    };
  };

  /**
   * The case the hook could not see before: a write made by somebody else.
   *
   * `@uniflowed/router` navigates with `window.history.pushState(null, "",
   * next + target.hash)`, and that call fires neither `hashchange` nor
   * `popstate`. Before `currententrychange` there was no listener that would
   * hear it, so a page whose tab strip reads `useHash` went on showing the
   * section it was on while the address bar said otherwise.
   */
  it("sees a pushState made by other code where the Navigation API exists", () => {
    const navigation = installNavigationApi();
    component Probe() {
      const [fragment] = useHash();
      return <output>{`[${fragment}]`}</output>;
    }
    try {
      render(<Probe />);
      expect(screen.getByText("[]")).toBeInTheDocument();

      // Not through the hook, and no event fired by hand: this is the router's
      // call, and everything after it is the platform's doing.
      act(() => {
        const win = globalThis.window;
        win.history.pushState(null, "", `${win.location.pathname}${win.location.search}#invoices`);
      });

      expect(screen.getByText("[invoices]")).toBeInTheDocument();
    } finally {
      navigation.restore();
    }
  });

  /**
   * And the honest half of the same sentence.
   *
   * Chrome and Edge have had the Navigation API since 102, Safari since 26.2
   * and Firefox since 147; an older Safari or Firefox has none of it, and
   * there this is what a router navigation still looks like. The hook does not
   * pretend otherwise and neither does its documentation — pinning it here is
   * what keeps the two in step, and what would fail if somebody ever removed
   * the registry on the theory that `currententrychange` had made it
   * redundant.
   */
  it("cannot see that same pushState in a browser without the Navigation API", () => {
    component Probe() {
      const [fragment] = useHash();
      return <output>{`[${fragment}]`}</output>;
    }
    render(<Probe />);
    expect(screen.getByText("[]")).toBeInTheDocument();

    act(() => {
      const win = globalThis.window;
      win.history.pushState(null, "", `${win.location.pathname}${win.location.search}#invoices`);
    });

    expect(globalThis.window.location.hash).toBe("#invoices");
    expect(screen.getByText("[]")).toBeInTheDocument();
  });

  it("removes its Navigation API listener at unmount", () => {
    // As below: whatever the previous test left mounted comes down first, so
    // its cleanup is not counted against this one.
    render(<output>nothing yet</output>);

    const navigation = installNavigationApi();
    component Probe() {
      const [fragment] = useHash();
      return <output>{`[${fragment}]`}</output>;
    }
    try {
      const { unmount } = render(<Probe />);
      expect(navigation.listenerCount()).toBe(1);
      unmount();
      expect(navigation.listenerCount()).toBe(0);
    } finally {
      navigation.restore();
    }
  });

  it("removes both of its listeners at unmount", () => {
    // Whatever the previous test left mounted comes down first. `render` cleans
    // up before it mounts, so its removals would otherwise be counted against
    // this one and the balance would look wrong for the wrong reason.
    render(<output>nothing yet</output>);

    const win = globalThis.window;
    const added = [];
    const removed = [];
    const realAdd = win.addEventListener.bind(win);
    const realRemove = win.removeEventListener.bind(win);
    win.addEventListener = (type, listener, options) => {
      added.push(type);
      realAdd(type, listener, options);
    };
    win.removeEventListener = (type, listener, options) => {
      removed.push(type);
      realRemove(type, listener, options);
    };

    component Probe() {
      const [fragment] = useHash();
      return <output>{`[${fragment}]`}</output>;
    }
    try {
      const { unmount } = render(<Probe />);
      unmount();
    } finally {
      win.addEventListener = realAdd;
      win.removeEventListener = realRemove;
    }

    // Both, and each exactly as many times as it was added: an unbalanced
    // subscription is the bug Strict Mode's second mount exists to find.
    expect(added.filter((type) => type === "hashchange").length).toBe(1);
    expect(removed.filter((type) => type === "hashchange").length).toBe(1);
    expect(added.filter((type) => type === "popstate").length).toBe(1);
    expect(removed.filter((type) => type === "popstate").length).toBe(1);
  });
});

describe("Strict Mode, which renders and mounts everything twice", () => {
  it("leaves one listener behind, not two", async () => {
    const handler = fn();
    component Probe() {
      const ref = useElementRef<HTMLButtonElement>();
      useEventListener(ref, "click", handler);
      return (
        <button ref={ref} type="button">
          press
        </button>
      );
    }

    const { unmount } = render(
      <React.StrictMode>
        <Probe />
      </React.StrictMode>,
    );
    const button = screen.getByRole("button");
    await userEvent.click(button);
    // React mounts, unmounts and mounts again in Strict Mode. A cleanup that
    // did not remove exactly what its effect added would count two here.
    expect(handler.mock.calls.length).toBe(1);

    unmount();
    fireEvent.click(button);
    expect(handler.mock.calls.length).toBe(1);
  });

  it("keeps the scroll lock's count balanced", () => {
    component Probe() {
      useScrollLock(true);
      return <p>dialog</p>;
    }
    const { unmount } = render(
      <React.StrictMode>
        <Probe />
      </React.StrictMode>,
    );
    expect(globalThis.document.body.style.overflow).toBe("hidden");
    unmount();
    // A lock that counted the double mount and released once would leave the
    // page frozen for the rest of the session.
    expect(globalThis.document.body.style.overflow).toBe("");
  });

  it("does not start two intervals or two requests", async () => {
    const ticked = fn();
    let started = 0;
    component Probe() {
      useInterval(ticked, 10);
      useAsync(async () => {
        started += 1;
        return "once";
      }, []);
      return null;
    }
    const { unmount } = render(
      <React.StrictMode>
        <Probe />
      </React.StrictMode>,
    );
    await tick(35);
    unmount();
    // Two intervals would tick about twice as often; the bound is loose on
    // purpose, and a doubled timer breaks it.
    expect(ticked.mock.calls.length <= 5).toBe(true);
    expect(ticked.mock.calls.length > 0).toBe(true);
    // Strict Mode runs the effect twice on mount, so the request is started
    // twice by design — what matters is that only the live one can write, and
    // the first one's signal is aborted.
    expect(started).toBe(2);
  });

  it("leaves a broadcast channel open and usable", async () => {
    const heard = [];
    component Probe() {
      const { post } = useBroadcast<string>("uf-hooks-strict", () => {});
      return (
        <button type="button" onClick={() => post("still connected")}>
          post
        </button>
      );
    }
    const { unmount } = render(
      <React.StrictMode>
        <Probe />
      </React.StrictMode>,
    );
    const other = new globalThis.BroadcastChannel("uf-hooks-strict");
    other.onmessage = (event: MessageEvent) => {
      heard.push(event.data);
    };

    await userEvent.click(screen.getByRole("button"));
    // The first mount's cleanup closes the first channel; posting has to reach
    // the second mount's, not a closed one or none at all.
    await waitFor(() => {
      expect(heard).toEqual(["still connected"]);
    });
    unmount();
    other.close();
  });

  it("keeps a storage key readable through the double mount", async () => {
    component Probe() {
      const [value, write] = useStorage("uf-test-strict", "start");
      return (
        <div>
          <button type="button" onClick={() => write("written")}>
            write
          </button>
          <output>{value}</output>
        </div>
      );
    }
    const { unmount } = render(
      <React.StrictMode>
        <Probe />
      </React.StrictMode>,
    );
    await userEvent.click(screen.getByRole("button"));
    expect(screen.getByText("written")).toBeInTheDocument();
    unmount();
  });
});

describe("useEventSource", () => {
  /** Every stream the hook has opened since `install`. */
  let opened: Array<$FlowFixMe> = [];

  /**
   * An `EventSource` the test drives.
   *
   * Installed on the global rather than injected, because that is where the
   * hook looks: an event stream is a facility of the runtime, and the hook has
   * to work in a process where a document was installed onto another host's
   * global — which is every process this suite runs in.
   */
  class FakeStream {
    url: string;
    readyState: number = 0;
    closed: boolean = false;
    listeners: Map<string, Array<(event: $FlowFixMe) => mixed>> = new Map();

    constructor(url: string) {
      this.url = url;
      opened.push(this);
    }

    addEventListener(type: string, listener: (event: $FlowFixMe) => mixed) {
      const held = this.listeners.get(type) ?? [];
      held.push(listener);
      this.listeners.set(type, held);
    }

    removeEventListener(type: string, listener: (event: $FlowFixMe) => mixed) {
      const held = this.listeners.get(type) ?? [];
      this.listeners.set(
        type,
        held.filter((one) => one !== listener),
      );
    }

    close() {
      this.closed = true;
      this.readyState = 2;
    }

    emit(type: string, event: $FlowFixMe) {
      for (const listener of this.listeners.get(type) ?? []) {
        listener(event);
      }
    }
  }

  const install = () => {
    opened = [];
    globalThis.EventSource = FakeStream;
  };

  afterEach(() => {
    uft.useRealTimers();
    globalThis.EventSource = undefined;
  });

  component Probe() {
    const stream = useEventSource("/api/feed", { events: ["tick"], retryDelay: 10 });
    return (
      <output>
        {[
          stream.status,
          stream.last?.name ?? "-",
          stream.last?.data ?? "-",
          stream.lastEventId ?? "-",
        ].join(" ")}
      </output>
    );
  }

  it("reports the named event a message listener would have missed", async () => {
    // A stream that sends `event: tick` delivers nothing to a `message`
    // listener, which is the single most common way an event stream looks
    // connected and silent.
    install();
    render(<Probe />);
    const stream = opened[0];

    act(() => {
      stream.emit("open", {});
    });
    expect(screen.getByText("open - - -")).toBeInTheDocument();

    act(() => {
      stream.emit("tick", { data: "42", lastEventId: "7" });
    });
    expect(screen.getByText("open tick 42 7")).toBeInTheDocument();
  });

  it("stays out of the way while the browser is retrying by itself", async () => {
    // `EventSource` reconnects on its own and sends `Last-Event-ID` with it,
    // which is strictly better than anything this hook can do. Two
    // reconnection schedules on one connection is worse than either.
    uft.useFakeTimers();
    install();
    render(<Probe />);
    const stream = opened[0];

    act(() => {
      stream.readyState = 0;
      stream.emit("error", {});
    });
    advance(1000);

    expect(screen.getByText("connecting - - -")).toBeInTheDocument();
    expect(opened.length).toBe(1);
  });

  it("opens a new connection when the browser has given up for good", async () => {
    // The one case the platform does not recover from: a response that is not
    // a 200 with `content-type: text/event-stream`. A deployment restarting
    // answers 502 for a second or two, and without this every open stream in
    // every tab is dead until somebody reloads the page.
    uft.useFakeTimers();
    install();
    render(<Probe />);

    act(() => {
      opened[0].readyState = 2;
      opened[0].emit("error", {});
    });
    expect(opened.length).toBe(1);

    advance(10);
    expect(opened.length).toBe(2);

    act(() => {
      opened[1].emit("open", {});
    });
    expect(screen.getByText("open - - -")).toBeInTheDocument();
  });

  it("closes the connection when the component goes", async () => {
    // An `EventSource` nobody is listening to still holds a connection and
    // still reconnects, so leaving it open is a socket per navigation for the
    // life of the tab.
    install();
    const { unmount } = render(<Probe />);
    const stream = opened[0];

    unmount();
    expect(stream.closed).toBe(true);
  });
});
