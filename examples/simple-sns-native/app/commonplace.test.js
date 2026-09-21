// @flow

import { test, expect } from "@uniflowed/test";
import { render, screen, press, changeText } from "@uniflowed/react-native-testing/native";

import type { Service } from "./_shared/service.js";

import { Commonplace } from "../app.native.js";
import { createService } from "./_shared/service.js";
import { failed } from "./_shared/social.js";

/** The service without its latency: every call still settles later, never in the same task. */

const instant = (): Service => createService(0);

/** A promise a test settles by hand, to hold a call open while it looks at the screen. */

function gate(): {| readonly opened: Promise<void>, readonly open: () => void |} {
  let open = () => {};
  const opened = new Promise<void>((resolve) => {
    open = () => resolve();
  });

  return { opened, open };
}

/** Create the account every signed-in flow below starts from. */

async function join() {
  await press(await screen.findByLabelText("Create account"));
  await changeText(await screen.findByLabelText("Display name"), "Aki Mori");
  await changeText(screen.getByLabelText("Handle"), "aki");
  await changeText(screen.getByLabelText("Email"), "aki@example.test");
  await press(screen.getByLabelText("Create account"));
}

test("a guest waits for the feed, filters it, and is invited to join", async () => {
  const real = instant();
  const held = gate();
  let first = true;
  const service: Service = {
    ...real,
    feed: async (filter) => {
      if (first) {
        first = false;
        await held.opened;
      }

      return real.feed(filter);
    },
  };
  await render(<Commonplace service={service} />);
  // The heading is there at once; the notes are behind a boundary that says it is loading.
  expect(screen.getByText("Notes from the people in your community.")).toBeTruthy();
  expect(screen.getByLabelText("Loading notes")).toBeTruthy();
  expect(screen.queryByText("Mika Tan")).toBe(null);
  held.open();
  expect(await screen.findByText("Mika Tan")).toBeTruthy();
  expect(screen.queryByLabelText("Loading notes")).toBe(null);
  expect(screen.getByText("Niko Reyes")).toBeTruthy();
  expect(screen.getByText("What are you working on?")).toBeTruthy();
  expect(screen.queryByLabelText("Post body")).toBe(null);

  // Changing channel is a transition: no skeleton again, the old notes stay until the new arrive.
  await press(screen.getByLabelText("Shipping"));
  expect(screen.queryByLabelText("Loading notes")).toBe(null);
  expect(await screen.findByText("Ren Ito")).toBeTruthy();
  expect(screen.queryByText("Mika Tan")).toBe(null);

  await press(screen.getByLabelText("All notes"));
  expect(await screen.findByText("Mika Tan")).toBeTruthy();
  await changeText(screen.getByLabelText("Search notes"), "slow query");
  await press(screen.getByLabelText("Search"));
  expect(await screen.findByText("Results for “slow query”")).toBeTruthy();
  expect(await screen.findByText("Sora Lin")).toBeTruthy();
  expect(screen.queryByText("Ren Ito")).toBe(null);

  await changeText(screen.getByLabelText("Search notes"), "nothing matches this");
  await press(screen.getByLabelText("Search"));
  expect(await screen.findByText("No notes here yet")).toBeTruthy();
  await press(screen.getByLabelText("Clear search"));
  expect(await screen.findByText("Mika Tan")).toBeTruthy();
});

test("an account refuses a bad handle, then publishes, appreciates and opens a note", async () => {
  await render(<Commonplace service={instant()} />);
  await press(await screen.findByLabelText("Create account"));
  await changeText(await screen.findByLabelText("Display name"), "Aki Mori");
  await changeText(screen.getByLabelText("Handle"), "Mika");
  await changeText(screen.getByLabelText("Email"), "aki@example.test");
  await press(screen.getByLabelText("Create account"));
  expect(await screen.findByText(/^Handles start with a letter/)).toBeTruthy();
  await changeText(screen.getByLabelText("Handle"), "mika");
  await press(screen.getByLabelText("Create account"));
  expect(await screen.findByText("That handle is taken.")).toBeTruthy();
  await changeText(screen.getByLabelText("Handle"), "aki");
  await press(screen.getByLabelText("Create account"));

  await changeText(await screen.findByLabelText("Post body"), "Native parity is in review.");
  expect(screen.getByText("27/500")).toBeTruthy();
  await press(screen.getByLabelText("Post to Shipping"));
  await press(screen.getByLabelText("Publish note"));
  expect(await screen.findByLabelText("Open note by Aki Mori")).toBeTruthy();
  expect(screen.getByText("@aki")).toBeTruthy();
  // The draft is cleared by the same refresh that brought the note in.
  expect(await screen.findByText("0/500")).toBeTruthy();

  await press(screen.getAllByLabelText("Appreciate · 0")[0]);
  expect(await screen.findByLabelText("Remove appreciation · 1")).toBeTruthy();

  await press(screen.getByLabelText("Open note by Aki Mori"));
  expect(await screen.findByText("← Back to feed")).toBeTruthy();
  // The note's own screen reads the note itself, and its reaction refreshes the feed too.
  await press(await screen.findByLabelText("Remove appreciation · 1"));
  expect(await screen.findByLabelText("Appreciate · 0")).toBeTruthy();
  await press(screen.getByLabelText("Back to feed"));
  expect(await screen.findByText("Notes from the people in your community.")).toBeTruthy();
  expect(screen.queryByLabelText("Remove appreciation · 1")).toBe(null);

  await press(screen.getByLabelText("Shipping"));
  expect(await screen.findByText("Ren Ito")).toBeTruthy();
  expect(screen.getByText("Native parity is in review.")).toBeTruthy();
  await press(screen.getByLabelText("Design"));
  expect(await screen.findByText("Mika Tan")).toBeTruthy();
  expect(screen.queryByText("Native parity is in review.")).toBe(null);
});

test("a reaction shows at once and goes back by itself when the service refuses it", async () => {
  const real = instant();
  const held = gate();
  const service: Service = {
    ...real,
    appreciate: async () => {
      await held.opened;

      return failed("Could not save. Try again.");
    },
  };
  await render(<Commonplace service={service} />);
  await join();

  await press((await screen.findAllByLabelText("Appreciate · 0"))[0]);
  // Optimistic: the service has not answered, and the heart already has.
  expect(await screen.findByLabelText("Remove appreciation · 1")).toBeTruthy();
  held.open();
  expect(await screen.findByText("Could not save. Try again.")).toBeTruthy();
  expect(screen.queryByLabelText("Remove appreciation · 1")).toBe(null);
  expect(screen.getAllByLabelText("Appreciate · 0").length).toBe(4);
});

test("a note being published is in the list before the service has it", async () => {
  const real = instant();
  const held = gate();
  const service: Service = {
    ...real,
    publish: async (body, topic) => {
      await held.opened;

      return real.publish(body, topic);
    },
  };
  await render(<Commonplace service={service} />);
  await join();

  await changeText(await screen.findByLabelText("Post body"), "Held at the door.");
  await press(screen.getByLabelText("Publish note"));
  // Both the button and the optimistic note say so while the Action runs.
  expect((await screen.findAllByText("Publishing…")).length).toBe(2);
  expect(screen.getByText("Held at the door.")).toBeTruthy();
  expect(screen.queryByLabelText("Open note by Aki Mori")).toBe(null);
  held.open();
  expect(await screen.findByLabelText("Open note by Aki Mori")).toBeTruthy();
  expect(screen.queryByText("Publishing…")).toBe(null);
});

test("a read that fails says so in its own region, and trying again recovers", async () => {
  const real = instant();
  let refusals = 1;
  const service: Service = {
    ...real,
    feed: async (filter) => {
      if (refusals > 0) {
        refusals -= 1;
        throw new Error("offline");
      }

      return real.feed(filter);
    },
  };
  await render(<Commonplace service={service} />);
  expect(await screen.findByText("Could not load notes")).toBeTruthy();
  // The rest of the screen is unaffected by the region that failed.
  expect(screen.getByLabelText("Search notes")).toBeTruthy();
  await press(screen.getByLabelText("Try again"));
  expect(await screen.findByText("Mika Tan")).toBeTruthy();
  expect(screen.queryByText("Could not load notes")).toBe(null);
});

test("the inbox and settings belong to the account, and signing out closes them", async () => {
  await render(<Commonplace service={instant()} />);
  await press(await screen.findByText("Inbox"));
  expect(await screen.findByText("Sign in to read your messages")).toBeTruthy();
  await join();

  await press(await screen.findByLabelText("Conversation with Mika Tan"));
  expect(await screen.findByText("Welcome to Commonplace. What are you working on?")).toBeTruthy();
  await changeText(screen.getByLabelText("Message"), "Porting the feed to native.");
  await press(screen.getByLabelText("Send message"));
  expect(await screen.findByText("Porting the feed to native.")).toBeTruthy();
  expect(await screen.findByDisplayValue("")).toBeTruthy();
  await press(screen.getByLabelText("Back to inbox"));
  // The thread's preview is its last message.
  expect(await screen.findByText("Porting the feed to native.")).toBeTruthy();

  await press(screen.getByText("Settings"));
  expect(await screen.findByDisplayValue("aki@example.test")).toBeTruthy();
  await changeText(screen.getByLabelText("Display name"), "Aki M.");
  await changeText(screen.getByLabelText("Email"), "not an address");
  await press(screen.getByLabelText("Save changes"));
  expect(await screen.findByText("Enter an email address.")).toBeTruthy();
  await changeText(screen.getByLabelText("Email"), "aki@example.test");
  await press(screen.getByLabelText("Save changes"));
  expect(await screen.findByText("Your changes are saved.")).toBeTruthy();
  // The name above the form is a read, refreshed by the save.
  expect(await screen.findByText("Aki M.")).toBeTruthy();

  await press(screen.getByLabelText("Sign out"));
  expect(await screen.findByText("Sign in to continue")).toBeTruthy();
  await press(screen.getByText("Inbox"));
  expect(await screen.findByText("Sign in to read your messages")).toBeTruthy();
});
