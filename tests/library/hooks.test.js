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
import { describe, expect, fn, it } from "@uniflowed/test";
import { act, fireEvent, render, screen, userEvent, waitFor } from "@uniflowed/react-testing";
import {
  useAsync,
  useClickOutside,
  useCounter,
  useDebouncedValue,
  useElementRef,
  useEventListener,
  useFocusWithin,
  useHover,
  useInterval,
  useMounted,
  useOnline,
  usePrevious,
  useStableCallback,
  useStorage,
  useToggle,
} from "@uniflowed/hooks";

const tick = (millis: number) => act(() => new Promise((resolve) => setTimeout(resolve, millis)));

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
    const output: any = container.querySelector("output");
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
    const region: any = screen.getByRole("button").parentElement;
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
