// @flow

import { test, expect } from "@uniflowed/test";
import { render, screen, press, changeText } from "@uniflowed/react-native-testing/native";

import { App } from "../../app.native.js";

test("shares a note through the real native input and keeps it across navigation", async () => {
  await render(<App />);
  await changeText(screen.getByLabelText("Your note"), "A small good thing.");
  await press(screen.getByLabelText("Share note"));
  expect(await screen.findByText("A small good thing.")).toBeTruthy();
  await press(screen.getByLabelText("Open note by You"));
  expect(await screen.findByText("Your note")).toBeTruthy();
  await press(screen.getByText("← Back to feed"));
  expect(await screen.findByText("Good to see you.")).toBeTruthy();
  expect(screen.getByDisplayValue("")).toBeTruthy();
});
