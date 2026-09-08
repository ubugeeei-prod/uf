// @flow
//
// `expect(container).toHaveNoAxeViolations()`.
//
// `@uniflowed/react-testing` renders into a real DOM and nothing was reading
// it, which is what ubugeeei-prod/uf#511 is about. This is the matcher over a
// rendered tree: an engine that finds real violations, a rule set that comes
// from `uf.config.js` rather than from each assertion, and a failure message a
// person can act on without a search engine.
//
// The audit is asynchronous — axe has no synchronous entry point — so every
// assertion here is awaited. That is the one thing about this matcher that is
// unlike the other forty-one, and it is stated in `Matchers` rather than left
// to be discovered.
//
// # What this cannot check
//
// Anything that needs layout. happy-dom computes no boxes, so `color-contrast`
// and the rules about overlapping targets are not decidable here and axe
// reports them as *incomplete* rather than as violations — which is why the
// engine's own "incomplete" list is not treated as a failure by the matcher.
// `uf test --browser` is where those become answerable, and `uf dev`'s audit
// answers them today because it runs in the browser.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it, uft } from "@uniflowed/test";
import { cleanup, render } from "@uniflowed/react-testing";

component Accessible() {
  return (
    <main>
      <h1>Report</h1>
      <img alt="a chart of nothing in particular" src="/chart.png" />
      <button type="button">Save</button>
    </main>
  );
}

// The two components below are deliberately inaccessible, and `uf lint`'s own
// static rules are right to report them: an `<img>` with no `alt` is an error
// and a heading level that jumps is a warning. They are broken on purpose
// because this file is about the *other* half of accessibility checking — the
// audit that reads a tree React actually rendered — and an audit needs
// something to find. Fixing the markup would delete the tests.
//
// Suppressed here rather than hidden in a fixture directory the linter does
// not read: a reader checking whether this file proves what it claims has to
// be able to see the markup beside the assertion about it. The block form is
// what JSX leaves available — a suppression comment has to be a `//` comment,
// and JSX children have no such thing — and it is closed immediately after the
// second component so nothing else in the file is covered by it.
//
// The two rule sets agreeing is the point rather than a conflict. `uf lint`
// reads source that was never rendered; axe reads what the render produced.
// Neither subsumes the other, which is why uf has both.
//
// uf-lint-disable a11y/alt-text, a11y/heading-order
component SkippedHeading() {
  // `heading-order` is `moderate`, which is what makes this tree the one the
  // floor can be measured against: a floor at `serious` has something to drop
  // and a floor at `moderate` has something to keep.
  return (
    <main>
      <h1>Report</h1>
      <h3>Detail</h3>
    </main>
  );
}

component Inaccessible() {
  return (
    <main>
      <h1>Report</h1>
      {/*
        No `alt`, so axe's `image-alt` fires; no text, so `button-name` does.
        Both are `critical`, which is what the floor cases below need.
      */}
      <img src="/chart.png" />
      <button type="button" />
    </main>
  );
}
// uf-lint-enable a11y/alt-text, a11y/heading-order

describe("toHaveNoAxeViolations", () => {
  // The rule set lives in the environment, and `uft.stubEnv` is undone between
  // *files* rather than between cases. Without this, the case below that narrows
  // the project's rules narrows them for every case after it — which is exactly
  // the leak `uf test`'s worker exists to prevent one level up, arriving one
  // level down.
  afterEach(() => {
    uft.unstubAllEnvs();
    // And the tree goes with the run. `render` clears its own containers on the
    // way *in*, so a file that never calls this leaves its last one in
    // `document.body` for whichever file the scheduler puts next on this
    // worker — ubugeeei-prod/uf#607, which is not a thing to add another
    // instance of.
    cleanup();
  });

  it("passes over a tree axe has nothing to say about", async () => {
    const { container } = render(<Accessible />);
    await expect(container).toHaveNoAxeViolations();
  });

  it("fails over a tree axe has something to say about", async () => {
    const { container } = render(<Inaccessible />);
    let thrown: mixed = null;
    try {
      await expect(container).toHaveNoAxeViolations();
    } catch (error) {
      thrown = error;
    }
    const message = String((thrown as $FlowFixMe)?.message ?? "");
    // The rule id, so the reader can look it up; the count, so they know how
    // much there is; and Deque's page, which is the part that turns an id into
    // something actionable.
    expect(message).toContain("image-alt");
    expect(message).toContain("button-name");
    expect(message).toContain("dequeuniversity.com");
    expect(String((thrown as $FlowFixMe)?.name)).toBe("AssertionError");
  });

  it("negates, and says what a passing audit did and did not establish", async () => {
    const { container } = render(<Inaccessible />);
    await expect(container).not.toHaveNoAxeViolations();

    const clean = render(<Accessible />);
    let thrown: mixed = null;
    try {
      await expect(clean.container).not.toHaveNoAxeViolations();
    } catch (error) {
      thrown = error;
    }
    expect(String((thrown as $FlowFixMe)?.message)).toContain("reported none");
  });

  it("takes the rule set from the project, and lets one assertion narrow it", async () => {
    const { container } = render(<Inaccessible />);

    // The project's answer, delivered to every worker in `UF_AXE` by
    // `HostCommand::with_axe`. Stubbed here rather than asserted through a
    // second `uf test` process: what this needs to establish is that the
    // matcher reads it, and the environment is where it reads it from.
    uft.stubEnv("UF_AXE", JSON.stringify({ disabledRules: ["image-alt", "button-name"] }));
    await expect(container).toHaveNoAxeViolations();

    // And a call may narrow further. The override replaces the project's list
    // rather than adding to it, so `button-name` comes back and the audit
    // fails again — which is the behaviour a test that wants *one* rule
    // checked depends on.
    let thrown: mixed = null;
    try {
      await expect(container).toHaveNoAxeViolations({ disabledRules: ["image-alt"] });
    } catch (error) {
      thrown = error;
    }
    const message = String((thrown as $FlowFixMe)?.message ?? "");
    expect(message).toContain("button-name");
    expect(message).not.toContain("image-alt");
  });

  it("drops findings below the configured floor, and keeps the ones at it", async () => {
    const { container } = render(<SkippedHeading />);
    // The only thing wrong with this tree is `moderate`.
    await expect(container).not.toHaveNoAxeViolations({ minImpact: "moderate" });
    await expect(container).toHaveNoAxeViolations({ minImpact: "serious" });

    // And a floor set once, for the project, does the same thing.
    uft.stubEnv("UF_AXE", JSON.stringify({ minImpact: "serious" }));
    await expect(container).toHaveNoAxeViolations();
  });

  it("keeps a critical finding, whatever the floor is", async () => {
    // The other end of the scale: `critical` is the top of it, so a project
    // that raised the floor as far as it goes still hears about these.
    const { container } = render(<Inaccessible />);
    await expect(container).not.toHaveNoAxeViolations({ minImpact: "critical" });
  });

  it("refuses what is not an element rather than failing inside axe", async () => {
    let thrown: mixed = null;
    try {
      await expect("<img>").toHaveNoAxeViolations();
    } catch (error) {
      thrown = error;
    }
    const error: $FlowFixMe = thrown;
    expect(String(error?.message)).toContain("needs an element");
    // All four fields, which is what #511's matcher turned up as missing on
    // every element matcher: a one-argument `AssertionError` reported
    // `undefined` for both sides of the comparison.
    expect(error?.matcher).toBe("toHaveNoAxeViolations");
    expect(error?.expected).toBe("an element");
  });
});
