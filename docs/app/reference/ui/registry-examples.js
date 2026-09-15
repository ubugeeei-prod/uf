"use client";
// @flow
//
// Every registry component's example, for the components reference to render
// live.
//
// A client module because the examples are client code. A registry component is
// a client component, and an example may hand one a function, such as a
// calendar's `isDateDisabled` or a slider's `valueText`, which a server
// component cannot pass across the boundary. The page hands this module a name,
// which it can.
//
// The examples are `registry/ui/` itself, read when the site is built, so a
// component added there is rendered here without anyone adding it by hand.

import * as React from "@uniflowed/react";

/** What an example module exports. */
type ExampleModule = { readonly Example: React.ComponentType<{}>, ... };

const EXAMPLES = import.meta.glob<ExampleModule>("../../../../registry/ui/*.example.js", {
  eager: true,
});

/** The example of the registry component called `name`, running. */
export component RegistryExample(name: string) {
  const example = EXAMPLES[`../../../../registry/ui/${name}.example.js`];
  if (example == null) {
    throw new Error(`registry/ui/ has no example for \`${name}\``);
  }
  const Example = example.Example;
  return <Example />;
}
