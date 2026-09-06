// @flow
//
// `@uniflowed/form/controller`: the field that cannot be uncontrolled.
//
// `register` works by handing a DOM control a `ref` and reading its `value`,
// which is the fast path and covers `input`, `select` and `textarea`. It does
// not cover the components a real application is mostly made of: a date picker,
// a combobox, a rich text editor, a `Select` from a component library. Those
// take a `value` and call an `onChange`, and there is no element to read.
//
// `useController` is the adapter. It subscribes to one field, hands back the
// four props such a component expects, and writes back through the store — so a
// controlled field is a first-class member of the same form, with the same
// rules, the same errors, the same dirty and touched flags.
//
//   component PriceField(control: Control<Order>) {
//     const { field, fieldState } = useController({
//       control,
//       name: "price",
//       rules: { required: "How much?" },
//     });
//     return <MoneyInput {...field} invalid={fieldState.invalid} />;
//   }
//
// # What it costs, and why that is the right trade
//
// A controlled field re-renders on every keystroke — this component, and only
// this component. That is the price of a value that lives in React rather than
// in the DOM, and it is unavoidable: the component being wrapped renders from
// its `value` prop.
//
// Which is exactly why this is a separate hook rather than something `register`
// grew an option for. The cost is real, it is per-field, and it should be paid
// deliberately by the fields that need it — not by a form that happened to
// enable a flag.
//
// # Why `field.ref` still exists
//
// So that a failed submit can move focus to the field. The wrapped component
// may or may not forward it to something focusable; when it does, `setFocus`
// and the focus-first-error behaviour work for controlled fields too, and when
// it does not, nothing breaks — the store simply has no element to focus.
// Attaching it does not make the field uncontrolled: the store prefers the
// value it already holds over anything an element reports.

import * as React from "@uniflowed/react";
import { useCallback, useMemo, useSyncExternalStore } from "@uniflowed/react";

import type { ValidationRules } from "./rules.js";
import type { FieldPath, FieldValues } from "./internal/field-path.js";
import type { Control } from "./internal/form-store.js";
import { useFormState, useWatch } from "./watch.js";

/** The props a controlled component is handed. */
export type ControlledField = {|
  readonly name: string,
  readonly value: mixed,
  readonly onChange: (value: mixed) => void,
  readonly onBlur: () => void,
  readonly ref: (element: mixed) => void,
  /**
   * Whether the field is switched off, by its own option or by the form's.
   *
   * Spread onto whatever the caller is wrapping, the way `register` puts it on
   * an `input`. It is here rather than left to the caller because the field
   * does not know about `useForm({ disabled })` and this does.
   *
   * Both halves are current: this field's own option because it is recorded
   * during this render, and the form's because `useForm` leaves it in the store
   * during its own — which is earlier in the same pass. So a form switched off
   * while it saves reaches a controlled field in the commit that switched it
   * off, exactly as it reaches `register`, and neither has to be told twice.
   */
  readonly disabled: boolean,
|};

/** What is currently true of the field, for rendering its state. */
export type ControlledFieldState = {|
  readonly invalid: boolean,
  readonly isDirty: boolean,
  readonly isTouched: boolean,
  readonly error: mixed,
|};

export type UseControllerOptions<TValues extends FieldValues, TOutput> = {|
  readonly control: Control<TValues, TOutput>,
  readonly name: FieldPath,
  readonly rules?: ValidationRules,
  readonly defaultValue?: mixed,
  /**
   * Switch this field off. The form's own `disabled` switches it off too, and
   * neither overrides the other: either one is enough.
   */
  readonly disabled?: boolean,
|};

export type UseControllerReturn = {|
  readonly field: ControlledField,
  readonly fieldState: ControlledFieldState,
|};

const NO_RULES: ValidationRules = Object.freeze({});

/**
 * Bind one field to a component that owns its own value.
 *
 * `onChange` takes the value, not an event — because the components this exists
 * for hand back a `Date`, an option object or a number, and unwrapping
 * `event.target.value` is exactly the thing they are not doing. An event is
 * still accepted, because a caller who wraps a plain `<input>` with this should
 * not have to think about it.
 */
export hook useController<TValues extends FieldValues, TOutput>(
  options: UseControllerOptions<TValues, TOutput>,
): UseControllerReturn {
  const control = options.control;
  const name = options.name;
  const own = options.rules ?? NO_RULES;
  // `disabled` is an option here and a rule there, and this is where the two
  // meet: `rulesFor` is the store's one channel for both, so the option is
  // folded in rather than given a second one. A new object only when the option
  // was passed, so the ordinary controlled field records the rules it was given.
  const rules = options.disabled == null ? own : { ...own, disabled: options.disabled };

  // The same write `register` makes, for the same reason: rules are not part of
  // any snapshot, and recording them again records the same thing.
  control.rulesFor(name, rules);

  // Read through a subscription rather than called, and both halves of that
  // matter.
  //
  // Called — `const disabled = control.isDisabled(name)` — it is a call whose
  // function and arguments the React Compiler can see never change, so the
  // compiler is entitled to cache its result and does: the field would report
  // the answer from its first render for the rest of its life, and a form
  // switched off later would never reach it at all. `useForm` has the same
  // hazard with `watch` and answers it the same way, by making the read
  // something the compiler cannot hold on to. A hook call is that.
  //
  // Subscribed, it also wakes: `configure` invalidates the form state when the
  // flag moves, which is what re-renders a controlled field whose form was
  // switched off from somewhere other than its own parent's render.
  const isDisabled = useCallback(() => control.isDisabled(name), [control, name]);
  const disabled = useSyncExternalStore(control.subscribeFormState, isDisabled, isDisabled);

  const value = useWatch({ control, name, defaultValue: options.defaultValue });
  const state = useFormState({ control, name });

  const onChange = useCallback(
    (next: mixed) => {
      const unwrapped: mixed =
        next != null && typeof next === "object" && (next as $FlowFixMe).target != null
          ? (next as $FlowFixMe).target.value
          : next;
      control.setValue(name, unwrapped, { shouldDirty: true, shouldValidate: false });
      control.handleControlledChange(name);
    },
    [control, name],
  );

  const onBlur = useCallback(() => {
    control.handleBlur(name);
  }, [control, name]);

  const ref = useCallback(
    (element: mixed) => {
      if (element != null) {
        control.attach(name, element);
      }
    },
    [control, name],
  );

  const field = useMemo(
    () => ({ name, value, onChange, onBlur, ref, disabled }),
    [name, value, onChange, onBlur, ref, disabled],
  );

  const fieldState = useMemo(
    () => ({
      invalid: state.errors[name] != null,
      isDirty: state.dirtyFields[name] === true,
      isTouched: state.touchedFields[name] === true,
      error: state.errors[name],
    }),
    [state, name],
  );

  return useMemo(() => ({ field, fieldState }), [field, fieldState]);
}

/**
 * `useController` as a component, for a render prop.
 *
 * The same hook, for the places where a hook cannot go: a list of fields built
 * from a configuration object, where each entry needs its own subscription and
 * a loop cannot call a hook.
 */
export component Controller<TValues extends FieldValues, TOutput = TValues>(
  control: Control<TValues, TOutput>,
  name: FieldPath,
  rules?: ValidationRules,
  defaultValue?: mixed,
  disabled?: boolean,
  render: (bound: UseControllerReturn) => React.Node,
) {
  const bound = useController({ control, name, rules, defaultValue, disabled });
  return render(bound);
}
