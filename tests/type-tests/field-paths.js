// @flow
//
// What a typed field path refuses, and the test that says so.
//
// This file is *supposed* to fail `uf check`. `@uniflowed/form` claims that a
// per-field read comes back as the type that field actually holds, inferred
// from the form's values rather than annotated at the call — and a claim about
// inference cannot be proved by rendering anything. It is proved the only way
// it can be: by running the checker and reading what it said.
//
// # How it is read
//
// A `// expect:` comment says that the line after it must be reported, and that
// the report must contain that text. A line without one must not be reported at
// all — so a change that makes any of these *stop* being an error fails the
// test, and so does one that makes something else here start being one. The
// lines with no marker are the other half of the claim: the dotted string form
// still works, and every correct typed read is silent.
//
// # Why it is checked with the package rather than on its own
//
// `uf check` builds its module map out of the files it is asked to check, and a
// relative import that leaves that set resolves to an any-typed value — after
// which `TValues` is `any`, every line below passes, and the test would prove
// nothing. So the test runs `uf check tests/type-tests packages/form`, with
// both in one set. `anchoring.js` beside this says the rest of why the fixtures
// live here rather than inside the package they are about.
//
// # What is checked at the call, and what is checked at the use
//
// The two are different and the difference is deliberate, so both appear below.
//
// `getValues`, `setValue`, `watch` and `getFieldState` are fields on an object,
// so their types are intersections of one signature per path length, and each
// segment is *bounded* by the keys of what the segment before it landed on. A
// bound is checked at the call: `getValues("address", "country")` is an error
// wherever its result goes.
//
// `useWatch` is a hook, and a hook declaration has one signature. Its `path` is
// a single generic tuple and the value is computed by a conditional type, which
// Flow evaluates against the type the result is wanted at — so a misspelt
// segment there makes the read `mixed`, and `mixed` is refused at the use. Both
// are better than `mixed` everywhere; only one of them names the segment.

import * as React from "@uniflowed/react";

import { useForm, useWatch } from "../../packages/form/index.js";

export component Checked() {
  // Nothing here is annotated. `TValues` is inferred from this object, and
  // every read below is inferred from `TValues`.
  const { getValues, setValue, watch, getFieldState, control } = useForm({
    defaultValues: {
      email: "",
      address: { city: "", zip: 0 },
      items: [{ price: 0, tags: [""] }],
    },
  });

  // --- what must be inferred, at the type the values say -----------------

  const email: string = getValues("email");
  const city: string = getValues("address", "city");
  const price: number = getValues("items", 0, "price");
  const tag: string = getValues("items", 0, "tags", 0);
  const whole: string = getValues().email;
  const watched: number = watch("address", "zip");
  const subscribed: string = useWatch({ control, path: ["address", "city"] });

  // Writing the right type is not an error, and neither is the dotted form —
  // which still means what it meant, and is still `mixed`.
  setValue(["items", 0, "price"], 12);
  setValue(["address", "city"], "Kyoto");
  setValue("items.0.price", 12);
  const dotted: mixed = getValues("items.0.price");
  const dottedWatch: mixed = watch("address.city");
  const several: $ReadOnlyArray<mixed> = watch(["email", "address"]);
  const state: boolean = getFieldState("address", "city").isDirty;
  const dottedState: boolean = getFieldState("address.city").isDirty;

  // --- what must be refused ---------------------------------------------

  // A field's type is the type the values gave it, not the one the call wants.
  // The `unknown` is the dotted arm: `"email"` is a key, so the first arm gave
  // `string`, `string` is not a `number`, and the fallback was tried and is
  // `mixed`. That is what the checker is describing when it says so.
  // expect: unknown is incompatible with number
  const wrongRead: number = getValues("email");

  // Two segments deep, and still the values' answer rather than the call's.
  // expect: number is incompatible with string
  const wrongNested: string = getValues("address", "zip");

  // A segment that is not a key of what the segment before it landed on.
  // expect: property country is missing
  const misspelled: mixed = getValues("address", "country");

  // Through an array index, which is the case a bare `$Keys` cannot do at all.
  // expect: number is incompatible with string
  const wrongIndexed: string = getValues("items", 0, "price");

  // The same, at the fourth segment — the depth this is capped at.
  // expect: string is incompatible with number
  const wrongDeep: number = getValues("items", 0, "tags", 0);

  // Writing the wrong type into a field, which no read can tell you about.
  // expect: "cheap" is incompatible with number
  setValue(["items", 0, "price"], "cheap");

  // expect: 3 is incompatible with string
  setValue(["address", "city"], 3);

  // And writing to a field that is not there.
  // expect: property country is missing
  setValue(["address", "country"], "JP");

  // `watch` carries the same bounds as `getValues`, beside its five old shapes.
  // expect: unknown is incompatible with number
  const wrongWatch: number = watch("email");

  // expect: property country is missing
  const misspelledWatch: mixed = watch("address", "country");

  // `getFieldState` has no value type to get wrong, so what it checks is the
  // path — which is the answer to reading a nested field's error with the
  // checker's help, since the errors themselves stay one flat map.
  // expect: property country is missing
  const misspelledState: mixed = getFieldState("address", "country");

  // `useWatch`'s `path` is a conditional rather than a bound, so this is where
  // it is caught: the read is the wrong type here…
  // expect: number is incompatible with string
  const wrongSubscribed: string = useWatch({ control, path: ["address", "zip"] });

  // …and `mixed` here, because "cty" is not a key and the conditional falls
  // through to the floor rather than naming it.
  // expect: unknown is incompatible with string
  const misspelledSubscribed: string = useWatch({ control, path: ["address", "cty"] });

  return (
    <output>
      {String([
        email,
        city,
        price,
        tag,
        whole,
        watched,
        subscribed,
        dotted,
        dottedWatch,
        several,
        state,
        dottedState,
        wrongRead,
        wrongNested,
        misspelled,
        wrongIndexed,
        wrongDeep,
        wrongWatch,
        misspelledWatch,
        misspelledState,
        wrongSubscribed,
        misspelledSubscribed,
      ])}
    </output>
  );
}
