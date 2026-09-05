// @flow
//
// `@uniflowed/react-testing`.
//
// A DOM, a real React root, and the queries a test actually reaches for.

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { describe, expect, it } from "@uniflowed/test";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  userEvent,
  waitFor,
  within,
} from "@uniflowed/react-testing";

component Greeting(name: string) {
  return <p>Hello, {name}</p>;
}

component Counter() {
  const [count, setCount] = useState(0);
  return (
    <div>
      <output>count: {count}</output>
      <button type="button" onClick={() => setCount(count + 1)}>
        increment
      </button>
    </div>
  );
}

describe("render", () => {
  it("mounts a component into a document", () => {
    const { container } = render(<Greeting name="world" />);
    expect(container.textContent).toBe("Hello, world");
  });

  it("renders again into the same container", () => {
    const { rerender, container } = render(<Greeting name="first" />);
    rerender(<Greeting name="second" />);
    expect(container.textContent).toBe("Hello, second");
  });

  it("takes the tree down on unmount", () => {
    const { unmount, container } = render(<Greeting name="world" />);
    unmount();
    expect(container.textContent).toBe("");
  });

  it("leaves nothing behind for the next render to find", () => {
    render(<Greeting name="first" />);
    render(<Greeting name="second" />);
    // Without cleanup between renders both would be in the document and
    // "exactly one paragraph" would be false for reasons unrelated to the test.
    expect(screen.getAllByText(/Hello/).length).toBe(1);
  });

  it("hands back the markup for a failure message", () => {
    const { asFragment } = render(<Greeting name="world" />);
    expect(asFragment()).toContain("<p>Hello, ");
  });
});

describe("queries", () => {
  it("finds by text, ignoring how the JSX was indented", () => {
    render(<p>Save all of your changes</p>);
    expect(screen.getByText("Save all of your changes")).toBeTruthy();
  });

  it("matches a substring when asked", () => {
    render(<p>Save all of your changes</p>);
    expect(screen.getByText("save all", { exact: false })).toBeTruthy();
  });

  it("finds the element closest to the text, not every ancestor", () => {
    render(
      <div>
        <section>
          <button type="button">Save</button>
        </section>
      </div>,
    );
    expect(screen.getByText("Save").tagName).toBe("BUTTON");
  });

  it("finds by an implicit role", () => {
    render(<button type="button">Save</button>);
    expect(screen.getByRole("button").textContent).toBe("Save");
  });

  it("finds by role and accessible name together", () => {
    render(
      <div>
        <button type="button">Save</button>
        <button type="button">Cancel</button>
      </div>,
    );
    expect(screen.getByRole("button", { name: "Cancel" }).textContent).toBe("Cancel");
  });

  it("reads a name from aria-label", () => {
    render(
      <button type="button" aria-label="Close the dialog">
        ×
      </button>,
    );
    expect(screen.getByRole("button", { name: "Close the dialog" })).toBeTruthy();
  });

  it("finds a control by the label pointing at it", () => {
    render(
      <div>
        <label htmlFor="email">Email address</label>
        <input id="email" type="email" />
      </div>,
    );
    expect(screen.getByLabelText("Email address").getAttribute("type")).toBe("email");
  });

  it("finds a control nested inside its label", () => {
    render(
      <label>
        Remember me
        <input type="checkbox" />
      </label>,
    );
    expect(screen.getByLabelText("Remember me").getAttribute("type")).toBe("checkbox");
  });

  it("finds by placeholder and by test id", () => {
    render(
      <div>
        <input placeholder="Search" />
        <span data-testid="marker">here</span>
      </div>,
    );
    expect(screen.getByPlaceholderText("Search")).toBeTruthy();
    expect(screen.getByTestId("marker").textContent).toBe("here");
  });

  it("finds a control by what it is displaying", () => {
    render(<input defaultValue="hello" />);
    expect(screen.getByDisplayValue("hello")).toBeTruthy();
  });

  it("returns null rather than raising when asked about absence", () => {
    render(<p>present</p>);
    expect(screen.queryByText("absent")).toBe(null);
  });

  it("says how many it found when it needed one", () => {
    render(
      <div>
        <button type="button">Save</button>
        <button type="button">Save</button>
      </div>,
    );
    let message = "";
    try {
      screen.getByText("Save");
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("found 2 elements");
  });

  it("says it found nothing, and shows the markup", () => {
    render(<p>present</p>);
    let message = "";
    try {
      screen.getByText("absent");
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("found nothing");
    expect(message).toContain("present");
  });

  it("restricts a query to one subtree with within", () => {
    render(
      <div>
        <section data-testid="first">
          <button type="button">Save</button>
        </section>
        <section data-testid="second">
          <button type="button">Save</button>
        </section>
      </div>,
    );
    const second = screen.getByTestId("second");
    expect(within(second).getAllByText("Save").length).toBe(1);
  });
});

describe("fireEvent", () => {
  it("dispatches an event React hears", () => {
    render(<Counter />);
    fireEvent.click(screen.getByRole("button"));
    expect(screen.getByText("count: 1")).toBeTruthy();
  });

  it("reports that a handler prevented the default", () => {
    component Form() {
      return (
        <form onSubmit={(event) => event.preventDefault()}>
          <button type="submit">Send</button>
        </form>
      );
    }
    const { container } = render(<Form />);
    const form: any = container.querySelector("form");
    expect(fireEvent.submit(form)).toBe(false);
  });

  it("reaches onFocus and onBlur, which React hears as focusin and focusout", () => {
    // React attaches its listeners to the root container, so it only hears
    // events that bubble — and `focus` does not. Dispatching a bare `focus`
    // used to call nothing at all, and a test written against it read as a
    // component that ignored focus rather than as an event that never arrived.
    component Field() {
      const [state, setState] = useState("idle");
      return (
        <input
          aria-label="name"
          onFocus={() => setState("focused")}
          onBlur={() => setState("blurred")}
          value={state}
          readOnly={true}
        />
      );
    }
    render(<Field />);
    const field = screen.getByLabelText("name") as HTMLInputElement;

    fireEvent.focus(field);
    expect(field.value).toBe("focused");

    fireEvent.blur(field);
    expect(field.value).toBe("blurred");
  });

  it("still sends the non-bubbling event itself", () => {
    // Anything listening on the element directly — which is what a `focus`
    // listener has to do — must still hear it.
    const { container } = render(<input aria-label="name" />);
    const field = container.querySelector("input") as HTMLInputElement;
    const heard: Array<string> = [];
    field.addEventListener("focus", () => heard.push("focus"));
    field.addEventListener("focusin", () => heard.push("focusin"));

    fireEvent.focus(field);

    expect(heard).toEqual(["focus", "focusin"]);
  });
});

describe("userEvent", () => {
  it("clicks", async () => {
    render(<Counter />);
    await userEvent.click(screen.getByRole("button"));
    expect(screen.getByText("count: 1")).toBeTruthy();
  });

  it("produces the events a real click produces, in order", async () => {
    const seen = [];
    component Probe() {
      return (
        <button
          type="button"
          onPointerDown={() => seen.push("pointerdown")}
          onMouseDown={() => seen.push("mousedown")}
          onMouseUp={() => seen.push("mouseup")}
          onClick={() => seen.push("click")}
        >
          press
        </button>
      );
    }
    render(<Probe />);
    await userEvent.click(screen.getByRole("button"));
    expect(seen).toEqual(["pointerdown", "mousedown", "mouseup", "click"]);
  });

  it("does not click a disabled control", async () => {
    let clicks = 0;
    render(
      <button type="button" disabled onClick={() => (clicks += 1)}>
        press
      </button>,
    );
    await userEvent.click(screen.getByRole("button"));
    expect(clicks).toBe(0);
  });

  it("types one character at a time, so every keystroke is seen", async () => {
    const seen = [];
    component Field() {
      const [value, setValue] = useState("");
      return (
        <input
          value={value}
          onChange={(event) => {
            seen.push(event.target.value);
            setValue(event.target.value);
          }}
        />
      );
    }
    render(<Field />);
    await userEvent.type(screen.getByRole("textbox"), "abc");
    expect(seen).toEqual(["a", "ab", "abc"]);
  });

  it("clears a control", async () => {
    render(<input defaultValue="something" />);
    const input: any = screen.getByRole("textbox");
    await userEvent.clear(input);
    expect(input.value).toBe("");
  });

  it("presses a named key at whatever has focus", async () => {
    const seen = [];
    render(
      <input
        onKeyDown={(event) => {
          seen.push(event.key);
        }}
      />,
    );
    await userEvent.click(screen.getByRole("textbox"));
    await userEvent.keyboard("{Escape}");
    expect(seen).toEqual(["Escape"]);
  });

  it("moves focus with tab, in document order", async () => {
    render(
      <div>
        <button type="button">first</button>
        <button type="button">second</button>
      </div>,
    );
    await userEvent.tab();
    expect(document.activeElement?.textContent).toBe("first");
    await userEvent.tab();
    expect(document.activeElement?.textContent).toBe("second");
  });

  it("chooses an option in a select", async () => {
    render(
      <select defaultValue="a">
        <option value="a">A</option>
        <option value="b">B</option>
      </select>,
    );
    const select: any = screen.getByRole("combobox");
    await userEvent.selectOptions(select, "b");
    expect(select.value).toBe("b");
  });
});

describe("waitFor and findBy", () => {
  component Later() {
    const [ready, setReady] = useState(false);
    React.useEffect(() => {
      const id = setTimeout(() => setReady(true), 25);
      return () => clearTimeout(id);
    }, []);
    return <p>{ready ? "ready" : "loading"}</p>;
  }

  it("waits for something that has not happened yet", async () => {
    render(<Later />);
    expect(screen.getByText("loading")).toBeTruthy();
    await expect(screen.findByText("ready")).resolves.toBeTruthy();
  });

  it("waits for an assertion to stop failing", async () => {
    render(<Later />);
    await waitFor(() => {
      expect(screen.getByText("ready")).toBeTruthy();
    });
  });

  it("gives up, keeping the last failure", async () => {
    render(<Greeting name="world" />);
    let message = "";
    try {
      await waitFor(() => screen.getByText("never"), { timeout: 60, interval: 10 });
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("found nothing");
  });
});

describe("cleanup", () => {
  it("empties the document", () => {
    render(<Greeting name="world" />);
    cleanup();
    expect(screen.queryByText(/Hello/)).toBe(null);
  });
});

describe("element matchers", () => {
  it("asserts an element is in the document", () => {
    // Removed by React rather than by the test: taking a node out from under
    // React leaves it holding a reference to a child of nothing, and its own
    // unmount then fails on a node that is no longer there.
    const { container, unmount } = render(<Greeting name="world" />);
    const paragraph: any = container.querySelector("p");
    expect(paragraph).toBeInTheDocument();
    unmount();
    expect(paragraph).not.toBeInTheDocument();
  });

  it("treats hidden and aria-hidden as invisible", () => {
    render(
      <div>
        <p>shown</p>
        <p hidden>concealed</p>
        <p aria-hidden="true">ignored</p>
      </div>,
    );
    expect(screen.getByText("shown")).toBeVisible();
    expect(screen.getByText("concealed")).not.toBeVisible();
    expect(screen.getByText("ignored")).not.toBeVisible();
  });

  it("sees a disabled control, and one inside a disabled fieldset", () => {
    render(
      <form>
        <button type="button" disabled>
          off
        </button>
        <fieldset disabled>
          <input aria-label="inner" />
        </fieldset>
        <button type="button">on</button>
      </form>,
    );
    expect(screen.getByRole("button", { name: "off" })).toBeDisabled();
    expect(screen.getByLabelText("inner")).toBeDisabled();
    expect(screen.getByRole("button", { name: "on" })).toBeEnabled();
  });

  it("reads checkedness from the property and from aria", () => {
    render(
      <div>
        <input aria-label="box" type="checkbox" defaultChecked />
        <div role="switch" aria-checked="false" aria-label="toggle" />
      </div>,
    );
    expect(screen.getByLabelText("box")).toBeChecked();
    expect(screen.getByLabelText("toggle")).not.toBeChecked();
  });

  it("asserts attributes, classes, text and value", () => {
    render(
      <input aria-label="field" className="a b" defaultValue="typed" placeholder="hint" required />,
    );
    const field = screen.getByLabelText("field");
    expect(field).toHaveAttribute("placeholder");
    expect(field).toHaveAttribute("placeholder", "hint");
    expect(field).toHaveClass("a", "b");
    expect(field).not.toHaveClass("c");
    expect(field).toHaveValue("typed");
    expect(field).toBeRequired();
  });

  it("collapses whitespace before comparing text", () => {
    render(<p>Save all of your changes</p>);
    expect(screen.getByText(/Save/)).toHaveTextContent("Save all of your changes");
  });

  it("asserts focus", async () => {
    render(<button type="button">press</button>);
    const button = screen.getByRole("button");
    expect(button).not.toHaveFocus();
    await userEvent.click(button);
    expect(button).toHaveFocus();
  });

  it("says what it received when it is not an element", () => {
    let message = "";
    try {
      expect("a string").toBeVisible();
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("needs an element");
  });
});

describe("the act environment", () => {
  it("renders without React warning that the environment is not configured", () => {
    // React prints "The current testing environment is not configured to
    // support act(...)" through `console.error` unless the harness sets
    // `IS_REACT_ACT_ENVIRONMENT`. Every render here goes through `act`, so
    // the warning arrived once per assertion and drowned the ones worth
    // reading.
    const original = console.error;
    const said: Array<string> = [];
    console.error = (...args: $ReadOnlyArray<mixed>) => {
      said.push(args.map((value) => String(value)).join(" "));
    };
    try {
      render(<Greeting name="world" />);
      fireEvent.click(screen.getByText(/Hello/));
    } finally {
      console.error = original;
    }

    expect(said.filter((line) => line.includes("act("))).toEqual([]);
  });

  it("says the process is a test when act is reached without a render", () => {
    // Nothing installs the flag when the package is imported — a test file
    // that renders nothing should not be made to build a document — so `act`
    // has to install it itself. A hook test whose first act is a timer firing
    // reaches `act` without going through `render`, and it must not be the
    // one call in the file that warns.
    //
    // Asserted through `act` rather than by reading the flag directly,
    // because reading it says only that some earlier test in this file
    // rendered.
    const original = console.error;
    const said: Array<string> = [];
    console.error = (...args: $ReadOnlyArray<mixed>) => {
      said.push(args.map((value) => String(value)).join(" "));
    };
    try {
      act(() => {});
    } finally {
      console.error = original;
    }

    expect(globalThis.IS_REACT_ACT_ENVIRONMENT).toBe(true);
    expect(said.filter((line) => line.includes("act("))).toEqual([]);
  });

  it("keeps its scope open until an async body settles", async () => {
    component Late() {
      const [text, setText] = useState("waiting");
      React.useEffect(() => {
        const id = setTimeout(() => setText("arrived"), 20);
        return () => clearTimeout(id);
      }, []);
      return <output>{text}</output>;
    }

    render(<Late />);
    expect(screen.getByText("waiting")).toBeInTheDocument();

    await act(() => new Promise((resolve) => setTimeout(resolve, 40)));

    // No `waitFor` here on purpose. The scope covered the timer, so the update
    // it caused is flushed by the time `act` returns.
    //
    // `act` hands back a bare thenable rather than a promise, and chaining off
    // its `then` produced an `undefined` that `await` resolved at once: this
    // line ran before the timer had fired, and — worse — every later `act` in
    // the process nested inside a scope that had been left open and flushed
    // nothing at all.
    expect(screen.getByText("arrived")).toBeInTheDocument();
  });

  it("stands the environment down for the length of a wait, and puts it back", async () => {
    // `act` holds updates until its scope closes, so a wait cannot happen
    // inside one — the loop would poll a tree that cannot change. The
    // environment is stood down instead, and the update the test is waiting
    // for is allowed to arrive unacted.
    render(<Greeting name="world" />);
    expect(globalThis.IS_REACT_ACT_ENVIRONMENT).toBe(true);

    let insideWait = null;
    let insideOuterWait = null;
    await waitFor(async () => {
      // A query, because every query installs the DOM and with it the act
      // environment. Deciding whether to install by reading the current value
      // turned the environment back on here, and only the first poll of a
      // wait was quiet.
      screen.getByText(/Hello/);
      await waitFor(() => {
        insideWait = globalThis.IS_REACT_ACT_ENVIRONMENT;
      });
      // The inner wait has returned; the outer one has not, so the
      // environment is still down. Every `findBy…` is a `waitFor`, so this
      // nesting is what a test does by accident rather than on purpose.
      insideOuterWait = globalThis.IS_REACT_ACT_ENVIRONMENT;
    });

    expect(insideWait).toBe(false);
    expect(insideOuterWait).toBe(false);
    expect(globalThis.IS_REACT_ACT_ENVIRONMENT).toBe(true);
  });
});
