// @flow

import { test, expect } from "@uniflowed/test";
import { render, screen, press, changeText } from "@uniflowed/react-native-testing/native";

import { App } from "../app.native.js";

/** Create the account every signed-in flow below starts from. */

async function join() {
  await press(screen.getByLabelText("Create account"));
  await changeText(await screen.findByLabelText("Display name"), "Aki Mori");
  await changeText(screen.getByLabelText("Handle"), "aki");
  await changeText(screen.getByLabelText("Email"), "aki@example.test");
  await press(screen.getByLabelText("Create account"));
}

test("a guest reads the seeded feed, filters it, and is invited to join", async () => {
  await render(<App />);
  expect(await screen.findByText("Notes from the people in your community.")).toBeTruthy();
  expect(screen.getByText("Mika Tan")).toBeTruthy();
  expect(screen.getByText("Niko Reyes")).toBeTruthy();
  expect(screen.getByText("What are you working on?")).toBeTruthy();
  expect(screen.queryByLabelText("Post body")).toBe(null);

  await press(screen.getByLabelText("Shipping"));
  expect(screen.getByText("Ren Ito")).toBeTruthy();
  expect(screen.queryByText("Mika Tan")).toBe(null);

  await press(screen.getByLabelText("All notes"));
  await changeText(screen.getByLabelText("Search notes"), "slow query");
  await press(screen.getByLabelText("Search"));
  expect(await screen.findByText("Results for “slow query”")).toBeTruthy();
  expect(screen.getByText("Sora Lin")).toBeTruthy();
  expect(screen.queryByText("Ren Ito")).toBe(null);

  await changeText(screen.getByLabelText("Search notes"), "nothing matches this");
  await press(screen.getByLabelText("Search"));
  expect(await screen.findByText("No notes here yet")).toBeTruthy();
  await press(screen.getByLabelText("Clear search"));
  expect(await screen.findByText("Mika Tan")).toBeTruthy();
});

test("an account refuses a bad handle, then publishes, appreciates and opens a note", async () => {
  await render(<App />);
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
  expect(await screen.findByText("Native parity is in review.")).toBeTruthy();
  expect(screen.getByText("@aki")).toBeTruthy();
  expect(screen.getByText("0/500")).toBeTruthy();

  await press(screen.getAllByLabelText("Appreciate · 0")[0]);
  expect(await screen.findByLabelText("Remove appreciation · 1")).toBeTruthy();

  await press(screen.getByLabelText("Open note by Aki Mori"));
  expect(await screen.findByText("← Back to feed")).toBeTruthy();
  // The note's own screen shows the same reaction, and changes it for the feed too.
  await press(screen.getByLabelText("Remove appreciation · 1"));
  await press(screen.getByLabelText("Back to feed"));
  expect(await screen.findByText("Notes from the people in your community.")).toBeTruthy();
  expect(screen.queryByLabelText("Remove appreciation · 1")).toBe(null);

  await press(screen.getByLabelText("Shipping"));
  expect(screen.getByText("Native parity is in review.")).toBeTruthy();
  await press(screen.getByLabelText("Design"));
  expect(screen.queryByText("Native parity is in review.")).toBe(null);
});

test("the inbox and settings belong to the account, and signing out closes them", async () => {
  await render(<App />);
  await press(await screen.findByText("Inbox"));
  expect(await screen.findByText("Sign in to read your messages")).toBeTruthy();
  await join();

  await press(await screen.findByLabelText("Conversation with Mika Tan"));
  expect(await screen.findByText("Welcome to Commonplace. What are you working on?")).toBeTruthy();
  await changeText(screen.getByLabelText("Message"), "Porting the feed to native.");
  await press(screen.getByLabelText("Send message"));
  expect(await screen.findByText("Porting the feed to native.")).toBeTruthy();
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
  expect(screen.getByText("Aki M.")).toBeTruthy();

  await press(screen.getByLabelText("Sign out"));
  expect(await screen.findByText("Sign in to continue")).toBeTruthy();
  await press(screen.getByText("Inbox"));
  expect(await screen.findByText("Sign in to read your messages")).toBeTruthy();
});
