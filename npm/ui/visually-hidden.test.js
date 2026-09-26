// @flow
//
// `visually-hidden.js`: the `VisuallyHidden` part, and `announce()` — one pair
// of live regions for the whole document, React Aria's LiveAnnouncer shape.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it, uft } from "@uniflowed/test";
import { act, cleanup, fireEvent, render, screen } from "@uniflowed/react-testing";

import { Dialog, ListBox, VisuallyHidden, announce, clearAnnouncements } from "./index.js";

afterEach(() => {
  cleanup();
  clearAnnouncements();
  uft.useRealTimers();
});

function htmlOf(element: Element): HTMLElement {
  if (!(element instanceof HTMLElement)) throw new Error("expected an HTML element");
  return element;
}

/** The announcer's region for one politeness, or a failure saying there is none. */
function regionFor(politeness: "polite" | "assertive"): HTMLElement {
  const region = document.querySelector(`[data-uf-live-announcer] [aria-live="${politeness}"]`);
  if (!(region instanceof HTMLElement)) throw new Error(`no ${politeness} region`);
  return region;
}

/** What a region currently holds, one entry per message. */
function messages(politeness: "polite" | "assertive"): $ReadOnlyArray<string> {
  return Array.from(regionFor(politeness).children).map((node) => node.textContent);
}

describe("announce", () => {
  // `announce` does nothing without a document, and the test DOM is installed
  // by the first `render`.
  function withDocument(): void {
    render(<span />);
  }

  it("puts each message in a log region that was in the document before it", () => {
    withDocument();
    uft.useFakeTimers();
    // Take any announcer an earlier test left, so this one creates it.
    document.querySelector("[data-uf-live-announcer]")?.remove();
    announce("3 results");
    const polite = regionFor("polite");
    expect(polite.getAttribute("role")).toBe("log");
    expect(polite.getAttribute("aria-relevant")).toBe("additions");
    // A region and its first message inserted together are silent, so the
    // message waits for the region to settle.
    expect(messages("polite")).toEqual([]);
    uft.advanceTimersByTime(100);
    expect(messages("polite")).toEqual(["3 results"]);
  });

  it("announces the same message twice as two additions, and removes each later", () => {
    withDocument();
    uft.useFakeTimers();
    announce("Saved");
    uft.advanceTimersByTime(100);
    announce("Saved");
    expect(messages("polite")).toEqual(["Saved", "Saved"]);
    uft.advanceTimersByTime(7000);
    expect(messages("polite")).toEqual([]);
  });

  it("routes assertive messages to their own region and ignores empty ones", () => {
    withDocument();
    uft.useFakeTimers();
    announce("   ");
    announce("Could not save", { politeness: "assertive", timeout: 0 });
    uft.advanceTimersByTime(100);
    expect(messages("assertive")).toEqual(["Could not save"]);
    expect(messages("polite")).toEqual([]);
    // `timeout: 0` keeps it until it is cleared.
    uft.advanceTimersByTime(60_000);
    expect(messages("assertive")).toEqual(["Could not save"]);
    clearAnnouncements("assertive");
    expect(messages("assertive")).toEqual([]);
  });

  /** A fresh announcer, so the next message waits `SETTLE_MS` on a timer. */
  function freshAnnouncer(): void {
    document.querySelector("[data-uf-live-announcer]")?.remove();
  }

  /**
   * Run `during` with the `document` global gone, as a test worker leaves it
   * between files (ubugeeei-prod/uf#1379 fired a timer into exactly that).
   */
  function withoutDocumentGlobal(during: () => void): void {
    const descriptor = Reflect.getOwnPropertyDescriptor(globalThis, "document");
    if (descriptor == null) throw new Error("expected a document global");
    Reflect.deleteProperty(globalThis, "document");
    try {
      during();
    } finally {
      Reflect.defineProperty(globalThis, "document", descriptor);
    }
  }

  it("never touches a document that went away before its message arrived", () => {
    withDocument();
    uft.useFakeTimers();
    freshAnnouncer();
    announce("3 results");
    announce("Could not save", { politeness: "assertive" });
    const polite = regionFor("polite");
    withoutDocumentGlobal(() => {
      expect(typeof document).toBe("undefined");
      // It used to throw `ReferenceError: document is not defined` here, from
      // the timer, into whichever test the worker was running by then.
      expect(() => uft.advanceTimersByTime(60_000)).not.toThrow();
    });
    expect(polite.children.length).toBe(0);
    expect(uft.getTimerCount()).toBe(0);
  });

  it("drops a message whose regions were taken out of the document", () => {
    withDocument();
    uft.useFakeTimers();
    freshAnnouncer();
    announce("Saved");
    const polite = regionFor("polite");
    // `cleanup()`, or an app replacing `<body>`, detaches the regions.
    document.querySelector("[data-uf-live-announcer]")?.remove();
    expect(() => uft.advanceTimersByTime(100)).not.toThrow();
    expect(polite.children.length).toBe(0);
    expect(uft.getTimerCount()).toBe(0);
  });

  it("removes a delivered message without the document global", () => {
    withDocument();
    uft.useFakeTimers();
    freshAnnouncer();
    announce("Saved");
    uft.advanceTimersByTime(100);
    expect(messages("polite")).toEqual(["Saved"]);
    const polite = regionFor("polite");
    withoutDocumentGlobal(() => {
      expect(() => uft.advanceTimersByTime(7000)).not.toThrow();
    });
    expect(polite.children.length).toBe(0);
  });

  it("cancels pending messages on clearAnnouncements, per politeness", () => {
    withDocument();
    uft.useFakeTimers();
    freshAnnouncer();
    announce("Loading");
    announce("Failed", { politeness: "assertive" });
    expect(uft.getTimerCount()).toBe(2);
    clearAnnouncements("polite");
    expect(uft.getTimerCount()).toBe(1);
    uft.advanceTimersByTime(100);
    expect(messages("polite")).toEqual([]);
    expect(messages("assertive")).toEqual(["Failed"]);
    // The delivered message's removal timer is pending work too.
    expect(uft.getTimerCount()).toBe(1);
    clearAnnouncements();
    expect(uft.getTimerCount()).toBe(0);
    expect(messages("assertive")).toEqual([]);
  });

  it("stays readable while a modal dialog hides the rest of the page", () => {
    withDocument();
    announce("Ready");
    const root = document.querySelector("[data-uf-live-announcer]");
    if (root == null) throw new Error("no announcer");
    // Another direct child of `<body>`, which the dialog must conceal.
    const banner = document.createElement("header");
    document.body?.append(banner);
    render(
      <Dialog.Root defaultOpen>
        <Dialog.Body aria-label="Settings">
          <button type="button">Done</button>
        </Dialog.Body>
      </Dialog.Root>,
    );
    expect(banner.getAttribute("aria-hidden")).toBe("true");
    expect(root.hasAttribute("aria-hidden")).toBe(false);
    expect(root.hasAttribute("inert")).toBe(false);
    cleanup();
    banner.remove();
  });
});

describe("VisuallyHidden", () => {
  it("hides from sight and not from the accessibility tree, keeping the caller's style", () => {
    render(
      <button type="button">
        <span aria-hidden="true">×</span>
        <VisuallyHidden style={{ color: "red" }}>Close</VisuallyHidden>
      </button>,
    );
    const label = htmlOf(screen.getByText("Close"));
    expect(screen.getByRole("button", { name: "Close" })).not.toBeNull();
    expect(label.style.position).toBe("absolute");
    expect(label.style.clipPath).toBe("inset(50%)");
    expect(label.style.color).toBe("red");
  });

  it("shows a focusable one while focus is inside it: the skip link", () => {
    render(
      <VisuallyHidden focusable render={(props) => <a {...props} href="#main" />}>
        Skip to content
      </VisuallyHidden>,
    );
    const link = htmlOf(screen.getByRole("link", { name: "Skip to content" }));
    expect(link.style.position).toBe("absolute");
    act(() => link.focus());
    expect(link.style.position).toBe("");
    act(() => link.blur());
    expect(link.style.position).toBe("absolute");
  });

  it("never shows one that is not focusable, however it gets focus", () => {
    render(
      <VisuallyHidden render={(props) => <a {...props} href="#main" />}>
        Hidden link
      </VisuallyHidden>,
    );
    const link = htmlOf(screen.getByRole("link", { name: "Hidden link" }));
    act(() => link.focus());
    expect(link.style.position).toBe("absolute");
  });
});

it("keeps a collection's live region off the screen", () => {
  // It used to render as plain text under the list: "1 selected".
  render(<ListBox aria-label="Fruit" items={[{ key: "a", textValue: "Apple" }]} />);
  fireEvent.keyDown(screen.getByRole("listbox"), { key: " " });
  const status = htmlOf(screen.getByRole("status"));
  expect(status.textContent).toBe("1 selected");
  expect(status.style.position).toBe("absolute");
});
