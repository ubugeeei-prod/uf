// @flow
//
// `@uniflowed/react-testing`.
//
// A DOM, a real React root, and the queries a test actually reaches for.

import * as React from "@uniflowed/react";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { spawn } from "node:child_process";
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

  it("tells a column header and a row header apart by their scope", () => {
    render(
      <table>
        <thead>
          <tr>
            <th scope="col">Name</th>
            <th scope="col">Born</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <th scope="row">Ada Lovelace</th>
            <td>1815</td>
          </tr>
        </tbody>
      </table>,
    );
    // Every `<th>` used to be a `columnheader`, so `getByRole("rowheader")`
    // found nothing in a table of records — where every row has one, and where
    // it is the cell that makes a screen reader say "Ada Lovelace, 1815"
    // rather than "1815".
    expect(screen.getAllByRole("columnheader").length).toBe(2);
    expect(screen.getByRole("rowheader").textContent).toBe("Ada Lovelace");
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

describe("a role query and the accessibility tree", () => {
  it("leaves out what the hidden attribute takes out of the tree", () => {
    render(
      <div>
        <section aria-label="open">shown</section>
        <section aria-label="closed" hidden>
          gone
        </section>
      </div>,
    );
    expect(
      screen.getAllByRole("region").map((region) => region.getAttribute("aria-label")),
    ).toEqual(["open"]);
  });

  it("hands the hidden one back to a test that says it means the document", () => {
    render(
      <section aria-label="closed" hidden>
        gone
      </section>,
    );
    expect(screen.queryByRole("region")).toBe(null);
    expect(screen.getByRole("region", { hidden: true })).toBeInTheDocument();
  });

  it('reads hidden="until-found" as hidden, which is what a closed panel uses', () => {
    // The case from #323: an accordion keeps its closed panels in the document
    // so that find-in-page can reach the prose in them, and a reader is still
    // not told about them — `content-visibility: hidden` is not in the
    // accessibility tree, and being findable is not being announced.
    //
    // Set on the element rather than written in the JSX, because React cannot
    // say it: `hidden` is one of its boolean attributes, so the string is
    // truthy and truthy is all it keeps. `packages/ui`'s `useUntilFound`
    // upgrades the attribute in an effect for exactly this reason, so this is
    // the shape the DOM is really in.
    const { container } = render(<section aria-label="panel">gone</section>);
    const panel: $FlowFixMe = container.querySelector("section");
    panel.setAttribute("hidden", "until-found");

    expect(screen.queryByRole("region")).toBe(null);
    expect(screen.getByRole("region", { hidden: true })).toBe(panel);
  });

  it("leaves out an aria-hidden subtree, which is the page behind a dialog", () => {
    render(
      <div>
        <div aria-hidden="true">
          <button type="button">behind</button>
        </div>
        <div role="dialog">
          <button type="button">in front</button>
        </div>
      </div>,
    );
    expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual(["in front"]);
  });

  it("leaves out what a display:none ancestor hides", () => {
    render(
      <div style={{ display: "none" }}>
        <button type="button">press</button>
      </div>,
    );
    expect(screen.queryByRole("button")).toBe(null);
  });

  it("keeps a closed details' summary and leaves out the rest of it", () => {
    component Disclosure(open: boolean) {
      return (
        <details open={open}>
          <summary>
            <span aria-label="chevron" role="img" />
            more
          </summary>
          <button type="button">inside</button>
        </details>
      );
    }
    render(<Disclosure open={false} />);
    // The summary is what a closed disclosure renders, so it is the one part
    // still announced — a rule about the ancestor alone would lose it too.
    expect(screen.getByRole("img", { name: "chevron" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "inside" })).toBe(null);

    cleanup();
    render(<Disclosure open={true} />);
    expect(screen.getByRole("button", { name: "inside" })).toBeInTheDocument();
  });

  it("finds a heading by its level, and not one at another level", () => {
    render(
      <div>
        <h2>Two</h2>
        <h3>Three</h3>
      </div>,
    );
    // The assertion #323 was filed for: this used to return every heading
    // whichever level was asked for, so it passed against the `<h2>`.
    expect(screen.getByRole("heading", { level: 3 }).textContent).toBe("Three");
    expect(screen.getByRole("heading", { level: 2 }).textContent).toBe("Two");
    expect(screen.queryByRole("heading", { level: 4 })).toBe(null);
  });

  it("takes the level from aria-level, which overrides the tag", () => {
    render(<h2 aria-level={4}>Four</h2>);
    expect(screen.getByRole("heading", { level: 4 })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 2 })).toBe(null);
  });

  it("reads a role=heading with no aria-level as level two, as a browser does", () => {
    render(<div role="heading">titled</div>);
    expect(screen.getByRole("heading", { level: 2 }).textContent).toBe("titled");
  });

  it("refuses a level on a role that has none, rather than ignoring it", () => {
    render(<button type="button">Save</button>);
    let message = "";
    try {
      screen.getByRole("button", { level: 3 });
    } catch (error) {
      message = String(error);
    }
    expect(message).toContain("a level narrows a heading");
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

describe("the document's classes", () => {
  it("installs every class the package narrows an element with", () => {
    // A query decides what an element *is* with `instanceof` — which is why
    // `internal/dom.js` installs the document's own classes as the global ones
    // — and an `instanceof` against a class that was never installed is not
    // `false`. It is `ReferenceError: HTMLFieldSetElement is not defined`,
    // thrown from a line about clicking a tab, in a file with no fieldset
    // anywhere in it. That is how `HTMLFieldSetElement` was found missing from
    // the install list, while `userEvent.click` was asking it whether a
    // control is disabled.
    //
    // So the install list and the narrowings are checked against each other
    // here, by asking the question the package asks, of elements a render
    // actually produced.
    const { container } = render(
      <fieldset>
        <input defaultValue="a" />
        <textarea defaultValue="b" />
        <select defaultValue="c">
          <option value="c">C</option>
        </select>
        <button type="button">press</button>
      </fieldset>,
    );

    expect(container.querySelector("input") instanceof HTMLInputElement).toBe(true);
    expect(container.querySelector("textarea") instanceof HTMLTextAreaElement).toBe(true);
    expect(container.querySelector("select") instanceof HTMLSelectElement).toBe(true);
    expect(container.querySelector("button") instanceof HTMLButtonElement).toBe(true);
    expect(container.querySelector("fieldset") instanceof HTMLFieldSetElement).toBe(true);
    expect(container.querySelector("button") instanceof HTMLElement).toBe(true);
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

// Where a failing query is reported, driven through a real worker.
//
// `uf test` prints a failure as `path:line:column`, and it takes the path from
// the file it asked the worker to run — the number is all the worker sends. So
// the two halves can disagree, and for every `getBy…` they did: the error is
// built inside `@uniflowed/react-testing`, whose frames the runner did not
// trim, and the first frame of `internal/queries.js` was reported under the
// test file's path. A nine-line file was told to look at line 369
// (ubugeeei-prod/uf#319).
//
// Nothing inside a test can see this. The position belongs to the protocol
// between the worker and `uf`, and by the time a case could look at its own
// report the case is the thing being reported. So this drives a worker the way
// `crates/uf_test/src/host.rs` does — one request on stdin, one JSON event per
// line back — and reads the `site` off the failures, exactly as
// `output-owner.test.js` reads the `test` field off `output` events, and for
// the same reason: a promise made on a wire is checked on that wire.
//
// The fixture is written to a temporary directory rather than kept beside this
// file, because a file in this workspace that registers cases *is* a test of
// this repository and would be collected by the very suite meant to run it.
// Nothing out there has a `node_modules` to resolve `@uniflowed/…` from, so it
// reaches the packages by path; `createElement` rather than JSX for the same
// reason, since the transform's JSX import is a bare specifier.
const here = path.dirname(fileURLToPath(import.meta.url));
const repository = path.resolve(here, "..", "..");

function entry(name: string): string {
  return pathToFileURL(path.join(repository, "packages", name, "index.js")).href;
}

const REPRODUCTION = `import { createElement } from "${entry("react")}";
import { describe, it } from "${entry("test")}";
import { render, screen } from "${entry("react-testing")}";

describe("a query that finds nothing", () => {
  it("reports the line it was called on", () => {
    render(createElement("p", null, "hello"));
    screen.getByRole("banner");
  });

  it("reports the line it was called on after waiting", async () => {
    render(createElement("p", null, "hello"));
    await screen.findByRole("banner");
  });
});
`;

/** One line the worker wrote back, in the shape `host.rs` reads. */
type Event = {
  event: string,
  name?: string,
  status?: string,
  message?: string,
  site?: {| line: number, column: number |} | null,
};

/**
 * How this host starts a worker, mirroring `HostCommand::with_flow_loader`.
 *
 * The worker imports Flow — `@uniflowed/test` is Flow source — so it needs the
 * host's loader. Deno has none in `@uniflowed/host` yet, so it cannot run this
 * at all; a named failure is better than a skip that reads like a pass.
 */
function loaderArguments(): Array<string> {
  const host = path.basename(process.execPath);
  if (host.startsWith("node")) {
    const register = path.join(repository, "packages", "host", "register.js");
    return ["--enable-source-maps", "--import", pathToFileURL(register).href];
  }
  if (host.startsWith("bun")) {
    return ["--preload", path.join(repository, "packages", "host", "bun-preload.js")];
  }
  throw new Error(`no Flow loader for ${host}: this test drives the worker uf would have started`);
}

/**
 * Run the reproduction in a worker of its own and collect every event it wrote.
 *
 * The fixture is written to a temporary directory rather than kept beside this
 * file, because a file in this workspace that registers cases *is* a test of
 * this repository: `uf test` discovers by reading a file rather than by naming
 * it, so a fixture here would be collected and run by the very suite that is
 * supposed to be running it.
 */
function runInWorker(): Promise<Array<Event>> {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "uf-query-site-"));
  const file = path.join(directory, "lines.test.js");
  fs.writeFileSync(file, REPRODUCTION);

  return new Promise((resolve, reject) => {
    const worker = path.join(repository, "packages", "test", "worker.js");
    const child = spawn(process.execPath, [...loaderArguments(), worker], {
      stdio: ["pipe", "pipe", "inherit"],
    });
    let written = "";
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      written += String(chunk);
    });
    child.on("error", reject);
    child.on("close", () => {
      fs.rmSync(directory, { recursive: true, force: true });
      try {
        resolve(
          written
            .split("\n")
            .filter((line) => line !== "")
            .map((line) => JSON.parse(line)),
        );
      } catch (error) {
        reject(new Error(`the worker wrote something that is not an event: ${String(error)}`));
      }
    });
    // Generous, because the second case waits a second before it gives up and
    // a loaded machine must not turn that into a timeout instead.
    child.stdin.end(`${JSON.stringify({ file, timeoutMs: 20000 })}\n`);
  });
}

let started: Promise<Array<Event>> | null = null;

/**
 * The run, started on first use and shared by the cases that read it.
 *
 * Not a `beforeAll`: a hook is charged against the budget of the case it runs
 * before, and a whole worker — a Node.js start, the Flow loader, React, a DOM
 * and a second of waiting — does not fit in one case's five. Awaited by each
 * case instead, which is where the longer budget can be asked for.
 */
function reports(): Promise<Array<Event>> {
  if (started == null) {
    started = runInWorker();
  }
  return started;
}

/** The one-based line of the fixture that contains `text`. */
function lineContaining(text: string): number {
  const index = REPRODUCTION.split("\n").findIndex((line) => line.includes(text));
  if (index < 0) {
    throw new Error(`the fixture has no line containing ${JSON.stringify(text)}`);
  }
  return index + 1;
}

/** The failure reported for the case whose name ends in `ending`. */
function failureOf(events: Array<Event>, ending: string): Event {
  const found = events.filter(
    (event) => event.event === "test" && String(event.name).endsWith(ending),
  );
  if (found.length !== 1) {
    throw new Error(`expected one report for ${ending} and the worker sent ${found.length}`);
  }
  return found[0];
}

/** How long one of these cases may take, worker and all. */
const WORKER_BUDGET = { timeout: 30000 };

describe("where a failing query is reported", () => {
  it(
    "names the line of the test that called it",
    async () => {
      const failed = failureOf(await reports(), "reports the line it was called on");
      expect(failed.status).toBe("failed");
      expect(failed.message).toContain("getByRole");
      // The whole of the bug in one assertion: the number is stamped onto the
      // fixture's path, so it has to be a line of the fixture — and the right
      // one. It used to be 369, the line where `queryFailure` builds the error.
      expect(failed.site?.line).toBe(lineContaining('screen.getByRole("banner")'));
    },
    WORKER_BUDGET,
  );

  it(
    "names the line of the test that waited, not the poll that gave up",
    async () => {
      const failed = failureOf(await reports(), "reports the line it was called on after waiting");
      expect(failed.status).toBe("failed");
      expect(failed.message).toContain("findByRole");
      // A wait keeps the last attempt's failure, and the last attempt runs
      // from a timer with none of the test underneath it. Trimming the
      // library's frames alone would leave this one with no position at all,
      // which is why `findBy…` carries its call site in.
      expect(failed.site?.line).toBe(lineContaining('screen.findByRole("banner")'));
    },
    WORKER_BUDGET,
  );

  it(
    "never names a line the file does not have",
    async () => {
      const lines = REPRODUCTION.split("\n").length;
      for (const event of await reports()) {
        if (event.event !== "test" || event.status !== "failed") {
          continue;
        }
        const line = event.site?.line;
        expect(line == null || (line >= 1 && line <= lines)).toBe(true);
      }
    },
    WORKER_BUDGET,
  );
});