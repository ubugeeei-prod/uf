// @flow
//
// `@uniflowed/react-testing`.
//
// A DOM, a real React root, and the queries a test actually reaches for.

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { describe, expect, it } from "@uniflowed/test";
import {
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
