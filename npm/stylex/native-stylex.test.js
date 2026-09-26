// @flow
import { expect, test } from "@uniflowed/test";
import { props } from "./props.js";
import { props as nativeProps } from "./native.js";

test("native namespaces merge as style props, with the last value winning", () => {
  const first = { $$native: true, padding: 12, opacity: 0.5 };
  const second = { $$native: true, opacity: 1, color: "red" };
  expect(props([first, false, [second]], null)).toEqual({
    style: { padding: 12, opacity: 1, color: "red" },
  });
  expect(props({ $$native: true })).toEqual({ style: {} });
});

test("the native entry preserves merges and refuses web or uncompiled namespaces", () => {
  expect(
    nativeProps({ $$native: true, padding: 12 }, false, [{ $$native: true, padding: 24 }]),
  ).toEqual({ style: { padding: 24 } });
  expect(nativeProps(null, false, [] as Array<null>)).toEqual({ style: {} });
  expect(() => nativeProps({ $$css: true, padding: "x123" })).toThrow("compiled native namespace");
  expect(() => nativeProps({ padding: 12 })).toThrow("compiled native namespace");
});

test("web and native outputs cannot accidentally share one runtime result", () => {
  expect(() => props({ $$native: true, color: "red" }, { $$css: true, color: "x123" })).toThrow(
    "cannot mix",
  );
  expect(props({ $$css: true, color: "x123" })).toEqual({ className: "x123" });
});
