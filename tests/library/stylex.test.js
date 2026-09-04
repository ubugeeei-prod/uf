// @flow
//
// `stylex.props`, the one part of StyleX that runs.
//
// Everything else is compiled away by `uf transform`, so these tests hand
// `props` the shape the compiler produces — an object with `$$css: true` and a
// class name per property — rather than calling `create`, which throws when it
// is reached at runtime precisely because reaching it means the compiler did
// not see the call.
//
// The merge is modelled a second time in `crates/uf_stylex/src/props.rs`, which
// is what lets its ordering be tested at compile time. The two have to agree,
// and the cases below are the ones where they could disagree.

import { describe, expect, it } from "@uniflowed/testing";
import { props, stylex } from "@uniflowed/stylex";

/** A compiled namespace, in the shape `uf transform` emits. */
function compiled(properties: { readonly [string]: string | null }): mixed {
  return { $$css: true, ...properties };
}

describe("stylex.props", () => {
  it("returns the class for a single namespace", () => {
    expect(props(compiled({ color: "c1" }))).toEqual({ className: "c1" });
  });

  it("joins the classes of disjoint properties with a space", () => {
    const out = props(compiled({ color: "c1", paddingTop: "p1" }));

    expect(out.className).toBe("c1 p1");
  });

  it("lets a later namespace replace an earlier one property by property", () => {
    const out = props(compiled({ color: "c1" }), compiled({ color: "c2" }));

    expect(out).toEqual({ className: "c2" });
  });

  it("keeps the properties a later namespace did not mention", () => {
    const out = props(compiled({ color: "c1", paddingTop: "p1" }), compiled({ color: "c2" }));

    expect(out.className).toBe("c2 p1");
  });

  it("skips falsy arguments, which is what conditional styles are", () => {
    const active = false;
    const out = props(compiled({ color: "c1" }), active && compiled({ color: "c2" }));

    expect(out).toEqual({ className: "c1" });
    expect(props(null, undefined, false)).toEqual({});
  });

  it("flattens a list of namespaces", () => {
    const out = props([compiled({ color: "c1" }), compiled({ paddingTop: "p1" })]);

    expect(out.className).toBe("c1 p1");
  });

  it("treats null as a deliberate unset rather than a class", () => {
    const out = props(compiled({ color: "c1" }), compiled({ color: null }));

    expect(out).toEqual({});
  });

  it("returns an empty object when nothing survives, not an empty className", () => {
    // `{...stylex.props()}` has to spread to nothing, or every element gets
    // `class=""`.
    expect(props()).toEqual({});
  });

  it("orders classes by the property that first claimed one", () => {
    // `color` is claimed first and keeps its position even though the class
    // that wins it comes from the second namespace.
    const out = props(
      compiled({ color: "c1", paddingTop: "p1" }),
      compiled({ paddingTop: "p2", color: "c2" }),
    );

    expect(out.className).toBe("c2 p2");
  });

  it("is reachable under its qualified name too", () => {
    expect(stylex.props(compiled({ color: "c1" }))).toEqual({ className: "c1" });
  });
});

describe("the compile-time surface", () => {
  it("throws when a create call reached the runtime", () => {
    // Reaching `create` means the compiler never saw the call, so the styles it
    // declares are in no stylesheet. Failing loudly is the only honest answer:
    // handing the input back renders an application with no styles and no
    // explanation.
    expect(() => stylex.create({ root: { color: "red" } })).toThrow();
    expect(() => stylex.defineVars({ accent: "#35D6F6" })).toThrow();
    expect(() => stylex.createTheme({ accent: "#D84BFF" })).toThrow();
  });
});
