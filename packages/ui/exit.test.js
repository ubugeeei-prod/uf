// @flow
//
// Every overlay leaves visibly, and nothing about leaving waits for the eye.
//
// A closing part stays on the page, marked `data-state="closed"` and `inert`,
// for as long as its exit transition runs, and not a moment longer. What a
// reader acts on — focus going back to the trigger, the page scrolling again,
// the rest of the page coming back from behind a modal — happens at the moment
// of closing, while the part is still fading. `internal/presence.js` is the
// rule; this file holds every part to it.
//
// The test DOM runs no CSS, so it has no transitions of its own and every part
// here goes at once unless a test says otherwise. That is the reduced-motion
// case, and the first test of each part checks it: with nothing animating, a
// part is gone in the commit that closed it, as it was before exits existed.
// A test that needs an exit gives the part a `getAnimations` that returns a
// `FakeAnimation`, and decides when it finishes.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  userEvent,
  within,
} from "@uniflowed/react-testing";

import {
  Accordion,
  Collapsible,
  Combobox,
  Dialog,
  HoverCard,
  Menu,
  NavigationMenu,
  Popover,
  Select,
  Sheet,
  Tabs,
  Toast,
  Tooltip,
  dismissAllToasts,
  dismissToast,
  toast,
} from "./index.js";

afterEach(() => {
  act(() => {
    dismissAllToasts();
  });
  cleanup();
});

/** An exit transition whose end the test controls. */
class FakeAnimation {
  readonly finished: Promise<mixed>;
  readonly playState: string = "running";
  readonly effect: { getComputedTiming: () => { endTime: number } } = {
    getComputedTiming: () => ({ endTime: 150 }),
  };
  finish: () => void;

  constructor() {
    let finish = () => {};
    this.finished = new Promise((resolve) => {
      finish = () => resolve(this);
    });
    this.finish = finish;
  }
}

/** Make `element` report an exit in progress, as a browser mid-transition would. */
function holdExit(element: HTMLElement): FakeAnimation {
  const exit = new FakeAnimation();
  const host: $FlowFixMe = element;
  host.getAnimations = () => [exit];
  return exit;
}

/** Let the exit's promise settle, and React commit what follows from it. */
async function settle(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

/** The part under test, found by the test id every case gives it. */
function part(): HTMLElement | null {
  return screen.queryByTestId("part");
}

/** The part, which the test expects to be on the page. */
function present(): HTMLElement {
  const element = part();
  if (element == null) {
    throw new Error("the part is not on the page");
  }
  return element;
}

type Overlay = {|
  readonly name: string,
  readonly render: (open: boolean) => React.Node,
  /**
   * A modal's scrim is outside the panel, so the panel makes it `inert` with
   * the rest of the page while it is open. That is the modal's doing, not the
   * exit's.
   */
  readonly concealed?: boolean,
|};

/**
 * Every overlay part, opened and closed from outside, each carrying
 * `data-testid="part"` on the element that plays the exit.
 */
const OVERLAYS: $ReadOnlyArray<Overlay> = [
  {
    name: "Dialog.Body",
    render: (open) => (
      <Dialog.Root open={open}>
        <Dialog.Trigger>Open</Dialog.Trigger>
        <Dialog.Body aria-label="Settings" data-testid="part">
          <button type="button">Inside</button>
        </Dialog.Body>
      </Dialog.Root>
    ),
  },
  {
    name: "Dialog.Overlay",
    concealed: true,
    render: (open) => (
      <Dialog.Root open={open}>
        <Dialog.Overlay data-testid="part" />
        <Dialog.Body aria-label="Settings">
          <button type="button">Inside</button>
        </Dialog.Body>
      </Dialog.Root>
    ),
  },
  {
    name: "Sheet.Body",
    render: (open) => (
      <Sheet.Root open={open}>
        <Sheet.Body aria-label="Filters" data-testid="part">
          <button type="button">Inside</button>
        </Sheet.Body>
      </Sheet.Root>
    ),
  },
  {
    name: "Popover.Body",
    render: (open) => (
      <Popover.Root open={open}>
        <Popover.Trigger>Open</Popover.Trigger>
        <Popover.Body data-testid="part">
          <button type="button">Inside</button>
        </Popover.Body>
      </Popover.Root>
    ),
  },
  {
    name: "Menu.Body",
    render: (open) => (
      <Menu.Root open={open}>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body data-testid="part">
          <Menu.Item>Open</Menu.Item>
        </Menu.Body>
      </Menu.Root>
    ),
  },
  {
    name: "Select.List",
    render: (open) => (
      <Select.Root open={open}>
        <Select.Trigger>Fruit</Select.Trigger>
        <Select.List data-testid="part">
          <Select.Option value="apple">Apple</Select.Option>
        </Select.List>
      </Select.Root>
    ),
  },
  {
    name: "Combobox.List",
    render: (open) => (
      <Combobox.Root open={open}>
        <Combobox.Input aria-label="Fruit" />
        <Combobox.List data-testid="part">
          <Combobox.Option value="apple">Apple</Combobox.Option>
        </Combobox.List>
      </Combobox.Root>
    ),
  },
  {
    name: "Tooltip.Body",
    render: (open) => (
      <Tooltip.Root open={open}>
        <Tooltip.Trigger aria-label="Bold">B</Tooltip.Trigger>
        <Tooltip.Body data-testid="part">Bold</Tooltip.Body>
      </Tooltip.Root>
    ),
  },
  {
    name: "HoverCard.Body",
    render: (open) => (
      <HoverCard.Root open={open}>
        <HoverCard.Trigger>@ada</HoverCard.Trigger>
        <HoverCard.Body data-testid="part">Ada Lovelace</HoverCard.Body>
      </HoverCard.Root>
    ),
  },
  {
    name: "NavigationMenu.Body",
    render: (open) => (
      <NavigationMenu.Root aria-label="Main" value={open ? "learn" : null}>
        <NavigationMenu.List>
          <NavigationMenu.Item value="learn">
            <NavigationMenu.Trigger>Learn</NavigationMenu.Trigger>
            <NavigationMenu.Body data-testid="part">
              <NavigationMenu.Link href="/guide">Guide</NavigationMenu.Link>
            </NavigationMenu.Body>
          </NavigationMenu.Item>
        </NavigationMenu.List>
      </NavigationMenu.Root>
    ),
  },
];

/**
 * Run `check` against every overlay in turn, each on a clean page, naming the
 * overlay in whatever fails.
 */
async function eachOverlay(check: (overlay: Overlay) => Promise<void> | void): Promise<void> {
  for (const overlay of OVERLAYS) {
    try {
      await check(overlay);
    } catch (error) {
      throw new Error(`${overlay.name}: ${String(error?.message ?? error)}`);
    } finally {
      cleanup();
    }
  }
}

describe("every overlay leaving", () => {
  it("is marked open, and not inert unless a modal conceals it, while it is open", async () => {
    await eachOverlay((overlay) => {
      render(overlay.render(true));
      expect(present()).toHaveAttribute("data-state", "open");
      if (!overlay.concealed) {
        expect(present()).not.toHaveAttribute("inert");
      }
    });
  });

  it("goes in the commit that closed it when nothing is animating", async () => {
    // Reduced motion, or no stylesheet: there is no exit to wait for, and a
    // part must not linger for one.
    await eachOverlay((overlay) => {
      const { rerender } = render(overlay.render(true));
      rerender(overlay.render(false));
      expect(part()).toBe(null);
    });
  });

  it("stays, closed and inert, until its exit has finished", async () => {
    await eachOverlay(async (overlay) => {
      const { rerender } = render(overlay.render(true));
      const element = present();
      const exit = holdExit(element);
      rerender(overlay.render(false));
      // The same element, never removed and put back, and out of reach while
      // it fades: no press, no focus, nothing for a screen reader to find.
      expect(part()).toBe(element);
      expect(element).toHaveAttribute("data-state", "closed");
      expect(element).toHaveAttribute("inert");
      await settle();
      expect(part()).toBe(element);

      exit.finish();
      await settle();
      expect(part()).toBe(null);
    });
  });

  it("opens again from where it was when it is reopened while closing", async () => {
    await eachOverlay(async (overlay) => {
      const { rerender } = render(overlay.render(true));
      const element = present();
      const exit = holdExit(element);
      rerender(overlay.render(false));
      rerender(overlay.render(true));
      expect(part()).toBe(element);
      expect(element).toHaveAttribute("data-state", "open");
      if (!overlay.concealed) {
        expect(element).not.toHaveAttribute("inert");
      }

      // The interrupted exit ending later must not take the reopened part.
      exit.finish();
      await settle();
      expect(part()).toBe(element);
      expect(element).toHaveAttribute("data-state", "open");
    });
  });
});

describe("Dialog: what closing does happens at once", () => {
  component Example() {
    return (
      <div>
        <Dialog.Root>
          <Dialog.Trigger>Open</Dialog.Trigger>
          <Dialog.Overlay data-testid="overlay" />
          <Dialog.Body data-testid="part">
            <Dialog.Title>Settings</Dialog.Title>
            <Dialog.Close>Done</Dialog.Close>
          </Dialog.Body>
        </Dialog.Root>
        <p>Page</p>
      </div>
    );
  }

  it("gives focus back, lets the page scroll and brings it back while the panel fades", async () => {
    const before = document.body?.style.overflow;
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Open" });
    await userEvent.click(trigger);
    const body = present();
    expect(document.body?.style.overflow).toBe("hidden");
    const exit = holdExit(body);

    await userEvent.keyboard("{Escape}");
    // Still on the page, and already closed in every way a reader can tell.
    expect(part()).toBe(body);
    expect(body).toHaveAttribute("inert");
    expect(trigger).toHaveFocus();
    expect(trigger).not.toHaveAttribute("inert");
    expect(trigger).not.toHaveAttribute("aria-hidden");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(document.body?.style.overflow).toBe(before);
    expect(body).not.toHaveAttribute("aria-modal");

    exit.finish();
    await settle();
    expect(part()).toBe(null);
  });

  it("takes focus and the page again when it is reopened while closing", async () => {
    render(<Example />);
    const trigger = screen.getByRole("button", { name: "Open" });
    await userEvent.click(trigger);
    const body = present();
    holdExit(body);
    await userEvent.keyboard("{Escape}");
    expect(trigger).toHaveFocus();

    await userEvent.click(trigger);
    expect(part()).toBe(body);
    expect(body).toHaveAttribute("data-state", "open");
    expect(body).not.toHaveAttribute("inert");
    expect(body).toHaveAttribute("aria-modal", "true");
    expect(screen.getByRole("button", { name: "Done" })).toHaveFocus();
    expect(document.body?.style.overflow).toBe("hidden");
    expect(trigger).toHaveAttribute("inert");
  });

  it("fades the scrim out with the panel, and goes with it", async () => {
    render(<Example />);
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    const overlay = screen.getByTestId("overlay");
    const exit = holdExit(overlay);
    const panelExit = holdExit(present());
    await userEvent.keyboard("{Escape}");
    expect(overlay).toHaveAttribute("data-state", "closed");
    expect(overlay).toHaveAttribute("inert");
    panelExit.finish();
    exit.finish();
    await settle();
    expect(screen.queryByTestId("overlay")).toBe(null);
    expect(part()).toBe(null);
  });
});

describe("Popover and Menu: focus goes back while they fade", () => {
  it("returns focus from a closing popover at once", async () => {
    render(
      <Popover.Root>
        <Popover.Trigger>Open</Popover.Trigger>
        <Popover.Body data-testid="part">
          <button type="button">Inside</button>
        </Popover.Body>
      </Popover.Root>,
    );
    const trigger = screen.getByRole("button", { name: "Open" });
    await userEvent.click(trigger);
    expect(screen.getByRole("button", { name: "Inside" })).toHaveFocus();
    holdExit(present());
    await userEvent.keyboard("{Escape}");
    expect(present()).toHaveAttribute("data-state", "closed");
    expect(trigger).toHaveFocus();
  });

  it("returns focus from a closing menu at once", async () => {
    render(
      <Menu.Root>
        <Menu.Trigger>File</Menu.Trigger>
        <Menu.Body data-testid="part">
          <Menu.Item>Open</Menu.Item>
        </Menu.Body>
      </Menu.Root>,
    );
    const trigger = screen.getByRole("button", { name: "File" });
    await userEvent.click(trigger);
    holdExit(present());
    await userEvent.keyboard("{Escape}");
    expect(present()).toHaveAttribute("inert");
    expect(trigger).toHaveFocus();
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).not.toHaveAttribute("aria-controls");
  });
});

describe("Toast leaving", () => {
  component Example() {
    return (
      <Toast.Region>
        {(each) => (
          <Toast.Root data-testid={`toast-${String(each.content)}`}>
            <Toast.Title>{each.content}</Toast.Title>
            <Toast.Close />
          </Toast.Root>
        )}
      </Toast.Region>
    );
  }

  it("goes at once when nothing is animating", () => {
    render(<Example />);
    let id = "";
    act(() => {
      id = toast("Saved", { duration: null });
    });
    expect(screen.getByTestId("toast-Saved")).toHaveAttribute("data-state", "open");
    act(() => {
      dismissToast(id);
    });
    expect(screen.queryByTestId("toast-Saved")).toBe(null);
  });

  it("stays where it was, closed and inert, until its exit has finished", async () => {
    render(<Example />);
    act(() => {
      toast("First", { duration: null });
      toast("Second", { duration: null });
      toast("Third", { duration: null });
    });
    const second = screen.getByTestId("toast-Second");
    const exit = holdExit(second);
    fireEvent.click(within(second).getByRole("button", { name: "Dismiss" }));

    expect(screen.getByTestId("toast-Second")).toBe(second);
    expect(second).toHaveAttribute("data-state", "closed");
    expect(second).toHaveAttribute("inert");
    // In its place in the stack, rather than jumping to the end to leave.
    const order = Array.from(document.querySelectorAll("[data-testid^=toast-]")).map((each) =>
      each.getAttribute("data-testid"),
    );
    expect(order).toEqual(["toast-First", "toast-Second", "toast-Third"]);

    // A notification that arrives meanwhile is not held up by it.
    act(() => {
      toast("Fourth", { duration: null });
    });
    expect(screen.getByTestId("toast-Fourth")).toHaveAttribute("data-state", "open");

    exit.finish();
    await settle();
    expect(screen.queryByTestId("toast-Second")).toBe(null);
    expect(screen.getByTestId("toast-First")).toHaveAttribute("data-state", "open");
    expect(screen.getByTestId("toast-Third")).toHaveAttribute("data-state", "open");
  });
});

describe("Collapsible and Accordion: hidden after the height transition", () => {
  component CollapsibleExample() {
    return (
      <Collapsible.Root measure>
        <Collapsible.Trigger>Details</Collapsible.Trigger>
        <Collapsible.Content data-testid="part">
          <a href="/shipping">Shipping rates</a>
        </Collapsible.Content>
      </Collapsible.Root>
    );
  }

  component AccordionExample() {
    return (
      <Accordion.Root measure>
        <Accordion.Item value="one">
          <Accordion.Header>
            <Accordion.Trigger>Details</Accordion.Trigger>
          </Accordion.Header>
          <Accordion.Content data-testid="part">
            <a href="/shipping">Shipping rates</a>
          </Accordion.Content>
        </Accordion.Item>
      </Accordion.Root>
    );
  }

  /** Closing with nothing animating hides the panel in the same commit. */
  async function closesAtOnce(): Promise<void> {
    const trigger = screen.getByRole("button", { name: "Details" });
    await userEvent.click(trigger);
    expect(present()).not.toHaveAttribute("hidden");
    expect(present()).toHaveAttribute("data-state", "open");
    await userEvent.click(trigger);
    expect(present()).toHaveAttribute("hidden");
  }

  /** A closing panel stays shown until its height transition has finished. */
  async function waitsForItsHeight(): Promise<void> {
    const trigger = screen.getByRole("button", { name: "Details" });
    await userEvent.click(trigger);
    const panel = present();
    const exit = holdExit(panel);
    await userEvent.click(trigger);
    // The trigger says closed at once; the panel is still on screen for the
    // stylesheet to take down to nothing.
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(panel).toHaveAttribute("data-state", "closed");
    expect(panel).toHaveAttribute("inert");
    expect(panel).not.toHaveAttribute("hidden");

    exit.finish();
    await settle();
    // Hidden the findable way once it has gone, and not inert: find-in-page
    // does not look inside an inert element.
    expect(panel.getAttribute("hidden")).toBe("until-found");
    expect(panel).not.toHaveAttribute("inert");
  }

  /** Reopening part-way through keeps the panel shown and open. */
  async function reopensWhileClosing(): Promise<void> {
    const trigger = screen.getByRole("button", { name: "Details" });
    await userEvent.click(trigger);
    const panel = present();
    const exit = holdExit(panel);
    await userEvent.click(trigger);
    await userEvent.click(trigger);
    expect(panel).toHaveAttribute("data-state", "open");
    expect(panel).not.toHaveAttribute("inert");
    exit.finish();
    await settle();
    expect(panel).not.toHaveAttribute("hidden");
  }

  it("Collapsible: closes at once with nothing animating", async () => {
    render(<CollapsibleExample />);
    await closesAtOnce();
  });

  it("Collapsible: stays shown, closed and inert, until its height has finished", async () => {
    render(<CollapsibleExample />);
    await waitsForItsHeight();
  });

  it("Collapsible: opens again from where it was when reopened while closing", async () => {
    render(<CollapsibleExample />);
    await reopensWhileClosing();
  });

  it("Accordion: closes at once with nothing animating", async () => {
    render(<AccordionExample />);
    await closesAtOnce();
  });

  it("Accordion: stays shown, closed and inert, until its height has finished", async () => {
    render(<AccordionExample />);
    await waitsForItsHeight();
  });

  it("Accordion: opens again from where it was when reopened while closing", async () => {
    render(<AccordionExample />);
    await reopensWhileClosing();
  });

  it("does not measure a panel while its height is moving", async () => {
    render(<CollapsibleExample />);
    const trigger = screen.getByRole("button", { name: "Details" });
    await userEvent.click(trigger);
    const panel = present();
    const before = panel.style.getPropertyValue("--uf-collapsible-height");
    holdExit(panel);
    // A measurement taken now would set `height: auto` on a panel whose height
    // is transitioning, which ends the transition then and there.
    let asked = 0;
    const host: $FlowFixMe = panel;
    const read = host.getBoundingClientRect.bind(panel);
    host.getBoundingClientRect = () => {
      asked += 1;
      return read();
    };
    await userEvent.click(trigger);
    expect(asked).toBe(0);
    expect(panel.style.getPropertyValue("--uf-collapsible-height")).toBe(before);
  });
});

describe("Tabs: where the selected tab is", () => {
  component Example() {
    return (
      <Tabs.Root defaultValue="one">
        <Tabs.List aria-label="Sections" data-testid="list">
          <Tabs.Tab value="one">One</Tabs.Tab>
          <Tabs.Tab value="two">Two</Tabs.Tab>
        </Tabs.List>
        <Tabs.Panel value="one">First</Tabs.Panel>
        <Tabs.Panel value="two">Second</Tabs.Panel>
      </Tabs.Root>
    );
  }

  /** Give `element` a box, which the test DOM does not lay out. */
  function box(element: HTMLElement, left: number, top: number, width: number, height: number) {
    const host: $FlowFixMe = element;
    host.getBoundingClientRect = () => ({
      bottom: top + height,
      height,
      left,
      right: left + width,
      top,
      width,
      x: left,
      y: top,
    });
  }

  it("writes the selected tab's box on the list, and follows the selection", async () => {
    render(<Example />);
    const list = screen.getByTestId("list");
    const one = screen.getByRole("tab", { name: "One" });
    const two = screen.getByRole("tab", { name: "Two" });
    box(list, 100, 50, 400, 40);
    box(one, 104, 52, 60, 36);
    box(two, 172, 52, 80, 36);

    await userEvent.click(two);
    const read = (name: string) => list.style.getPropertyValue(`--uf-tabs-indicator-${name}`);
    // Relative to the list, and unitless so a stylesheet can scale by it.
    expect(read("left")).toBe("72");
    expect(read("top")).toBe("2");
    expect(read("width")).toBe("80");
    expect(read("height")).toBe("36");

    await userEvent.click(one);
    expect(read("left")).toBe("4");
    expect(read("width")).toBe("60");
  });
});
