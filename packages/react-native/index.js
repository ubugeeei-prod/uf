// @flow
//
// `@uniflowed/react-native`: React Native, re-exported.
//
// This is the real `react-native` package, not a declaration of it. uf owns the
// Flow-first compiler, router, lints and target selection around React Native;
// the host components, Platform object, StyleSheet runtime and native module
// bridge stay with React Native itself.
//
// `export *` rather than a list of names: React Native's public JavaScript
// surface moves with its release, and a hand-maintained copy here would turn
// every upstream addition into a uf runtime gap.
//
// React Native is a peer dependency, so the application chooses the exact
// version. uf's own release does not decide when an app moves its native host.

export * from "react-native";

import * as ReactNative from "react-native";

export { ReactNative };
