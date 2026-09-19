// @flow
import * as React from "react";
import { act, render, screen, fireEvent } from "@testing-library/react-native";
import { createNativeStackNavigator } from "@react-navigation/native-stack";
import { createBottomTabNavigator } from "@react-navigation/bottom-tabs";
import { createNativeNavigation } from "@uniflowed/router/native-navigation";
import { routeTable as table, layouts } from "./router.native.js";
import { test, expect } from "@uniflowed/test";

const create = () => createNativeNavigation({ table, layouts, stack: createNativeStackNavigator(), tabs: createBottomTabNavigator() });

test("file layouts mount real stacks and tabs with shared StyleX and typed params", async () => {
  const app = create();
  const Root = app.Root;
  await render(<Root />);
  expect(await screen.findByText("Home")).toBeTruthy();
  expect(screen.getByTestId("home").props.style).toEqual({ padding: 12, backgroundColor: "#f3f7fb" });
  await fireEvent.press(screen.getByRole("link"));
  expect(await screen.findByText("User 42")).toBeTruthy();
  await fireEvent.press(screen.getByText("Replace user"));
  expect(await screen.findByText("User 43")).toBeTruthy();
  await fireEvent.press(screen.getByText("Back"));
  expect(await screen.findByText("Home")).toBeTruthy();
  await act(async () => app.router.push("/users/Ada%20Lovelace"));
  expect(await screen.findByText("User Ada Lovelace")).toBeTruthy();
  expect(() => app.router.push("https://unclaimed.example/users/1")).toThrow("external");
  expect(() => app.router.push("users/1")).toThrow("relative");
  await app.router.prefetch("/users/99");
});

test("a deep-link state and push use the same route and params", async () => {
  const app = create();
  const Root = app.Root;
  await render(<Root />);
  const state = app.linking.getStateFromPath("/users/42");
  expect(app.linking.getPathFromState(state)).toBe("/users/42");
  await act(async () => app.router.push("/users/42"));
  expect(await screen.findByText("User 42")).toBeTruthy();
});


test("claimed cold and warm OS links open the same screens as push", async () => {
  let receive = null;
  const claimed = { ...table, nativeLinks: { origins: ["https://example.com"], routes: ["/users/:id"] } };
  const app = createNativeNavigation({
    table: claimed, layouts, stack: createNativeStackNavigator(), tabs: createBottomTabNavigator(),
    links: {
      getInitialURL: async () => "https://example.com/users/42",
      addEventListener: (_event, listener) => { receive = listener; return { remove: () => { receive = null; } }; },
    },
  });
  const Root = app.Root;
  await render(<Root />);
  expect(await screen.findByText("User 42")).toBeTruthy();
  await act(async () => receive({ url: "https://example.com/users/43" }));
  expect(await screen.findByText("User 43")).toBeTruthy();
  await act(async () => app.router.back());
  expect(await screen.findByText("User 42")).toBeTruthy();
  await act(async () => receive({ url: "https://unclaimed.example/users/9" }));
  expect(screen.queryByText("User 9")).toBe(null);
});
