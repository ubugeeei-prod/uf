// @flow
import { test, expect } from "@uniflowed/test";
import { render, screen, fireEvent } from "@uniflowed/react-native-testing/native";
import { App } from "../../app.native.js";
test("shares a note through the real native input and keeps it across navigation", async () => {
  await render(<App />);
  await fireEvent.changeText(screen.getByLabelText("Your note"), "A small good thing.");
  await fireEvent.press(screen.getByLabelText("Share note"));
  expect(await screen.findByText("A small good thing.")).toBeTruthy();
  await fireEvent.press(screen.getByLabelText("Open note by You"));
  expect(await screen.findByText("Your note")).toBeTruthy();
  await fireEvent.press(screen.getByText("← Back to feed"));
  expect(await screen.findByText("Good to see you.")).toBeTruthy();
  expect(screen.getByLabelText("Your note").props.value).toBe("");
});
