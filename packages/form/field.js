// @flow
//
// The join between a form and `@uniflowed/ui`'s `Field`.
//
// Both packages knew how to make a control accessible and neither knew about
// the other, so both produced `aria-invalid` and `aria-describedby` and the
// later spread won — ubugeeei-prod/uf#297. `Field` was described by the form's
// message *or* by `Field.Description`, depending on the order the two objects
// were spread in, and every application using both wrote
// `invalid={errors.email != null}` by hand and then got to choose which of the
// two attributes survived.
//
// One hook ends it, and the division of labour it implements is the one each
// package can keep:
//
//   * The form knows whether the field is wrong, whether it is required, what
//     the message says, and how the control has to be bound to the store.
//   * `Field` knows the ids, because it owns the label and the description, and
//     it is the only thing that can compose them into one token list.
//
// So this returns *facts and a binding*, and no `aria-*` at all:
//
//   component EmailField(form: UseFormReturn<Signup>) {
//     const email = useFieldSource(form, "email", { required: "We need one" });
//     return (
//       <Field.Root field={email}>
//         <Field.Label>Email address</Field.Label>
//         <Field.Control render={(props) => <input type="email" {...props} />} />
//         <Field.Description>We will not share it.</Field.Description>
//         <Field.Error />
//       </Field.Root>
//     );
//   }
//
// `aria-describedby` on that input names the description *and* the message, and
// `Field.Error` is filled by the store rather than by the caller reaching back
// into `formState.errors` for a string this hook is already carrying.
//
// # Why this lives here and not in `@uniflowed/ui`
//
// Because the dependency can only run one way. `@uniflowed/ui` is on npm and
// `@uniflowed/form` is not — its name has never been bound (#210) — and
// `tools/ci/publishable.sh` refuses a published package that depends on an
// unpublished one, because `npm install @uniflowed/ui` would then answer
// `ETARGET`. A `Field.Root name="email"` that looked the form up in a context
// would need exactly that dependency. So `@uniflowed/ui` states the shape it
// accepts and this package, which *may* depend on it, produces one.
//
// The import of `FieldSource` is a type and nothing else, and it is still a real
// dependency rather than a development one. `uf_lib`'s
// `every_uniflowed_import_is_declared` is right about why: a published package
// whose types do not resolve is a package whose types do not resolve, and a
// consumer running `uf check` against a `devDependency` they never installed gets
// `Schema`-shaped nothing — which is #146, already paid for once.
//
// What it costs is an application that uses `@uniflowed/form` with markup of its
// own installing `@uniflowed/ui` as well. The alternative is one type declared in
// two packages, which is the drift this repository argues against everywhere else,
// and it would be a worse trade: a structural match that holds until somebody
// changes one of them.
//
// # The edge reads backwards, and stays until #210
//
// ubugeeei-prod/uf#614 is right that a headless form store depending on a
// component library is the wrong direction, and that `docs/architecture.md`
// asks for the two to be independently consumable. The decision recorded there
// is to keep the edge and hold it to a *type*: Flow erases this import, so
// nothing of `@uniflowed/ui` is loaded, bundled or run by a project that
// installs this package — the cost is an entry in `package.json` so `uf check`
// can resolve it, and not a component library in a form store's runtime. The
// line that would make it a real dependency is a *value* crossing, and
// `tests/library/published-packages.test.js` fails on one.
//
// The trigger to reverse it is #210. Once `@uniflowed/form` is on npm,
// `publishable.sh` allows `ui → form`, and `FieldSource` belongs in the package
// that produces it rather than the one that consumes it.
//
// # The subscription is this component's, not the form's
//
// `useFormState({ control, name })` rather than reading `form.formState`. A form
// with forty fields where every message read the form's own snapshot would
// re-render all forty on every keystroke in any of them; subscribing to the one
// name is what `watch.js` exists for, and it is why a message can live at the
// bottom of a deep tree without the form knowing where it is.
//
// # What is checked about the name, and what is not
//
// `name` is a `FieldSegment<TValues>` — one key of the values, so `"emial"` is
// a type error at the call rather than a field that silently never has an
// error. A path *into* a nested object or a field array is not spelled here:
// the checked form of a deep path is the four-deep intersection `GetFieldState`
// declares, and an intersection of overloads needs a `const` with a type
// annotation, which a `hook` declaration is not. A nested field is bound with
// `register` and `errorProps` today, and widening this is mechanical work that
// has not been done rather than work that cannot be.

"use client";

import type { FieldSource } from "@uniflowed/ui/field";

import type { FieldSegment, FieldValues } from "./internal/field-path.js";
import type { UseFormReturn } from "./use-form.js";
import type { ValidationRules } from "./rules.js";
import { isRequired } from "./rules.js";
import { useFormState } from "./watch.js";

/**
 * The attributes `register` emits that a `Field` computes for itself.
 *
 * The whole of the bug this module fixes, written down as data: these are the
 * names that were produced twice. `register` keeps emitting them, because it is
 * right on its own — an input with no `Field` around it needs them — and they
 * are dropped here, where a `Field` is about to say the same things better.
 */
const FIELD_COMPUTES = ["aria-invalid", "aria-describedby", "aria-required"];

/**
 * Everything a `Field.Root` needs to know about one of this form's fields.
 *
 * `rules` are recorded the same way `register(name, rules)` records them, so a
 * field declared through a `Field` is validated like any other; the store
 * documents why that write during a render is safe.
 */
export hook useFieldSource<
  TValues extends FieldValues,
  TOutput,
  TName extends FieldSegment<TValues>,
>(form: UseFormReturn<TValues, TOutput>, name: TName, rules?: ValidationRules): FieldSource {
  const path: string = name;
  // Narrow: this component re-renders when *this* field's error changes and
  // not when any other field's does.
  const state = useFormState({ control: form.control, name: path });
  const registered = form.register(path, rules);

  const control: { [string]: mixed } = {};
  for (const key of Object.keys(registered)) {
    if (!FIELD_COMPUTES.includes(key)) {
      control[key] = (registered as $FlowFixMe)[key];
    }
  }

  const error = state.errors[path];
  return {
    invalid: error != null,
    // From the rules rather than from the element, so a field that has not been
    // rendered yet still announces that it is required — and so that
    // `aria-required` is one decision made in one place, which is the whole of
    // what this module is for.
    required: isRequired(rules ?? {}),
    disabled: registered.disabled === true,
    busy: state.isSubmitting || state.isValidating || state.isLoading,
    message: error?.message ?? null,
    control,
  };
}
