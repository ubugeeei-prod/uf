// @flow
//
// `@uniflowed/stylex`: the merge that runs, and the preset the compiler makes.
//
// This file is itself compiled by `uf transform` before the runner loads it, so
// the `stylex.create` below is not a fixture — it is the compiler's own output,
// and the assertions on it are assertions that the runtime reads what the
// compiler wrote. The same goes for `@uniflowed/stylex/preset` and
// `/theme`: importing them compiles them, so a preset that does not compile
// fails here rather than in a browser.
//
// The merge is modelled a second time in `crates/uf_stylex/src/props.rs`, which
// is what lets its ordering be tested at compile time. The two have to agree,
// and the cases below are the ones where they could disagree.

import { describe, expect, it, render, screen } from "@uniflowed/testing";
import { props, stylex } from "@uniflowed/stylex";
import {
  buttonStyles,
  cardStyles,
  controlStyles,
  fieldStyles,
  menuItemStyles,
  tabStyles,
  textStyles,
} from "@uniflowed/stylex/preset";
import { ufAutoTheme, ufDarkTheme } from "@uniflowed/stylex/theme";
import { Switch, Tabs } from "@uniflowed/ui";

/** A compiled namespace, in the shape `uf transform` emits. */
function compiled(properties: { readonly [string]: string | null }): mixed {
  return { $$css: true, ...properties };
}

/** The class names in a `StyleProps`, as a list. */
function classes(styled: { readonly className?: string }): $ReadOnlyArray<string> {
  const name = styled.className;
  return name == null || name === "" ? [] : name.split(" ");
}

// Compiled by `uf transform` on the way in. `local.base` is a real compiled
// namespace, not an object this file wrote to look like one.
const local = stylex.create({
  base: { color: "black", paddingTop: 8 },
  loud: { color: { default: "red", ":hover": "maroon" } },
  quiet: { color: "grey" },
});

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

  it("never puts the compiled marker in a class attribute", () => {
    expect(props({ $$css: true })).toEqual({});
  });
});

describe("a property written with states", () => {
  // A property with more than one state compiles to a map rather than to a
  // class name. Keeping only the strings would drop every `:hover`, every
  // `:focus-visible` and every media query in an application, while every test
  // that used a plain value carried on passing.

  it("contributes every one of its classes", () => {
    const out = props(compiled({ color: { default: "c1", ":hover": "c2" } }));

    expect(classes(out)).toEqual(["c1", "c2"]);
  });

  it("keeps the order the compiler wrote the states in", () => {
    // The compiler writes a property's states in sheet order, so the base rule
    // is first and the state that overrides it follows.
    const out = props(compiled({ color: { default: "c1", ":hover": "c2", ":active": "c3" } }));

    expect(classes(out)).toEqual(["c1", "c2", "c3"]);
  });

  it("is replaced whole by a later plain value, states included", () => {
    const out = props(
      compiled({ color: { default: "c1", ":hover": "c2" } }),
      compiled({ color: "c9" }),
    );

    expect(classes(out)).toEqual(["c9"]);
  });

  it("replaces an earlier plain value whole", () => {
    const out = props(
      compiled({ color: "c9" }),
      compiled({ color: { default: "c1", ":hover": "c2" } }),
    );

    expect(classes(out)).toEqual(["c1", "c2"]);
  });

  it("skips a state the compiler deliberately unset", () => {
    const out = props(compiled({ color: { default: "c1", ":hover": null } }));

    expect(classes(out)).toEqual(["c1"]);
  });
});

describe("what the compiler actually emitted for this file", () => {
  it("gives every namespace the compiled marker", () => {
    // If this fails, `uf transform` did not see the call and everything below
    // is testing an object literal rather than the compiler.
    expect(local.base.$$css).toBe(true);
  });

  it("compiles a plain property to one class and a stateful one to a map", () => {
    expect(typeof local.base.color).toBe("string");
    expect(typeof local.loud.color).toBe("object");
  });

  it("merges the compiler's own output the way the compile-time model says", () => {
    const out = props(local.base, local.loud);

    // `paddingTop` survives untouched, and `color` is the loud namespace's
    // default and hover classes — three classes, none of them shared.
    expect(classes(out)).toHaveLength(3);
    expect(out.className).toContain(String(local.base.paddingTop));
    expect(out.className).not.toContain(String(local.base.color));
  });

  it("lets a later plain colour drop the hover state that came before it", () => {
    const out = props(local.base, local.loud, local.quiet);

    expect(classes(out)).toHaveLength(2);
    expect(out.className).toContain(String(local.quiet.color));
  });

  it("gives the same property in two namespaces two different classes", () => {
    // The namespace takes part in the class name, which is why a merge has
    // something to choose between.
    expect(local.base.color).not.toBe(local.quiet.color);
  });
});

describe("the preset", () => {
  it("answers with a className and nothing else", () => {
    const styled = buttonStyles({ tone: "primary" });

    expect(Object.keys(styled)).toEqual(["className"]);
    expect(classes(styled).length).toBeGreaterThan(4);
  });

  it("gives a project a default look without a token set", () => {
    // Every one of these is a compiled namespace: reaching `stylex.create` at
    // run time would have thrown on import.
    for (const styled of [
      cardStyles(),
      textStyles(),
      buttonStyles(),
      fieldStyles(),
      menuItemStyles(),
      tabStyles(),
      controlStyles(),
    ]) {
      expect(classes(styled).length).toBeGreaterThan(0);
    }
  });

  it("changes the classes when a variant changes", () => {
    expect(buttonStyles({ tone: "primary" }).className).not.toBe(
      buttonStyles({ tone: "danger" }).className,
    );
    expect(tabStyles({ selected: true }).className).not.toBe(tabStyles().className);
  });

  it("keeps the same number of classes for two tones of one control", () => {
    // A tone replaces properties rather than adding them, so a danger button is
    // not a heavier button.
    expect(classes(buttonStyles({ tone: "primary" }))).toHaveLength(
      classes(buttonStyles({ tone: "danger" })).length,
    );
  });

  it("lets a variant replace a hover state it does not want", () => {
    // `menuItemStyles({ active: true })` sets `backgroundColor` plainly over an
    // item that set it with a `:hover`, so the hover class has to be gone.
    const plain = classes(menuItemStyles());
    const active = classes(menuItemStyles({ active: true }));

    expect(active.length).toBe(plain.length - 1);
  });

  it("adds a class for a property a state introduces", () => {
    // A disabled button dims itself, and nothing in the base sets `opacity`.
    expect(classes(buttonStyles({ disabled: true })).length).toBeGreaterThan(
      classes(buttonStyles()).length,
    );
  });

  it("replaces rather than adds when a state only changes properties the base set", () => {
    // An invalid field recolours its border and its background, both of which
    // the base already claimed — so it is a different look, not a heavier one.
    const plain = fieldStyles();
    const invalid = fieldStyles({ invalid: true });

    expect(classes(invalid)).toHaveLength(classes(plain).length);
    expect(invalid.className).not.toBe(plain.className);
  });
});

describe("a theme", () => {
  it("is a compiled namespace, so it is applied by spreading it", () => {
    expect(ufDarkTheme.$$css).toBe(true);
    expect(classes(props(ufDarkTheme)).length).toBeGreaterThan(10);
  });

  it("names custom properties, because that is what a token is", () => {
    for (const property of Object.keys(ufDarkTheme)) {
      if (property !== "$$css") {
        expect(property.startsWith("--")).toBe(true);
      }
    }
  });

  it("writes only a conditional class when every override is conditional", () => {
    // The automatic theme costs one media block: outside dark mode its classes
    // match no rule at all.
    for (const property of Object.keys(ufAutoTheme)) {
      if (property !== "$$css") {
        expect(typeof ufAutoTheme[property]).toBe("object");
      }
    }
  });

  it("is replaced token by token by a later theme", () => {
    const both = classes(props(ufAutoTheme, ufDarkTheme));

    // Both themes cover the same tokens, so the later one wins all of them and
    // nothing of the earlier one is left.
    expect(both).toEqual(classes(props(ufDarkTheme)));
  });

  it("composes with the styles it themes rather than replacing them", () => {
    const themed = classes(props(ufDarkTheme, local.base));

    expect(themed.length).toBe(classes(props(ufDarkTheme)).length + 2);
  });
});

describe("the preset on a headless primitive", () => {
  // `@uniflowed/ui` ships no styles and has no StyleX in its types. The preset
  // reaches it the way any other stylesheet would: through `className`.

  it("puts the preset's classes on a switch", () => {
    render(
      <Switch {...controlStyles({ shape: "track" })}>
        <span />
      </Switch>,
    );

    const control = screen.getByRole("switch");
    const expected = classes(controlStyles({ shape: "track" }));

    expect(expected.length).toBeGreaterThan(0);
    for (const name of expected) {
      expect(control.classList.contains(name)).toBe(true);
    }
  });

  it("keeps the behaviour the primitive is there for", () => {
    render(<Switch {...controlStyles({ shape: "track" })} defaultChecked={true} />);

    expect(screen.getByRole("switch").getAttribute("aria-checked")).toBe("true");
  });

  it("styles a selected tab differently from an unselected one", () => {
    render(
      <Tabs.Root defaultValue="one">
        <Tabs.List>
          <Tabs.Tab value="one" {...tabStyles({ selected: true })}>
            One
          </Tabs.Tab>
          <Tabs.Tab value="two" {...tabStyles()}>
            Two
          </Tabs.Tab>
        </Tabs.List>
      </Tabs.Root>,
    );

    const [first, second] = screen.getAllByRole("tab");

    expect(first.getAttribute("class")).not.toBe(second.getAttribute("class"));
    expect(first.getAttribute("aria-selected")).toBe("true");
  });
});

describe("the compile-time surface", () => {
  // Reached through a local binding on purpose. A direct `stylex.create({…})`
  // here would be rewritten by `uf transform` like any other call — which is
  // exactly the point of these three functions — so the only way to test what
  // happens when the compiler did *not* see a call is to write one it cannot.
  const create = stylex.create;
  const defineVars = stylex.defineVars;
  const createTheme = stylex.createTheme;

  it("throws when a create call reached the runtime", () => {
    // Reaching `create` means the compiler never saw the call, so the styles it
    // declares are in no stylesheet. Failing loudly is the only honest answer:
    // handing the input back renders an application with no styles and no
    // explanation.
    expect(() => create({ root: { color: "red" } })).toThrow();
    expect(() => defineVars({ accent: "#35D6F6" })).toThrow();
    expect(() => createTheme({ accent: "#35D6F6" }, { accent: "#D84BFF" })).toThrow();
  });

  it("names the binding that was reached", () => {
    expect(() => create({ root: { color: "red" } })).toThrow("stylex.create");
  });
});
