/**
 * @flow
 * @fileoverview The part of React Native this repository's root check reads,
 * typed for Flow.
 *
 * `react-native` is an optional peer of `@uniflowed/stylex`,
 * `@uniflowed/react-native` and `@uniflowed/router`, installed by the native
 * example and not at this repository's root. So at the root `uf check` saw it
 * as untyped, and `packages/stylex/native.js`'s `import type { ViewStyle }`
 * was "an any-typed value used as a type" — the error Flow gives a type
 * imported from an untyped module.
 *
 * The style types here are open maps, not React Native's: what `native.js`
 * accepts is checked against the real ones where React Native is installed,
 * by `native:example` and `tools/ci/native-stylex-types.mjs`, and a copy of
 * React Native's style declarations here would only agree with itself. The
 * values are the ones the root imports. A project that installs React Native
 * never reads this file: it is the repository's `flow-typed/`, which `uf
 * check` reads for the repository alone.
 */

declare module "react-native" {
  import type { ComponentType } from "react";

  declare export type ViewStyle = { readonly [property: string]: mixed };
  declare export type TextStyle = { readonly [property: string]: mixed };
  declare export type ImageStyle = { readonly [property: string]: mixed };

  declare export var Pressable: ComponentType<{ readonly onPress?: () => mixed, ... }>;
}
