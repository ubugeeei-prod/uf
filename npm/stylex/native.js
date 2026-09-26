// @flow
import type { ViewStyle, TextStyle, ImageStyle } from "react-native";
import { nativeRuntimeRequired } from "@uniflowed/core/native";
import { nativeProps } from "./native-props.js";

// Use one property map: a ViewStyle | TextStyle union would accept an invalid
// fontWeight through its ViewStyle branch, whose optional fields all match.
export type NativeStyle = $ReadOnly<{
  ...$Exact<ViewStyle>,
  ...$Exact<TextStyle>,
  ...$Exact<ImageStyle>,
}>;
export type NativeStyleArgument<Style> =
  | Style
  | false
  | null
  | void
  | $ReadOnlyArray<NativeStyleArgument<Style>>;
type StyleFields<Value> = Value extends $ReadOnlyArray<infer Item>
  ? StyleFields<Item>
  : Value extends (false | null | void)
    ? {}
    : Value;

/** Compile-time namespaces, checked against the app's React Native styles. */
export function create<Styles extends { readonly [string]: NativeStyle }>(styles: Styles): Styles {
  return nativeRuntimeRequired("@uniflowed/stylex/native", "stylex.create");
}

/** Native-only props retain the authored property types and never add className. */
export function props<Args extends $ReadOnlyArray<NativeStyleArgument<NativeStyle>>>(
  ...styles: Args
): { readonly style: Partial<StyleFields<Args>> } {
  const result = nativeProps(styles, true);
  // The compiler validates the native subset and preserves every scalar value.
  // The merger copies those fields, but its dynamic keys cannot retain Style.
  return (result ?? { style: {} }) as $FlowFixMe;
}

export const stylex = { create, props };
