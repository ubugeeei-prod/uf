// @flow
//
// `@uniflowed/form`.
//
// These test the claims the library is built on rather than the API surface: a
// keystroke that renders nothing, a watcher that wakes for one field and not
// another, a mode that stays quiet until it should not, a stale async answer
// that loses to a newer one, and a field array whose keys survive a removal
// from the middle. A test of "register returns an object with a name" would
// pass while every one of those was broken.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";

import * as React from "@uniflowed/react";
import { StrictMode, useEffect, useState } from "@uniflowed/react";
import { describe, expect, fn, it } from "@uniflowed/test";
import { act, fireEvent, render, screen, userEvent, waitFor } from "@uniflowed/react-testing";
import { email, minLength, object, pipe, string, transform } from "@uniflowed/validator";
import {
  Controller,
  FormProvider,
  useController,
  useFieldArray,
  useForm,
  useFormContext,
  useFormState,
  useWatch,
  validatorResolver,
} from "@uniflowed/form";
import type {
  Control,
  FieldErrors,
  FieldPath,
  FieldState,
  FormState,
  Mode,
  Resolver,
  ValidationRules,
} from "@uniflowed/form";

import { controlIn, elementIn, elementsIn, valueIn } from "./dom.js";

const submitForm = (container: Element) => {
  fireEvent.submit(elementIn(container, "form"));
};

const settle = () => act(() => Promise.resolve());

describe("register: the keystroke that renders nothing", () => {
  it("does not re-render the form while the user types", async () => {
    let renders = 0;
    component Probe() {
      renders += 1;
      const { register } = useForm({ defaultValues: { email: "", note: "" } });
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          <input aria-label="note" {...register("note")} />
        </form>
      );
    }

    render(<Probe />);
    expect(renders).toBe(1);

    // The first keystroke turns `isDirty` on, which is a real change to the
    // form state this component is reading: one render, and only one.
    await userEvent.type(screen.getByLabelText("email"), "hello");
    expect(renders).toBe(2);

    // Every keystroke after it is free. Six more characters, no renders.
    await userEvent.type(screen.getByLabelText("email"), " world");
    expect(renders).toBe(2);
  });

  it("renders per observable transition, not per keystroke", async () => {
    // What the two renders above are, spelled out — because "not per keystroke"
    // is the claim, and the alternative reading is "never", which is not true.
    let renders = 0;
    component Probe() {
      renders += 1;
      const { register, formState } = useForm({ defaultValues: { email: "", note: "" } });
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          <input aria-label="note" {...register("note")} />
          <output>{String(formState.isDirty)}</output>
        </form>
      );
    }

    render(<Probe />);
    await userEvent.type(screen.getByLabelText("email"), "hello");
    expect(renders).toBe(2); // mount, then pristine to dirty

    // Moving to the second field: one render for the blur that marks the first
    // field visited, one for the second field becoming dirty. Then nothing.
    await userEvent.type(screen.getByLabelText("note"), "a note");
    expect(renders).toBe(4);

    await userEvent.type(screen.getByLabelText("note"), " continued");
    await userEvent.type(screen.getByLabelText("note"), " and continued");
    expect(renders).toBe(4);
  });

  it("costs a render per character when the same form is controlled", async () => {
    // The comparison that makes the number above mean something.
    let renders = 0;
    component Controlled() {
      renders += 1;
      const [value, setValue] = useState("");
      return (
        <input
          aria-label="email"
          value={value}
          onChange={(event) => setValue(event.target.value)}
        />
      );
    }

    render(<Controlled />);
    await userEvent.type(screen.getByLabelText("email"), "hello world");
    expect(renders).toBe(12);
  });

  it("counts the owner, the watcher and a controlled input over the same six keystrokes", async () => {
    // The table on the forms guide, in one place, because the interesting
    // number is not the smallest of the three: a `useWatch` subscriber renders
    // once per keystroke exactly like the controlled input does. What the
    // subscription changes is *which* component that is — an `<output>` here,
    // and the whole form in the controlled column.
    let ownerRenders = 0;
    let watcherRenders = 0;
    let controlledRenders = 0;

    component TotalView(control: Control<{ title: string }>) {
      watcherRenders += 1;
      const value = useWatch({ control, name: "title", defaultValue: "" });
      return <output>{String(value ?? "")}</output>;
    }
    // Memoised, so a render of the form for its own reasons is not counted as
    // the subscription waking.
    const Total = React.memo(TotalView);

    component Owner() {
      ownerRenders += 1;
      const { register, control } = useForm({ defaultValues: { title: "" } });
      return (
        <form>
          <input aria-label="title" {...register("title")} />
          <Total control={control} />
        </form>
      );
    }

    component Controlled() {
      controlledRenders += 1;
      const [value, setValue] = useState("");
      return (
        <input
          aria-label="controlled"
          value={value}
          onChange={(event) => setValue(event.target.value)}
        />
      );
    }

    // Both in one tree: `render` mounts into one container, so a second call
    // would replace the first rather than stand beside it.
    render(
      <div>
        <Owner />
        <Controlled />
      </div>,
    );

    await userEvent.type(screen.getByLabelText("title"), "abcdef");
    // Read before moving on: leaving the field blurs it, and the blur that
    // marks a field visited is a real change to `formState` and a third render
    // of the owner. Six keystrokes into one field is what the guide's table
    // measures, so the count is taken while the caret is still in it.
    const ownerAfterSix = ownerRenders;
    const watcherAfterSix = watcherRenders;

    await userEvent.type(screen.getByLabelText("controlled"), "abcdef");

    expect(ownerAfterSix).toBe(2); // mount, then pristine to dirty
    expect(watcherAfterSix).toBe(7); // mount, then one per keystroke
    expect(controlledRenders).toBe(7); // the same seven, over the whole component
  });

  it("keeps the values even though nothing rendered", async () => {
    let read: () => mixed = () => ({});
    component Probe() {
      const { register, getValues } = useForm({
        defaultValues: { email: "", nested: { city: "" } },
      });
      read = getValues;
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          <input aria-label="city" {...register("nested.city")} />
        </form>
      );
    }

    render(<Probe />);
    await userEvent.type(screen.getByLabelText("email"), "a@b.com");
    await userEvent.type(screen.getByLabelText("city"), "Kyoto");
    expect(read()).toEqual({ email: "a@b.com", nested: { city: "Kyoto" } });
  });

  it("reads a checkbox, a radio group and a multi-select as their own shapes", async () => {
    let read: () => mixed = () => ({});
    component Probe() {
      const { register, getValues } = useForm({ defaultValues: {} });
      read = getValues;
      return (
        <form>
          <input type="checkbox" aria-label="terms" {...register("terms")} />
          <input type="radio" value="cat" aria-label="cat" {...register("pet")} />
          <input type="radio" value="dog" aria-label="dog" {...register("pet")} />
          <select multiple aria-label="tags" {...register("tags")}>
            <option value="one">one</option>
            <option value="two">two</option>
          </select>
        </form>
      );
    }

    const { container } = render(<Probe />);
    await userEvent.click(screen.getByLabelText("terms"));
    await userEvent.click(screen.getByLabelText("dog"));
    const select = elementIn(container, "select");
    await userEvent.selectOptions(select, ["two"]);

    expect(read()).toEqual({ terms: true, pet: "dog", tags: ["two"] });
  });
});

describe("watch: one field, not the others", () => {
  it("re-renders the form for the watched field and not for another", async () => {
    let renders = 0;
    component Probe() {
      renders += 1;
      const { register, watch } = useForm({ defaultValues: { a: "", b: "" } });
      const a = watch("a");
      return (
        <form>
          <input aria-label="a" {...register("a")} />
          <input aria-label="b" {...register("b")} />
          <output>{String(a)}</output>
        </form>
      );
    }

    const { container } = render(<Probe />);
    const output = elementIn(container, "output");

    // Visit and dirty both fields first. After that neither the touched set nor
    // the dirty set can move again, so what is counted below is the watch and
    // nothing else.
    await userEvent.type(screen.getByLabelText("a"), "1");
    await userEvent.type(screen.getByLabelText("b"), "1");
    await userEvent.click(screen.getByLabelText("a"));
    await userEvent.click(screen.getByLabelText("b"));
    const settled = renders;

    await userEvent.type(screen.getByLabelText("b"), "xyz");
    expect(renders).toBe(settled);

    await userEvent.type(screen.getByLabelText("a"), "abc");
    expect(output.textContent).toBe("1abc");
    // Three characters in the watched field are three renders; three in the
    // unwatched one were none.
    expect(renders).toBe(settled + 3);
  });

  it("re-renders only the component that called useWatch", async () => {
    let formRenders = 0;
    let totalRenders = 0;

    component TotalView(control: Control<{ price: string, note: string }>) {
      totalRenders += 1;
      const price = useWatch({ control, name: "price", defaultValue: "" });
      return <output>{String(price)}</output>;
    }
    // Memoised, so that a render of the form for its own reasons — the blur
    // that marks a field visited, say — is not mistaken for the subscription
    // waking. What is being tested is the subscription.
    const Total = React.memo(TotalView);

    component Probe() {
      formRenders += 1;
      const { register, control } = useForm({ defaultValues: { price: "", note: "" } });
      return (
        <form>
          <input aria-label="price" {...register("price")} />
          <input aria-label="note" {...register("note")} />
          <Total control={control} />
        </form>
      );
    }

    const { container } = render(<Probe />);
    const output = elementIn(container, "output");

    await userEvent.type(screen.getByLabelText("price"), "42");
    expect(output.textContent).toBe("42");
    // Two characters, two renders of the watcher — and the form rendered once,
    // for `isDirty`, not twice more.
    expect(totalRenders).toBe(3);
    expect(formRenders).toBe(2);

    const before = totalRenders;
    await userEvent.type(screen.getByLabelText("note"), "hello");
    expect(totalRenders).toBe(before);
  });

  it("watches a whole subtree, including a change under it", async () => {
    component Probe() {
      const { register, control } = useForm({
        defaultValues: { address: { city: "", street: "" }, other: "" },
      });
      const address = useWatch({ control, name: "address" });
      return (
        <form>
          <input aria-label="city" {...register("address.city")} />
          <output>{JSON.stringify(address)}</output>
        </form>
      );
    }

    const { container } = render(<Probe />);
    const output = elementIn(container, "output");
    await userEvent.type(screen.getByLabelText("city"), "Kyoto");
    expect(JSON.parse(output.textContent)).toEqual({ city: "Kyoto", street: "" });
  });

  it("subscribes a callback without rendering anything", async () => {
    const seen: Array<mixed> = [];
    let renders = 0;
    component Probe() {
      renders += 1;
      const { register, watch } = useForm({ defaultValues: { a: "", b: "" } });
      React.useEffect(() => watch("a", (values) => seen.push(values.a)), [watch]);
      return (
        <form>
          <input aria-label="a" {...register("a")} />
          <input aria-label="b" {...register("b")} />
        </form>
      );
    }

    render(<Probe />);
    // Settle the transitions that have nothing to do with watching: both fields
    // visited, both dirty.
    await userEvent.type(screen.getByLabelText("a"), "hi");
    await userEvent.type(screen.getByLabelText("b"), "n");
    await userEvent.click(screen.getByLabelText("a"));
    await userEvent.click(screen.getByLabelText("b"));
    const settled = renders;

    await userEvent.type(screen.getByLabelText("b"), "o");
    await userEvent.click(screen.getByLabelText("a"));
    await userEvent.type(screen.getByLabelText("a"), "!");

    expect(seen).toEqual(["h", "hi", "hi!"]);
    // The listener saw every change to `a` and none to `b`, and subscribing
    // this way rendered nothing at all.
    expect(renders).toBe(settled);
  });
});

describe("a path given as segments is the same field", () => {
  // The typed half of the API is a claim about the *checker*, and
  // `tests/type-tests/field-paths.js` is where that claim is held. What is left
  // for a running test is the half a type cannot state: that `["items", 0,
  // "price"]` and `"items.0.price"` reach the same place in the same store —
  // because a typed accessor that addressed a field the string form could not
  // would be two forms wearing one name.

  it("reads, writes and subscribes at the same field as the dotted string", async () => {
    let read: () => mixed = () => null;
    let readDotted: () => mixed = () => null;
    component Probe() {
      const { register, getValues, setValue, control } = useForm({
        defaultValues: { items: [{ price: 1 }, { price: 2 }], address: { city: "" } },
      });
      read = () => getValues("items", 1, "price");
      readDotted = () => getValues("items.1.price");
      // The subscription form, in a component that is not the one that owns the
      // form: `path` where `name` would have been.
      return (
        <form>
          <input aria-label="city" {...register("address.city")} />
          <button type="button" onClick={() => setValue(["items", 1, "price"], 99)}>
            Raise
          </button>
          <Row control={control} />
        </form>
      );
    }
    component Row(
      control: Control<{ items: Array<{ price: number }>, address: { city: string } }>,
    ) {
      const price = useWatch({ control, path: ["items", 1, "price"] });
      const city = useWatch({ control, path: ["address", "city"] });
      return <output>{`${String(price)}/${String(city)}`}</output>;
    }

    const { container } = render(<Probe />);
    const output = elementIn(container, "output");
    expect(output.textContent).toBe("2/");
    expect(read()).toBe(2);
    expect(readDotted()).toBe(2);

    // A write through the segment form is visible to a subscription made
    // through the segment form, and to a read made through the dotted one.
    await userEvent.click(screen.getByRole("button", { name: "Raise" }));
    expect(output.textContent).toBe("99/");
    expect(read()).toBe(99);
    expect(readDotted()).toBe(99);

    // And a change made by the user, through the registered dotted name, is
    // visible to the segment subscription.
    await userEvent.type(screen.getByLabelText("city"), "Kyoto");
    expect(output.textContent).toBe("99/Kyoto");
  });

  it("watches a nested field from the form, and getFieldState answers about it", async () => {
    let segments: () => FieldState = () => {
      throw new Error("the probe has not rendered");
    };
    let dotted: () => FieldState = segments;
    component Probe() {
      const { register, watch, getFieldState } = useForm({
        defaultValues: { address: { city: "" } },
      });
      segments = () => getFieldState("address", "city");
      dotted = () => getFieldState("address.city");
      const city = watch("address", "city");
      return (
        <form>
          <input aria-label="city" {...register("address.city")} />
          <output>{String(city)}</output>
        </form>
      );
    }

    const { container } = render(<Probe />);
    expect(segments().isDirty).toBe(false);
    await userEvent.type(screen.getByLabelText("city"), "Osaka");
    // `watch("address", "city")` re-rendered the form and read the same field
    // the dotted `register` wrote.
    expect(elementIn(container, "output").textContent).toBe("Osaka");
    expect(segments().isDirty).toBe(true);
    // And the two spellings are one question: the whole `FieldState`, not just
    // the flag the line above happened to look at.
    expect(segments()).toEqual(dotted());
  });
});

describe("validation modes: quiet until they should not be", () => {
  component Probe(mode: Mode) {
    const { register, handleSubmit, formState, errorProps } = useForm({
      defaultValues: { email: "" },
      mode,
    });
    return (
      <form onSubmit={handleSubmit(() => {})}>
        <input aria-label="email" {...register("email", { required: "Required" })} />
        {formState.errors.email != null && (
          <p {...errorProps("email")}>{formState.errors.email.message}</p>
        )}
        <button type="submit">Save</button>
      </form>
    );
  }

  it("onSubmit says nothing until the form is submitted", async () => {
    const { container } = render(<Probe mode="onSubmit" />);
    const field = screen.getByLabelText("email");

    await userEvent.type(field, "x");
    await userEvent.clear(field);
    await userEvent.tabAway(field);
    expect(screen.queryByRole("alert")).toBe(null);

    submitForm(container);
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toBe("Required");
    });
  });

  it("onChange reports as soon as the value changes", async () => {
    render(<Probe mode="onChange" />);
    const field = screen.getByLabelText("email");
    await userEvent.type(field, "x");
    expect(screen.queryByRole("alert")).toBe(null);
    await userEvent.clear(field);
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });
  });

  it("onBlur says nothing while typing and reports when focus leaves", async () => {
    render(<Probe mode="onBlur" />);
    const field = screen.getByLabelText("email");
    await userEvent.type(field, "x");
    await userEvent.clear(field);
    expect(screen.queryByRole("alert")).toBe(null);
    await userEvent.tabAway(field);
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });
  });

  it("all reports on a change and on a blur", async () => {
    render(<Probe mode="all" />);
    const field = screen.getByLabelText("email");
    await userEvent.type(field, "x");
    await userEvent.clear(field);
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });
    await userEvent.type(field, "y");
    await waitFor(() => {
      expect(screen.queryByRole("alert")).toBe(null);
    });
    await userEvent.tabAway(field);
    expect(screen.queryByRole("alert")).toBe(null);
  });

  it("onTouched waits for the first blur, then reports every change", async () => {
    render(<Probe mode="onTouched" />);
    const field = screen.getByLabelText("email");
    await userEvent.type(field, "x");
    await userEvent.clear(field);
    expect(screen.queryByRole("alert")).toBe(null);

    await userEvent.tabAway(field);
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });

    await userEvent.type(field, "y");
    await waitFor(() => {
      expect(screen.queryByRole("alert")).toBe(null);
    });
  });
});

describe("built-in rules", () => {
  const check = async (rules: ValidationRules, typed: string) => {
    let message = null;
    component Probe() {
      const { register, handleSubmit, formState } = useForm({ defaultValues: { field: "" } });
      message = formState.errors.field?.message ?? null;
      return (
        <form onSubmit={handleSubmit(() => {})}>
          <input aria-label="field" {...register("field", rules)} />
        </form>
      );
    }
    const { container } = render(<Probe />);
    if (typed !== "") {
      await userEvent.type(screen.getByLabelText("field"), typed);
    }
    submitForm(container);
    await settle();
    await settle();
    return message;
  };

  it("required", async () => {
    expect(await check({ required: "Say something" }, "")).toBe("Say something");
    expect(await check({ required: "Say something" }, "a")).toBe(null);
  });

  it("min and max compare on a scale, not as strings", async () => {
    expect(await check({ min: 10 }, "9")).toBe("Must be at least 10");
    expect(await check({ min: 10 }, "11")).toBe(null);
    expect(await check({ max: { value: 10, message: "Too big" } }, "11")).toBe("Too big");
  });

  it("minLength and maxLength", async () => {
    expect(await check({ minLength: 3 }, "ab")).toBe("Must be at least 3 characters");
    expect(await check({ maxLength: 2 }, "abc")).toBe("Must be at most 2 characters");
  });

  it("pattern", async () => {
    const rules = { pattern: { value: /^[a-z]+$/, message: "Letters only" } };
    expect(await check(rules, "ab1")).toBe("Letters only");
    expect(await check(rules, "abc")).toBe(null);
  });

  it("validate, as one function and as several named ones", async () => {
    expect(await check({ validate: (value) => value === "ok" || "Not ok" }, "no")).toBe("Not ok");
    const named = {
      validate: {
        lower: (value: mixed) => String(value) === String(value).toLowerCase() || "Lower case",
        short: (value: mixed) => String(value).length < 4 || "Too long",
      },
    };
    expect(await check(named, "ABC")).toBe("Lower case");
    expect(await check(named, "abcd")).toBe("Too long");
    expect(await check(named, "abc")).toBe(null);
  });

  it("says nothing about length when the field is simply empty", async () => {
    // "must be at least 8 characters" is true of an empty password and is not
    // the thing to tell someone who has not typed anything.
    expect(await check({ minLength: 8 }, "")).toBe(null);
  });
});

describe("handleSubmit", () => {
  const InvalidForm = (
    onValid: (values: mixed, event?: mixed) => mixed,
    onInvalid?: (errors: FieldErrors, event?: mixed) => mixed,
  ) => {
    component Probe() {
      const { register, handleSubmit } = useForm({ defaultValues: { email: "" } });
      return (
        <form onSubmit={handleSubmit(onValid, onInvalid)}>
          <input aria-label="email" {...register("email", { required: "Required" })} />
        </form>
      );
    }
    return <Probe />;
  };

  it("does not call onValid when validation fails, and tells onInvalid what failed", async () => {
    const onValid = fn();
    const onInvalid = fn();
    const { container } = render(InvalidForm(onValid, onInvalid));

    submitForm(container);
    await waitFor(() => {
      expect(onInvalid).toHaveBeenCalled();
    });
    expect(onValid).not.toHaveBeenCalled();
    // Asserted through `objectContaining` rather than read off a cast: a spy
    // hands its arguments back as `mixed`, and `expect.any`-style matchers are
    // how this suite asks a question of one without claiming to know its type.
    const [errors] = onInvalid.mock.calls[0].args;
    expect(errors).toEqual(
      expect.objectContaining({
        email: expect.objectContaining({ message: "Required", type: "required" }),
      }),
    );
  });

  it("calls onValid with the values once they are valid", async () => {
    const onValid = fn();
    const { container } = render(InvalidForm(onValid, undefined));
    await userEvent.type(screen.getByLabelText("email"), "a@b.com");
    submitForm(container);
    await waitFor(() => {
      expect(onValid).toHaveBeenCalled();
    });
    expect(onValid.mock.calls[0].args[0]).toEqual({ email: "a@b.com" });
  });

  it("reports isSubmitting across an async submit, and counts the submit", async () => {
    let release: () => void = () => {};
    const seen: Array<boolean> = [];
    component Probe() {
      const { handleSubmit, formState } = useForm({ defaultValues: {} });
      seen.push(formState.isSubmitting);
      return (
        <form
          onSubmit={handleSubmit(
            () =>
              new Promise<void>((resolve) => {
                release = () => resolve();
              }),
          )}
        >
          <output>{`${String(formState.isSubmitting)}:${String(formState.submitCount)}`}</output>
        </form>
      );
    }

    const { container } = render(<Probe />);
    const output = elementIn(container, "output");

    submitForm(container);
    await waitFor(() => {
      expect(output.textContent).toBe("true:0");
    });

    await act(async () => {
      release();
      await Promise.resolve();
    });
    await waitFor(() => {
      expect(output.textContent).toBe("false:1");
    });
    expect(seen).toContain(true);
  });

  it("clears isSubmitting when the submit throws, and does not swallow the failure", async () => {
    const caught = fn();
    component Probe() {
      const { handleSubmit, formState } = useForm({ defaultValues: {} });
      const submit = handleSubmit(() => {
        throw new Error("the server said no");
      });
      return (
        <form
          onSubmit={(event) => {
            submit(event).catch(caught);
          }}
        >
          <output>{`${String(formState.isSubmitting)}:${String(formState.isSubmitSuccessful)}`}</output>
        </form>
      );
    }

    const { container } = render(<Probe />);
    const output = elementIn(container, "output");
    submitForm(container);

    await waitFor(() => {
      expect(caught).toHaveBeenCalled();
    });
    expect(String(caught.mock.calls[0].args[0])).toContain("the server said no");
    // The button is usable again, and the form did not claim success.
    await waitFor(() => {
      expect(output.textContent).toBe("false:false");
    });
  });

  it("moves focus to the first field with an error", async () => {
    component Probe() {
      const { register, handleSubmit } = useForm({ defaultValues: { first: "ok", second: "" } });
      return (
        <form onSubmit={handleSubmit(() => {})}>
          <input aria-label="first" {...register("first", { required: "Required" })} />
          <input aria-label="second" {...register("second", { required: "Required" })} />
        </form>
      );
    }
    const { container } = render(<Probe />);
    submitForm(container);
    await waitFor(() => {
      expect(screen.getByLabelText("second")).toHaveFocus();
    });
  });
});

describe("accessibility", () => {
  component Probe() {
    const { register, handleSubmit, formState, errorProps } = useForm({
      defaultValues: { email: "" },
    });
    return (
      <form onSubmit={handleSubmit(() => {})}>
        <input aria-label="email" {...register("email", { required: "We need an email" })} />
        {formState.errors.email != null && (
          <p {...errorProps("email")}>{formState.errors.email.message}</p>
        )}
      </form>
    );
  }

  it("says nothing about validity while the field is fine", () => {
    render(<Probe />);
    expect(screen.getByLabelText("email")).not.toHaveAttribute("aria-invalid");
    expect(screen.getByLabelText("email")).not.toHaveAttribute("aria-describedby");
  });

  it("marks the control invalid and points it at the message that is rendered", async () => {
    const { container } = render(<Probe />);
    submitForm(container);
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });

    const control = screen.getByLabelText("email");
    expect(control).toHaveAttribute("aria-invalid", "true");
    const message = screen.getByRole("alert");
    expect(control.getAttribute("aria-describedby")).toBe(message.getAttribute("id"));
  });

  it("announces a required field before it has been checked", () => {
    // `aria-invalid` says a field is wrong after it has been checked.
    // `aria-required` says it is required before the user gets there, which is
    // the announcement that prevents the error rather than reporting it.
    render(<Probe />);
    expect(screen.getByLabelText("email")).toHaveAttribute("aria-required", "true");
  });

  it("does not announce a field that is not required", () => {
    component Optional() {
      const { register } = useForm({ defaultValues: { nickname: "" } });
      return (
        <form>
          <input aria-label="nickname" {...register("nickname", { maxLength: 20 })} />
        </form>
      );
    }
    render(<Optional />);
    expect(screen.getByLabelText("nickname")).not.toHaveAttribute("aria-required");
  });

  it("drops aria-required when the native attribute is there to say it", () => {
    // Exactly one of the two, never both: `required` already announces the
    // field, and a second attribute saying the same thing is noise of the kind
    // `aria-invalid="false"` would be.
    component Progressive() {
      const { register } = useForm({ defaultValues: { email: "" }, progressive: true });
      return (
        <form>
          <input aria-label="email" {...register("email", { required: "We need an email" })} />
        </form>
      );
    }
    render(<Progressive />);
    const control = screen.getByLabelText("email");
    expect(control).toHaveAttribute("required");
    expect(control).not.toHaveAttribute("aria-required");
  });

  it("gives two copies of the same form different ids", async () => {
    const { container } = render(
      <div>
        <Probe />
        <Probe />
      </div>,
    );
    for (const form of elementsIn(container, "form")) {
      fireEvent.submit(form);
    }
    await waitFor(() => {
      expect(screen.getAllByRole("alert").length).toBe(2);
    });
    const [first, second] = screen.getAllByRole("alert");
    expect(first.getAttribute("id")).not.toBe(second.getAttribute("id"));
  });
});

describe("reset", () => {
  it("puts the defaults back, in the store and in the DOM, and forgets what happened", async () => {
    // The latest render's state, kept behind an accessor so that reading it
    // before anything rendered is a failure that says so rather than a
    // `property of null`.
    let state: FormState | null = null;
    let read: () => mixed = () => null;
    const stateOf = (): FormState => {
      if (state == null) {
        throw new Error("the probe has not rendered");
      }
      return state;
    };
    component Probe() {
      const { register, reset, formState, getValues } = useForm({
        defaultValues: { email: "start" },
      });
      state = formState;
      read = getValues;
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          <button type="button" onClick={() => reset()}>
            Reset
          </button>
        </form>
      );
    }

    render(<Probe />);
    const field = screen.getByLabelText("email");
    await userEvent.type(field, "!");
    await userEvent.tabAway(field);

    expect(stateOf().isDirty).toBe(true);
    expect(stateOf().touchedFields.email).toBe(true);

    await userEvent.click(screen.getByRole("button", { name: "Reset" }));

    expect(valueIn(field)).toBe("start");
    expect(read()).toEqual({ email: "start" });
    expect(stateOf().isDirty).toBe(false);
    expect(stateOf().dirtyFields).toEqual({});
    expect(stateOf().touchedFields).toEqual({});
    expect(stateOf().submitCount).toBe(0);
  });

  it("takes new defaults, which is what a loaded record needs", async () => {
    let read: () => mixed = () => ({});
    component Probe() {
      const { register, reset, getValues } = useForm({ defaultValues: { email: "" } });
      read = getValues;
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          <button type="button" onClick={() => reset({ email: "loaded@example.com" })}>
            Load
          </button>
        </form>
      );
    }

    render(<Probe />);
    await userEvent.click(screen.getByRole("button", { name: "Load" }));
    expect(valueIn(screen.getByLabelText("email"))).toBe("loaded@example.com");
    expect(read()).toEqual({ email: "loaded@example.com" });
  });

  it("keeps the fields the user edited and replaces the rest", async () => {
    let state: FormState<{ name: string, note: string }> | null = null;
    let read: () => mixed = () => ({});
    component Probe() {
      const { register, reset, getValues, formState } = useForm({
        defaultValues: { name: "old", note: "old" },
      });
      state = formState;
      read = getValues;
      return (
        <form>
          <input aria-label="name" {...register("name")} />
          <input aria-label="note" {...register("note")} />
          <button
            type="button"
            onClick={() => reset({ name: "server", note: "server" }, { keepDirtyValues: true })}
          >
            Sync
          </button>
        </form>
      );
    }

    render(<Probe />);
    await userEvent.clear(screen.getByLabelText("name"));
    await userEvent.type(screen.getByLabelText("name"), "mine");
    await userEvent.click(screen.getByRole("button", { name: "Sync" }));

    // The field being typed into keeps what was typed. The other one takes what
    // arrived, and the control shows it.
    expect(read()).toEqual({ name: "mine", note: "server" });
    expect(valueIn(screen.getByLabelText("name"))).toBe("mine");
    expect(valueIn(screen.getByLabelText("note"))).toBe("server");
    // And the kept field is still dirty, because it still disagrees with what
    // a reset would now go back to.
    if (state == null) {
      throw new Error("the probe has not rendered");
    }
    expect(state.dirtyFields).toEqual({ name: true });
    expect(state.defaultValues).toEqual({ name: "server", note: "server" });
  });

  it("recomputes which kept fields are still dirty against the values that arrived", async () => {
    // The other half of `keepDirtyValues`, and the one that decides whether
    // `isDirty` still means anything after a re-seed. Two edited fields and one
    // record: the field the record agrees with is not unsaved work any more,
    // and the field it disagrees with still is.
    let state: FormState<{ name: string, note: string }> | null = null;
    let read: () => mixed = () => ({});
    component Probe() {
      const { register, reset, getValues, formState } = useForm({
        defaultValues: { name: "old", note: "old" },
      });
      state = formState;
      read = getValues;
      return (
        <form>
          <input aria-label="name" {...register("name")} />
          <input aria-label="note" {...register("note")} />
          <button
            type="button"
            onClick={() => reset({ name: "mine", note: "server" }, { keepDirtyValues: true })}
          >
            Sync
          </button>
        </form>
      );
    }

    render(<Probe />);
    await userEvent.clear(screen.getByLabelText("name"));
    await userEvent.type(screen.getByLabelText("name"), "mine");
    await userEvent.clear(screen.getByLabelText("note"));
    await userEvent.type(screen.getByLabelText("note"), "edited");
    if (state == null) {
      throw new Error("the probe has not rendered");
    }
    expect(state.dirtyFields).toEqual({ name: true, note: true });

    await userEvent.click(screen.getByRole("button", { name: "Sync" }));

    // Both edits survive, because both fields were dirty.
    expect(read()).toEqual({ name: "mine", note: "edited" });
    // `name` now says what the record says, so it is not an unsaved change.
    // `note` still disagrees with it, so it is.
    expect(state.dirtyFields).toEqual({ note: true });
    expect(state.isDirty).toBe(true);
  });
});

describe("values that come from outside the form", () => {
  // ubugeeei-prod/uf#294. Everything here is the case the library could not
  // serve before it: the values are not known when the component first renders,
  // or they change afterwards.

  it("follows a values object when its identity changes", async () => {
    let read: () => mixed = () => ({});
    let state: FormState<{ email: string, name: string }> | null = null;
    component Probe(record: { email: string, name: string }) {
      const { register, getValues, formState } = useForm({
        defaultValues: { email: "", name: "" },
        values: record,
      });
      read = getValues;
      state = formState;
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          <input aria-label="name" {...register("name")} />
        </form>
      );
    }

    const first = { email: "a@example.com", name: "Ada" };
    const { rerender } = render(<Probe record={first} />);
    // Seeded on the first render, not one commit later: no empty frame.
    expect(read()).toEqual(first);
    expect(valueIn(screen.getByLabelText("email"))).toBe("a@example.com");
    // And the first seed moves the defaults, exactly as every one after it
    // does — otherwise `reset()` would go back to a form that never existed.
    if (state == null) {
      throw new Error("the probe has not rendered");
    }
    expect(state.defaultValues).toEqual(first);

    // The same object again is not news, and re-seeding on it would throw away
    // whatever the user had done since.
    await userEvent.clear(screen.getByLabelText("name"));
    await userEvent.type(screen.getByLabelText("name"), "typed");
    rerender(<Probe record={first} />);
    expect(read()).toEqual({ email: "a@example.com", name: "typed" });

    // A different record is.
    const second = { email: "b@example.com", name: "Bea" };
    rerender(<Probe record={second} />);
    expect(read()).toEqual(second);
    expect(valueIn(screen.getByLabelText("name"))).toBe("Bea");
  });

  it("does not re-seed for an object that is new but says the same thing", async () => {
    let read: () => mixed = () => ({});
    component Probe(email: string) {
      const { register, getValues } = useForm({
        defaultValues: { email: "", note: "" },
        // A literal written inline: a different object on every render, which
        // is what a caller who has not thought about identity will write.
        values: { email, note: "" },
      });
      read = getValues;
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          <input aria-label="note" {...register("note")} />
        </form>
      );
    }

    const { rerender } = render(<Probe email="a@example.com" />);
    await userEvent.type(screen.getByLabelText("note"), "kept");
    rerender(<Probe email="a@example.com" />);
    // Nothing about the values changed, so nothing was thrown away.
    expect(read()).toEqual({ email: "a@example.com", note: "kept" });
  });

  it("drops a field the new values no longer contain", async () => {
    let read: () => mixed = () => ({});
    component Probe(record: { email: string, nickname?: string }) {
      const { register, getValues } = useForm({ defaultValues: { email: "" }, values: record });
      read = getValues;
      return (
        <form>
          <input aria-label="email" {...register("email")} />
        </form>
      );
    }

    const { rerender } = render(<Probe record={{ email: "a@example.com", nickname: "ada" }} />);
    expect(read()).toEqual({ email: "a@example.com", nickname: "ada" });

    // The tree is replaced rather than merged, so a key the caller stopped
    // sending is a key the form stops holding — and a submit stops sending.
    rerender(<Probe record={{ email: "a@example.com" }} />);
    expect(read()).toEqual({ email: "a@example.com" });
  });

  it("keeps what the user typed when the record is re-sent, with keepDirtyValues", async () => {
    let read: () => mixed = () => ({});
    component Probe(record: { email: string, name: string }) {
      const { register, getValues } = useForm({
        defaultValues: { email: "", name: "" },
        values: record,
        resetOptions: { keepDirtyValues: true },
      });
      read = getValues;
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          <input aria-label="name" {...register("name")} />
        </form>
      );
    }

    const { rerender } = render(<Probe record={{ email: "a@example.com", name: "Ada" }} />);
    await userEvent.clear(screen.getByLabelText("name"));
    await userEvent.type(screen.getByLabelText("name"), "mine");

    rerender(<Probe record={{ email: "b@example.com", name: "Bea" }} />);
    expect(read()).toEqual({ email: "b@example.com", name: "mine" });
  });

  it("loads an asynchronous default without a second component or a wrong first render", async () => {
    let renders = 0;
    let read: () => mixed = () => ({});
    let state: FormState<{ email: string }> | null = null;
    let settleFetch: (values: { email: string }) => void = () => {};
    const record = new Promise<{ email: string }>((resolve) => {
      settleFetch = resolve;
    });

    component Probe() {
      renders += 1;
      const { register, getValues, formState } = useForm({ defaultValues: () => record });
      read = getValues;
      state = formState;
      return (
        <form>
          <input aria-label="email" {...register("email")} />
        </form>
      );
    }

    render(<Probe />);
    if (state == null) {
      throw new Error("the probe has not rendered");
    }
    // The first render is honest rather than absent: empty, not dirty, and
    // saying so.
    expect(state.isLoading).toBe(true);
    expect(state.isDirty).toBe(false);
    expect(read()).toEqual({});
    expect(renders).toBe(1);

    await act(async () => {
      settleFetch({ email: "loaded@example.com" });
      await record;
    });

    expect(state.isLoading).toBe(false);
    expect(read()).toEqual({ email: "loaded@example.com" });
    expect(valueIn(screen.getByLabelText("email"))).toBe("loaded@example.com");
    expect(state.defaultValues).toEqual({ email: "loaded@example.com" });
    // One render for the values arriving, and one only.
    expect(renders).toBe(2);
  });

  it("does not overwrite what the user typed while the default was still loading", async () => {
    let read: () => mixed = () => ({});
    let settleFetch: (values: { email: string, note: string }) => void = () => {};
    const record = new Promise<{ email: string, note: string }>((resolve) => {
      settleFetch = resolve;
    });

    component Probe() {
      const { register, getValues } = useForm({ defaultValues: () => record });
      read = getValues;
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          <input aria-label="note" {...register("note")} />
        </form>
      );
    }

    render(<Probe />);
    await userEvent.type(screen.getByLabelText("note"), "mine");

    await act(async () => {
      settleFetch({ email: "loaded@example.com", note: "server" });
      await record;
    });

    // The fetch is slower than the user, and the user wins the field they were
    // in. Everything else lands.
    expect(read()).toEqual({ email: "loaded@example.com", note: "mine" });
    expect(valueIn(screen.getByLabelText("note"))).toBe("mine");
    expect(valueIn(screen.getByLabelText("email"))).toBe("loaded@example.com");
  });

  it("lets a values record stand while a slower default becomes what a reset goes back to", async () => {
    // Both options at once is a caller saying two different things: here is the
    // record, and here is what the form should go back to. They are answered
    // separately rather than the later one winning.
    let read: () => mixed = () => ({});
    let state: FormState<{ email: string }> | null = null;
    let settleFetch: (values: { email: string }) => void = () => {};
    const original = new Promise<{ email: string }>((resolve) => {
      settleFetch = resolve;
    });

    component Probe() {
      const { register, getValues, formState } = useForm({
        defaultValues: () => original,
        values: { email: "draft@example.com" },
      });
      read = getValues;
      state = formState;
      return (
        <form>
          <input aria-label="email" {...register("email")} />
        </form>
      );
    }

    render(<Probe />);
    expect(read()).toEqual({ email: "draft@example.com" });

    await act(async () => {
      settleFetch({ email: "saved@example.com" });
      await original;
    });

    if (state == null) {
      throw new Error("the probe has not rendered");
    }
    expect(read()).toEqual({ email: "draft@example.com" });
    expect(state.defaultValues).toEqual({ email: "saved@example.com" });
    expect(state.isLoading).toBe(false);
  });

  it("drops a default that resolves after somebody said what the values are", async () => {
    let read: () => mixed = () => ({});
    let settleFetch: (values: { email: string }) => void = () => {};
    const record = new Promise<{ email: string }>((resolve) => {
      settleFetch = resolve;
    });

    component Probe() {
      const { register, getValues, reset } = useForm({ defaultValues: () => record });
      read = getValues;
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          <button type="button" onClick={() => reset({ email: "chosen@example.com" })}>
            Choose
          </button>
        </form>
      );
    }

    render(<Probe />);
    // Empty and loading, not holding the thunk it was handed.
    expect(read()).toEqual({});

    await userEvent.click(screen.getByRole("button", { name: "Choose" }));
    await act(async () => {
      settleFetch({ email: "loaded@example.com" });
      await record;
    });

    // The same stale-answer rule the resolver runs on, applied to values: an
    // answer to a question nobody is asking any more does not land.
    expect(read()).toEqual({ email: "chosen@example.com" });
  });

  it("does not re-apply an errors map that is new but says the same thing", async () => {
    // The same identity-then-content rule `values` follows, and here it is the
    // difference between a feature and a render loop: an errors map written
    // inline is a new object *and* a new `FieldError` on every render, and
    // applying one notifies, which renders, which arrives back at the same
    // comparison.
    let renders = 0;
    component Probe(message: string) {
      renders += 1;
      const { register, formState } = useForm({
        defaultValues: { email: "" },
        errors: { email: { type: "server", message } },
      });
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          {formState.errors.email != null && <p role="alert">{formState.errors.email.message}</p>}
        </form>
      );
    }

    const { rerender } = render(<Probe message="Already registered" />);
    expect(screen.getByRole("alert").textContent).toBe("Already registered");
    const settled = renders;

    rerender(<Probe message="Already registered" />);
    // The rerender itself is one render, and the store added none to it.
    expect(renders).toBe(settled + 1);
    expect(screen.getByRole("alert").textContent).toBe("Already registered");

    rerender(<Probe message="That address is in use" />);
    expect(screen.getByRole("alert").textContent).toBe("That address is in use");
  });

  it("takes a map of errors as an input and clears it on the next successful submit", async () => {
    const onValid = fn();
    component Probe(errors: FieldErrors) {
      const { register, handleSubmit, formState } = useForm({
        defaultValues: { email: "" },
        errors,
      });
      return (
        <form onSubmit={handleSubmit(onValid)}>
          <input aria-label="email" {...register("email")} />
          {formState.errors.email != null && <p role="alert">{formState.errors.email.message}</p>}
        </form>
      );
    }

    const rejected: FieldErrors = { email: { type: "server", message: "Already registered" } };
    const { container, rerender } = render(<Probe errors={rejected} />);
    // On screen from the first render, without a loop at the call site and
    // without an effect.
    expect(screen.getByRole("alert").textContent).toBe("Already registered");

    rerender(<Probe errors={{}} />);
    expect(screen.queryByRole("alert")).toBe(null);

    rerender(<Probe errors={rejected} />);
    expect(screen.getByRole("alert").textContent).toBe("Already registered");

    // A submit that passes is the form's own verdict about the same field, and
    // it replaces the server's.
    await act(async () => {
      submitForm(container);
    });
    await settle();
    expect(screen.queryByRole("alert")).toBe(null);
    expect(onValid).toHaveBeenCalledTimes(1);
  });
});

describe("setValue, setError, clearErrors and trigger", () => {
  it("writes a value through to an uncontrolled control", async () => {
    component Probe() {
      const { register, setValue } = useForm({ defaultValues: { email: "" } });
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          <button type="button" onClick={() => setValue("email", "set@example.com")}>
            Set
          </button>
        </form>
      );
    }
    render(<Probe />);
    await userEvent.click(screen.getByRole("button", { name: "Set" }));
    expect(valueIn(screen.getByLabelText("email"))).toBe("set@example.com");
  });

  it("shows a manual error and clears it again", async () => {
    component Probe() {
      const { register, setError, clearErrors, formState } = useForm({
        defaultValues: { email: "" },
      });
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          <button type="button" onClick={() => setError("email", { message: "Already taken" })}>
            Fail
          </button>
          <button type="button" onClick={() => clearErrors("email")}>
            Clear
          </button>
          {formState.errors.email != null && <p role="alert">{formState.errors.email.message}</p>}
        </form>
      );
    }
    render(<Probe />);
    await userEvent.click(screen.getByRole("button", { name: "Fail" }));
    expect(screen.getByRole("alert").textContent).toBe("Already taken");
    await userEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(screen.queryByRole("alert")).toBe(null);
  });

  it("validates on demand and answers whether it passed", async () => {
    const answers: Array<boolean> = [];
    component Probe() {
      const { register, trigger, formState } = useForm({ defaultValues: { email: "" } });
      return (
        <form>
          <input aria-label="email" {...register("email", { required: "Required" })} />
          <button
            type="button"
            onClick={() => {
              void trigger("email").then((ok) => answers.push(ok));
            }}
          >
            Check
          </button>
          {formState.errors.email != null && <p role="alert">{formState.errors.email.message}</p>}
        </form>
      );
    }
    render(<Probe />);
    await userEvent.click(screen.getByRole("button", { name: "Check" }));
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });
    expect(answers).toEqual([false]);
  });
});

describe("async validation", () => {
  it("does not let a slow answer land on top of a newer one", async () => {
    const delays: { [string]: number } = { first: 60, second: 5 };
    component Probe() {
      const { register, formState } = useForm({
        defaultValues: { name: "" },
        mode: "onChange",
      });
      return (
        <form>
          <input
            aria-label="name"
            {...register("name", {
              validate: async (value) => {
                const key = String(value) === "first" ? "first" : "second";
                await new Promise((resolve) => setTimeout(resolve, delays[key]));
                return String(value) === "second" || `${String(value)} is taken`;
              },
            })}
          />
          {formState.errors.name != null && <p role="alert">{formState.errors.name.message}</p>}
        </form>
      );
    }

    render(<Probe />);
    const field = controlIn(screen.getByLabelText("name"));

    // "first" is slow and wrong; "second" is fast and right. Without the
    // sequence stamp the slow answer arrives last and wins.
    await act(async () => {
      field.value = "first";
      fireEvent.input(field);
    });
    await act(async () => {
      field.value = "second";
      fireEvent.input(field);
    });

    await act(() => new Promise((resolve) => setTimeout(resolve, 120)));
    expect(screen.queryByRole("alert")).toBe(null);
  });

  it("stops reporting isValidating for a pass a reset threw away", async () => {
    // A promise cannot be cancelled, so a reset that clears `isValidating`
    // cannot clear what is in flight. It disowns it — and the half that is
    // easy to get wrong is what happens when the disowned pass finishes
    // *after* a newer one has started: it must not turn the flag off under it.
    const settlers: Array<() => void> = [];
    component Probe() {
      const { register, reset, formState } = useForm({
        defaultValues: { name: "" },
        mode: "onChange",
      });
      return (
        <form>
          <input
            aria-label="name"
            {...register("name", {
              validate: async () => {
                await new Promise<void>((resolve) => settlers.push(resolve));
                return true;
              },
            })}
          />
          <output>{String(formState.isValidating)}</output>
          <button type="button" onClick={() => reset()}>
            Reset
          </button>
        </form>
      );
    }

    const { container } = render(<Probe />);
    const output = elementIn(container, "output");
    const settle = async (at: number) => {
      await act(async () => {
        settlers[at]();
        await Promise.resolve();
      });
    };

    // An eager mode primes every field once on mount, so the first pass is
    // that one. Let it finish before the interesting part begins.
    expect(settlers.length).toBe(1);
    await settle(0);
    await waitFor(() => {
      expect(output.textContent).toBe("false");
    });

    await userEvent.type(screen.getByLabelText("name"), "a");
    expect(output.textContent).toBe("true");
    expect(settlers.length).toBe(2);

    // The reset drops the flag even though that pass is still running.
    await userEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(output.textContent).toBe("false");

    // A third pass starts, and then the disowned one finishes. The flag
    // belongs to the third now, and the second must not turn it off.
    await userEvent.type(screen.getByLabelText("name"), "b");
    expect(output.textContent).toBe("true");
    expect(settlers.length).toBe(3);

    await settle(1);
    expect(output.textContent).toBe("true");

    await settle(2);
    await waitFor(() => {
      expect(output.textContent).toBe("false");
    });
  });

  it("reports isValidating while an async check is pending", async () => {
    component Probe() {
      const { register, formState } = useForm({ defaultValues: { name: "" }, mode: "onChange" });
      return (
        <form>
          <input
            aria-label="name"
            {...register("name", {
              validate: async () => {
                await new Promise((resolve) => setTimeout(resolve, 30));
                return true;
              },
            })}
          />
          <output>{String(formState.isValidating)}</output>
        </form>
      );
    }

    const { container } = render(<Probe />);
    const output = elementIn(container, "output");
    expect(output.textContent).toBe("false");
    await userEvent.type(screen.getByLabelText("name"), "x");
    expect(output.textContent).toBe("true");
    await waitFor(() => {
      expect(output.textContent).toBe("false");
    });
  });
});

describe("useFieldArray", () => {
  component Rows() {
    const { register, control } = useForm({
      defaultValues: { items: [{ name: "a" }, { name: "b" }, { name: "c" }] },
    });
    const { fields, append, prepend, insert, remove, swap, move, update } = useFieldArray({
      control,
      name: "items",
    });
    return (
      <form>
        <ul>
          {fields.map((field, index) => (
            <li key={field.id} data-key={field.id}>
              <input aria-label={`row ${index}`} {...register(`items.${index}.name`)} />
            </li>
          ))}
        </ul>
        <button type="button" onClick={() => remove(1)}>
          Remove middle
        </button>
        <button type="button" onClick={() => append({ name: "d" })}>
          Append
        </button>
        <button type="button" onClick={() => prepend({ name: "z" })}>
          Prepend
        </button>
        <button type="button" onClick={() => insert(1, { name: "i" })}>
          Insert
        </button>
        <button type="button" onClick={() => swap(0, 2)}>
          Swap
        </button>
        <button type="button" onClick={() => move(0, 2)}>
          Move
        </button>
        <button type="button" onClick={() => update(1, { name: "updated" })}>
          Update
        </button>
      </form>
    );
  }

  const keysIn = (container: Element): $ReadOnlyArray<string> =>
    elementsIn(container, "li").map((node) => String(node.getAttribute("data-key")));
  const textIn = (container: Element): $ReadOnlyArray<string> =>
    elementsIn(container, "input").map(valueIn);

  it("keeps the keys of the rows that stayed when one is removed from the middle", async () => {
    const { container } = render(<Rows />);
    const before = keysIn(container);
    expect(textIn(container)).toEqual(["a", "b", "c"]);

    await userEvent.click(screen.getByRole("button", { name: "Remove middle" }));

    const after = keysIn(container);
    expect(after).toEqual([before[0], before[2]]);
    // Which is the whole point: React kept the right DOM nodes, so the
    // uncontrolled inputs still show the rows they belong to.
    expect(textIn(container)).toEqual(["a", "c"]);
  });

  it("keeps what the user typed into a row that shifts up", async () => {
    const { container } = render(<Rows />);
    await userEvent.clear(screen.getByLabelText("row 2"));
    await userEvent.type(screen.getByLabelText("row 2"), "typed");
    await userEvent.click(screen.getByRole("button", { name: "Remove middle" }));
    expect(textIn(container)).toEqual(["a", "typed"]);
  });

  it("appends, prepends and inserts with new keys and leaves the rest alone", async () => {
    const { container } = render(<Rows />);
    const start = keysIn(container);

    await userEvent.click(screen.getByRole("button", { name: "Append" }));
    expect(textIn(container)).toEqual(["a", "b", "c", "d"]);
    expect(keysIn(container).slice(0, 3)).toEqual(start);

    await userEvent.click(screen.getByRole("button", { name: "Prepend" }));
    expect(textIn(container)).toEqual(["z", "a", "b", "c", "d"]);
    expect(keysIn(container).slice(1, 4)).toEqual(start);

    await userEvent.click(screen.getByRole("button", { name: "Insert" }));
    expect(textIn(container)).toEqual(["z", "i", "a", "b", "c", "d"]);
  });

  it("swaps and moves rows, and the keys travel with the values", async () => {
    const { container } = render(<Rows />);
    const start = keysIn(container);

    await userEvent.click(screen.getByRole("button", { name: "Swap" }));
    expect(textIn(container)).toEqual(["c", "b", "a"]);
    expect(keysIn(container)).toEqual([start[2], start[1], start[0]]);

    await userEvent.click(screen.getByRole("button", { name: "Move" }));
    expect(textIn(container)).toEqual(["b", "a", "c"]);
    expect(keysIn(container)).toEqual([start[1], start[0], start[2]]);
  });

  it("replaces a row's values but not its identity", async () => {
    const { container } = render(<Rows />);
    const start = keysIn(container);
    await userEvent.click(screen.getByRole("button", { name: "Update" }));
    expect(textIn(container)).toEqual(["a", "updated", "c"]);
    expect(keysIn(container)).toEqual(start);
  });

  it("moves the error with the row it belongs to", async () => {
    component WithErrors() {
      const { register, control, handleSubmit, formState } = useForm({
        defaultValues: { items: [{ name: "a" }, { name: "" }, { name: "c" }] },
      });
      const { fields, remove } = useFieldArray({ control, name: "items" });
      return (
        <form onSubmit={handleSubmit(() => {})}>
          {fields.map((field, index) => (
            <div key={field.id}>
              <input
                aria-label={`row ${index}`}
                {...register(`items.${index}.name`, { required: "Required" })}
              />
            </div>
          ))}
          <output>{Object.keys(formState.errors).join(",")}</output>
          <button type="button" onClick={() => remove(0)}>
            Drop first
          </button>
          <button type="submit">Save</button>
        </form>
      );
    }

    const { container } = render(<WithErrors />);
    const output = elementIn(container, "output");
    submitForm(container);
    await waitFor(() => {
      expect(output.textContent).toBe("items.1.name");
    });

    await userEvent.click(screen.getByRole("button", { name: "Drop first" }));
    // The empty row is now row 0, and so is its error.
    expect(output.textContent).toBe("items.0.name");
  });
});

describe("cross-field rules and re-validation", () => {
  component Passwords() {
    const { register, handleSubmit, formState } = useForm({
      defaultValues: { password: "", confirm: "" },
      mode: "onChange",
    });
    return (
      <form onSubmit={handleSubmit(() => {})}>
        <input aria-label="password" {...register("password", { deps: ["confirm"] })} />
        <input
          aria-label="confirm"
          {...register("confirm", {
            validate: (value, values) => value === values.password || "They do not match",
          })}
        />
        {formState.errors.confirm != null && <p role="alert">{formState.errors.confirm.message}</p>}
      </form>
    );
  }

  it("re-checks a dependent field when the field it depends on changes", async () => {
    render(<Passwords />);
    await userEvent.type(screen.getByLabelText("password"), "hunter2");
    await userEvent.type(screen.getByLabelText("confirm"), "hunter");
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });

    // Fixing the *password* is what makes the confirmation right, and `deps`
    // is what makes the confirmation notice.
    await userEvent.clear(screen.getByLabelText("password"));
    await userEvent.type(screen.getByLabelText("password"), "hunter");
    await waitFor(() => {
      expect(screen.queryByRole("alert")).toBe(null);
    });
  });

  it("says nothing until a submit, and then re-checks on every change", async () => {
    component Probe() {
      const { register, handleSubmit, formState } = useForm({
        defaultValues: { email: "" },
        mode: "onSubmit",
      });
      return (
        <form onSubmit={handleSubmit(() => {})}>
          <input aria-label="email" {...register("email", { required: "Required" })} />
          {formState.errors.email != null && <p role="alert">{formState.errors.email.message}</p>}
        </form>
      );
    }

    const { container } = render(<Probe />);
    await userEvent.type(screen.getByLabelText("email"), "x");
    await userEvent.clear(screen.getByLabelText("email"));
    expect(screen.queryByRole("alert")).toBe(null);

    submitForm(container);
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });

    // `reValidateMode` defaults to `onChange`, so once the form has been
    // submitted the error goes as soon as the field is right.
    await userEvent.type(screen.getByLabelText("email"), "a@b.com");
    await waitFor(() => {
      expect(screen.queryByRole("alert")).toBe(null);
    });
  });
});

describe("unregister, getFieldState and setFocus", () => {
  it("forgets a field's value and its state", async () => {
    let read: () => mixed = () => ({});
    component Probe() {
      const { register, unregister, getValues } = useForm({ defaultValues: { a: "", b: "" } });
      read = getValues;
      return (
        <form>
          <input aria-label="a" {...register("a")} />
          <input aria-label="b" {...register("b")} />
          <button type="button" onClick={() => unregister("a")}>
            Forget a
          </button>
        </form>
      );
    }

    render(<Probe />);
    await userEvent.type(screen.getByLabelText("a"), "one");
    await userEvent.type(screen.getByLabelText("b"), "two");
    await userEvent.click(screen.getByRole("button", { name: "Forget a" }));
    expect(read()).toEqual({ b: "two" });
  });

  it("answers what is true of one field", async () => {
    let state: (name: FieldPath) => FieldState = () => {
      throw new Error("the probe has not rendered");
    };
    component Probe() {
      const { register, getFieldState } = useForm({ defaultValues: { a: "start" } });
      state = getFieldState;
      return (
        <form>
          <input aria-label="a" {...register("a")} />
        </form>
      );
    }

    render(<Probe />);
    expect(state("a")).toEqual({
      invalid: false,
      isDirty: false,
      isTouched: false,
      error: undefined,
    });

    await userEvent.type(screen.getByLabelText("a"), "!");
    await userEvent.tabAway(screen.getByLabelText("a"));
    const after = state("a");
    expect(after.isDirty).toBe(true);
    expect(after.isTouched).toBe(true);
  });

  it("moves focus where it is told to", async () => {
    component Probe() {
      const { register, setFocus } = useForm({ defaultValues: { a: "", b: "" } });
      return (
        <form>
          <input aria-label="a" {...register("a")} />
          <input aria-label="b" {...register("b")} />
          <button type="button" onClick={() => setFocus("b")}>
            Go to b
          </button>
        </form>
      );
    }
    render(<Probe />);
    await userEvent.click(screen.getByRole("button", { name: "Go to b" }));
    expect(screen.getByLabelText("b")).toHaveFocus();
  });
});

describe("the validator resolver", () => {
  const account = object({
    email: pipe(string(), email()),
    profile: object({ city: pipe(string(), minLength(2)) }),
    age: pipe(string(), transform(Number)),
  });

  // Written out because they are the point of these tests: what the form holds
  // is all strings, and what the schema hands `onValid` has `age` as a number.
  // `Resolver<TIn, TOut>` is generic in both for exactly that reason, and
  // `validatorResolver` cannot infer `TIn` from a schema — it describes the
  // output — so the caller says it.
  type AccountValues = { email: string, profile: { city: string }, age: string };
  type AccountOutput = { email: string, profile: { city: string }, age: number };

  component Probe(onValid: (values: AccountOutput, event?: mixed) => mixed) {
    const { register, handleSubmit, formState } = useForm<AccountValues, AccountOutput>({
      defaultValues: { email: "", profile: { city: "" }, age: "0" },
      resolver: validatorResolver<AccountValues, AccountOutput>(account),
    });
    return (
      <form onSubmit={handleSubmit(onValid)}>
        <input aria-label="email" {...register("email")} />
        <input aria-label="city" {...register("profile.city")} />
        <input aria-label="age" {...register("age")} />
        <output>{Object.keys(formState.errors).sort().join(",")}</output>
      </form>
    );
  }

  it("turns schema issues into field errors at the path the field is registered at", async () => {
    const { container } = render(<Probe onValid={() => {}} />);
    const output = elementIn(container, "output");

    submitForm(container);
    await waitFor(() => {
      expect(output.textContent).toBe("email,profile.city");
    });
  });

  it("hands onValid the schema's output, not the form's input", async () => {
    const onValid = fn();
    const { container } = render(<Probe onValid={onValid} />);

    await userEvent.type(screen.getByLabelText("email"), "a@b.com");
    await userEvent.type(screen.getByLabelText("city"), "Kyoto");
    await userEvent.clear(screen.getByLabelText("age"));
    await userEvent.type(screen.getByLabelText("age"), "42");

    submitForm(container);
    await waitFor(() => {
      expect(onValid).toHaveBeenCalled();
    });
    // `age` was `"42"` in the form and is `42` here, because the schema said so.
    expect(onValid.mock.calls[0].args[0]).toEqual({
      email: "a@b.com",
      profile: { city: "Kyoto" },
      age: 42,
    });
  });

  it("clears a field's error as soon as the schema accepts it", async () => {
    component Eager() {
      const { register, formState } = useForm<AccountValues, AccountOutput>({
        defaultValues: { email: "", profile: { city: "ok" }, age: "1" },
        resolver: validatorResolver<AccountValues, AccountOutput>(account),
        mode: "onChange",
      });
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          <output>{formState.errors.email?.message ?? "none"}</output>
        </form>
      );
    }

    const { container } = render(<Eager />);
    const output = elementIn(container, "output");
    await userEvent.type(screen.getByLabelText("email"), "nope");
    await waitFor(() => {
      expect(output.textContent).not.toBe("none");
    });
    await userEvent.type(screen.getByLabelText("email"), "@example.com");
    await waitFor(() => {
      expect(output.textContent).toBe("none");
    });
  });
});

describe("useController and Controller", () => {
  component Money(value: mixed, onChange: (value: mixed) => void, invalid: boolean) {
    return (
      <input
        aria-label="amount"
        aria-invalid={invalid ? "true" : undefined}
        value={String(value ?? "")}
        onChange={(event) => onChange(event.target.value)}
      />
    );
  }

  it("binds a component that owns its own value", async () => {
    let read: () => mixed = () => ({});
    component Probe() {
      const form = useForm({ defaultValues: { amount: "" }, mode: "onChange" });
      read = form.getValues;
      const { field, fieldState } = useController({
        control: form.control,
        name: "amount",
        rules: { required: "How much?" },
      });
      return (
        <form>
          <Money value={field.value} onChange={field.onChange} invalid={fieldState.invalid} />
        </form>
      );
    }

    render(<Probe />);
    const field = screen.getByLabelText("amount");
    await userEvent.type(field, "12");
    expect(valueIn(field)).toBe("12");
    expect(read()).toEqual({ amount: "12" });
  });

  it("reports the controlled field's own error", async () => {
    component Probe() {
      const form = useForm({ defaultValues: { amount: "5" }, mode: "onChange" });
      return (
        <form>
          <Controller
            control={form.control}
            name="amount"
            rules={{ required: "How much?" }}
            render={({ field, fieldState }) => (
              <Money value={field.value} onChange={field.onChange} invalid={fieldState.invalid} />
            )}
          />
        </form>
      );
    }

    render(<Probe />);
    const field = screen.getByLabelText("amount");
    await userEvent.clear(field);
    await waitFor(() => {
      expect(screen.getByLabelText("amount")).toHaveAttribute("aria-invalid", "true");
    });
  });
});

describe("disabled", () => {
  it("puts the attribute on the control the render that switched it off", async () => {
    // Not one render later. A form disabled while it saves has to *be*
    // disabled while it saves, and the store learns the flag from an effect —
    // so `register` is handed it directly instead.
    component Probe() {
      const [saving, setSaving] = useState(false);
      const { register } = useForm({ defaultValues: { email: "" }, disabled: saving });
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          <button type="button" onClick={() => setSaving(true)}>
            Save
          </button>
        </form>
      );
    }

    render(<Probe />);
    expect(screen.getByLabelText("email")).not.toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(screen.getByLabelText("email")).toBeDisabled();
  });

  it("switches off one field without switching off the form", () => {
    component Probe() {
      const { register } = useForm({ defaultValues: { email: "", code: "" } });
      return (
        <form>
          <input aria-label="email" {...register("email")} />
          <input aria-label="code" {...register("code", { disabled: true })} />
        </form>
      );
    }
    render(<Probe />);
    expect(screen.getByLabelText("email")).not.toBeDisabled();
    expect(screen.getByLabelText("code")).toBeDisabled();
  });

  it("leaves a disabled field out of the values a submit hands over", async () => {
    // The rule that makes `disabled` mean something: a value the user was never
    // shown must not be sent as though they had agreed to it. `getValues()`
    // still answers with it, because that question is "what does the form
    // hold".
    const onValid = fn();
    let read: () => mixed = () => null;
    component Probe() {
      const { register, handleSubmit, getValues } = useForm({
        defaultValues: { email: "a@b.com", code: "secret" },
      });
      read = getValues;
      return (
        <form onSubmit={handleSubmit(onValid)}>
          <input aria-label="email" {...register("email")} />
          <input aria-label="code" {...register("code", { disabled: true })} />
        </form>
      );
    }

    const { container } = render(<Probe />);
    submitForm(container);
    await waitFor(() => {
      expect(onValid).toHaveBeenCalled();
    });
    expect(onValid.mock.calls[0].args[0]).toEqual({ email: "a@b.com" });
    expect(read()).toEqual({ email: "a@b.com", code: "secret" });
  });

  it("does not validate a disabled field, and validates it again once it is back", async () => {
    component Probe(off: boolean) {
      const { register, trigger, formState } = useForm({ defaultValues: { code: "" } });
      return (
        <form>
          <input aria-label="code" {...register("code", { required: "Required", disabled: off })} />
          <button
            type="button"
            onClick={() => {
              void trigger();
            }}
          >
            Check the form
          </button>
          <button
            type="button"
            onClick={() => {
              void trigger("code");
            }}
          >
            Check the field
          </button>
          <output>{formState.errors.code?.message ?? "no error"}</output>
        </form>
      );
    }

    const view = render(<Probe off={true} />);
    await userEvent.click(screen.getByRole("button", { name: "Check the form" }));
    // A field the user cannot answer must not be able to stop them submitting.
    expect(screen.getByText("no error")).toBeInTheDocument();

    // Named directly rather than reached through `trigger()`, which is the
    // path that does not go past `liveNames`.
    await userEvent.click(screen.getByRole("button", { name: "Check the field" }));
    expect(screen.getByText("no error")).toBeInTheDocument();

    view.rerender(<Probe off={false} />);
    await userEvent.click(screen.getByRole("button", { name: "Check the form" }));
    await waitFor(() => {
      expect(screen.getByText("Required")).toBeInTheDocument();
    });
  });

  it("forgets the error a field earned before it was switched off", async () => {
    // Otherwise nothing would ever re-check it and `handleSubmit` — which
    // refuses while any error stands — would be blocked forever.
    const onValid = fn();
    component Probe(off: boolean) {
      const { register, handleSubmit } = useForm({ defaultValues: { code: "" } });
      return (
        <form onSubmit={handleSubmit(onValid)}>
          <input aria-label="code" {...register("code", { required: "Required", disabled: off })} />
        </form>
      );
    }

    const view = render(<Probe off={false} />);
    submitForm(view.container);
    await waitFor(() => {
      expect(screen.getByLabelText("code")).toHaveAttribute("aria-invalid", "true");
    });

    view.rerender(<Probe off={true} />);
    submitForm(view.container);
    await waitFor(() => {
      expect(onValid).toHaveBeenCalled();
    });
  });

  it("does not let a programmatic write to a disabled field make the form dirty", async () => {
    component Probe() {
      const { register, setValue, formState } = useForm({ defaultValues: { code: "start" } });
      return (
        <form>
          <input aria-label="code" {...register("code", { disabled: true })} />
          <button
            type="button"
            onClick={() => {
              setValue("code", "changed", { shouldDirty: true });
            }}
          >
            Fill it in
          </button>
          <output>{`dirty: ${String(formState.isDirty)}`}</output>
        </form>
      );
    }

    render(<Probe />);
    await userEvent.click(screen.getByRole("button", { name: "Fill it in" }));
    // A write to a field the user cannot reach is not the user changing it.
    expect(screen.getByText("dirty: false")).toBeInTheDocument();
  });

  it("reports the form's own flag in formState, not a summary of its fields", () => {
    component Probe(off: boolean) {
      const { register, formState } = useForm({ defaultValues: { code: "" }, disabled: off });
      return (
        <form>
          <input aria-label="code" {...register("code", { disabled: true })} />
          <output>{`form: ${String(formState.disabled)}`}</output>
        </form>
      );
    }

    const view = render(<Probe off={false} />);
    // One disabled field is not a disabled form.
    expect(screen.getByText("form: false")).toBeInTheDocument();
    view.rerender(<Probe off={true} />);
    // The store is told from an effect, which `rerender` has already flushed.
    expect(screen.getByText("form: true")).toBeInTheDocument();
  });

  it("keeps a disabled field out of the values a resolver hands back", async () => {
    // A resolver's output is a type of its own, not a subset of the form's
    // values — that is what `Resolver<TIn, TOut>` is generic in both for. So
    // pruning the input is not enough to keep a disabled field out of the
    // submission: a schema with a default for the field that went missing puts
    // it straight back, and `handleSubmit` hands the resolver's answer to
    // `onValid` verbatim.
    const onValid = fn();
    type Coded = { email: string, code: string };
    // No `errors` key rather than an empty one: `ResolverResult` is an exact
    // union so that checking `errors` narrows `values`, and `errorsOf` reads a
    // missing `errors` as none — which is what an empty object meant.
    const withADefault: Resolver<Coded, Coded> = (values) => ({
      values: { ...values, code: "filled in by the schema" },
    });
    component Probe() {
      const { register, handleSubmit } = useForm<Coded, Coded>({
        defaultValues: { email: "a@b.com", code: "never shown" },
        resolver: withADefault,
      });
      return (
        <form onSubmit={handleSubmit(onValid)}>
          <input aria-label="email" {...register("email")} />
          <input aria-label="code" {...register("code", { disabled: true })} />
        </form>
      );
    }

    const { container } = render(<Probe />);
    submitForm(container);
    await waitFor(() => {
      expect(onValid).toHaveBeenCalled();
    });
    expect(onValid.mock.calls[0].args[0]).toEqual({ email: "a@b.com" });
  });

  it("tells a controlled field the form was switched off in the commit that switched it", async () => {
    // The same claim the `register` test at the top of this describe makes,
    // for the hook that cannot be handed the flag: a form disabled while it
    // saves has to reach a controlled field on the render that disabled it.
    //
    // Not one commit later, and — before this was a subscription — not ever:
    // `control.isDisabled(name)` called in a render body is a call whose
    // function and arguments the React Compiler can see never change, so it
    // cached the first render's answer and the field stayed enabled for good.
    const commits = [];
    component Probe() {
      const [saving, setSaving] = useState(false);
      const { control } = useForm({ defaultValues: { colour: "red" }, disabled: saving });
      const { field } = useController({ control, name: "colour" });
      const off = field.disabled;
      useEffect(() => {
        commits.push(`saving=${String(saving)} disabled=${String(off)}`);
      });
      return (
        <form>
          <output>{`colour: ${String(off)}`}</output>
          <button type="button" onClick={() => setSaving(true)}>
            Save
          </button>
        </form>
      );
    }

    render(<Probe />);
    expect(screen.getByText("colour: false")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(screen.getByText("colour: true")).toBeInTheDocument();
    // And no commit in between drew an enabled field into a form being saved.
    expect(commits).not.toContain("saving=true disabled=false");
  });

  it("tells a controlled field it is switched off", () => {
    component Probe() {
      const { control } = useForm({ defaultValues: { colour: "red", size: "M" } });
      const colour = useController({ control, name: "colour", disabled: true });
      const size = useController({ control, name: "size" });
      return (
        <form>
          <output>{`colour: ${String(colour.field.disabled)}`}</output>
          <output>{`size: ${String(size.field.disabled)}`}</output>
        </form>
      );
    }

    render(<Probe />);
    expect(screen.getByText("colour: true")).toBeInTheDocument();
    expect(screen.getByText("size: false")).toBeInTheDocument();
  });

  it("leaves a disabled controlled field out of the submitted values", async () => {
    const onValid = fn();
    component Probe() {
      const { control, handleSubmit } = useForm({ defaultValues: { colour: "red", size: "M" } });
      useController({ control, name: "colour", disabled: true });
      useController({ control, name: "size" });
      return <form onSubmit={handleSubmit(onValid)} />;
    }

    const { container } = render(<Probe />);
    submitForm(container);
    await waitFor(() => {
      expect(onValid).toHaveBeenCalled();
    });
    expect(onValid.mock.calls[0].args[0]).toEqual({ size: "M" });
  });
});

/** As much of `react-dom/server` as this file uses. */
type ReactDomServer = {| readonly renderToStaticMarkup: (node: React.Node) => string |};

describe("server rendering", () => {
  // Loaded the way `@uniflowed/react-testing` loads `react-dom/client`: through
  // a synchronous require, so a test file that never renders on the server does
  // not pay for the module.
  //
  // The annotation is the trust boundary, and it is one line wide for the
  // reason `@uniflowed/react-testing` gives for its own: a synchronous require
  // of a CommonJS build answers `any` whatever anyone writes, so the choice is
  // between saying what is expected of the module and saying nothing.
  const server: ReactDomServer = createRequire(import.meta.url)("react-dom/server");

  component Probe() {
    const { register, formState, errorProps } = useForm({
      defaultValues: { email: "someone@example.com" },
    });
    return (
      <form>
        <input aria-label="email" {...register("email", { required: "Required" })} />
        {formState.errors.email != null && (
          <p {...errorProps("email")}>{formState.errors.email.message}</p>
        )}
        <output>{String(formState.isDirty)}</output>
      </form>
    );
  }

  it("renders where there is no DOM, from the same snapshots", async () => {
    // `useSyncExternalStore` asks for a server snapshot separately, and a
    // library that had none — or whose server snapshot differed from the first
    // client one — would either throw here or hydrate into a mismatch. Both of
    // this package's snapshots are pure reads of values the store already has,
    // so the server's answer is the client's first answer.
    const markup = String(server.renderToStaticMarkup(<Probe />));
    expect(markup).toContain("<form>");
    expect(markup).toContain('name="email"');
    // `isDirty` is false on the server and false on the first client render.
    expect(markup).toContain("<output>false</output>");
    // Nothing has been submitted, so nothing is invalid and nothing is
    // described by a message that is not in the document.
    expect(markup).not.toContain("aria-invalid");
    expect(markup).not.toContain("aria-describedby");

    // And what is *not* there: the value. `register` gives an input a `ref`,
    // not a `value`, and a ref does not run on a server — so `defaultValues`
    // alone puts nothing in the HTML. The documented way to server-render a
    // value is the next test: put it in the markup, and the store adopts it.
    expect(markup).not.toContain("someone@example.com");
  });

  it("carries no constraints into the markup unless it was asked to", () => {
    // The default, and the reason `progressive` exists as a flag: emitting
    // `pattern` gives the browser a second opinion about the same regular
    // expression, and a form should not acquire that without asking.
    const markup = String(server.renderToStaticMarkup(<Probe />));
    expect(markup).not.toContain('required=""');
    expect(markup).not.toContain("pattern=");
    // What it does carry is the announcement, which enforces nothing.
    expect(markup).toContain('aria-required="true"');
  });

  it("carries a progressive form's constraints into the markup, unhydrated", () => {
    // The claim this exists for: a page a user submits before the JavaScript
    // arrives is refused by the browser rather than accepted by the server.
    component Constrained() {
      const { register } = useForm({
        defaultValues: { email: "", age: "", nickname: "" },
        progressive: true,
      });
      return (
        <form>
          <input
            aria-label="email"
            {...register("email", { required: "Required", pattern: /.+@.+/ })}
          />
          <input aria-label="age" {...register("age", { min: 18, max: 120 })} />
          <input
            aria-label="nickname"
            {...register("nickname", {
              minLength: 2,
              maxLength: { value: 20, message: "Too long" },
            })}
          />
        </form>
      );
    }

    // HTML attribute names are case-insensitive, so React's `minLength` is the
    // `minlength` attribute as far as a browser parsing this is concerned.
    const markup = String(server.renderToStaticMarkup(<Constrained />));
    expect(markup).toContain('required=""');
    expect(markup).toContain('pattern=".+@.+"');
    expect(markup).toContain('min="18"');
    expect(markup).toContain('max="120"');
    expect(markup).toContain('minLength="2"');
    expect(markup).toContain('maxLength="20"');
    // Enforced by the browser, so the ARIA copy of the same fact is not sent.
    expect(markup).not.toContain("aria-required");
  });

  it("carries a disabled field into the markup either way", () => {
    // Not progressive enhancement: a form that is switched off has to render
    // switched off, whether or not the browser is being asked to validate it.
    component Off() {
      const { register } = useForm({ defaultValues: { email: "" }, disabled: true });
      return (
        <form>
          <input aria-label="email" {...register("email")} />
        </form>
      );
    }
    expect(String(server.renderToStaticMarkup(<Off />))).toContain("disabled=");
  });

  it("adopts the value the server put in the markup once it mounts", async () => {
    let read: () => mixed = () => ({});
    component Adopting() {
      const { register, getValues } = useForm({ defaultValues: {} });
      read = getValues;
      return (
        <form>
          <input aria-label="email" defaultValue="from-markup" {...register("email")} />
        </form>
      );
    }

    render(<Adopting />);
    // A form written as markup rather than as `defaultValues` still has values
    // before anybody types, which is what makes a server-rendered page's first
    // interaction behave the same as a client-rendered one's.
    expect(read()).toEqual({ email: "from-markup" });
  });
});

describe("React semantics", () => {
  it("survives Strict Mode's second render without registering anything twice", async () => {
    let read: () => mixed = () => ({});
    component Probe() {
      const { register, getValues } = useForm({ defaultValues: { email: "" } });
      read = getValues;
      return (
        <form>
          <input aria-label="email" {...register("email", { required: "Required" })} />
        </form>
      );
    }

    render(
      <StrictMode>
        <Probe />
      </StrictMode>,
    );
    await userEvent.type(screen.getByLabelText("email"), "hi");
    expect(read()).toEqual({ email: "hi" });
    expect(screen.getAllByLabelText("email").length).toBe(1);
  });

  it("does not re-render a memoised child when an unrelated field changes", async () => {
    let childRenders = 0;
    const Child = React.memo(function Child(props: { readonly label: string }) {
      childRenders += 1;
      return <span>{props.label}</span>;
    });

    component Probe() {
      const { register } = useForm({ defaultValues: { a: "", b: "" } });
      return (
        <form>
          <input aria-label="a" {...register("a")} />
          <input aria-label="b" {...register("b")} />
          <Child label="steady" />
        </form>
      );
    }

    render(<Probe />);
    expect(childRenders).toBe(1);
    await userEvent.type(screen.getByLabelText("a"), "abc");
    await userEvent.type(screen.getByLabelText("b"), "def");
    // The form rendered once, for `isDirty`; the memoised child rendered not
    // at all, because its props never moved.
    expect(childRenders).toBe(1);
  });

  it("reaches the form from a descendant through FormProvider", async () => {
    component Field() {
      const { register } = useFormContext();
      return <input aria-label="email" {...register("email", { required: "Required" })} />;
    }

    component Message(control: Control<{ email: string }>) {
      const { errors } = useFormState({ control, name: "email" });
      return errors.email == null ? null : <p role="alert">{errors.email.message}</p>;
    }

    component Probe() {
      const form = useForm({ defaultValues: { email: "" } });
      return (
        <FormProvider form={form}>
          <form onSubmit={form.handleSubmit(() => {})}>
            <Field />
            <Message control={form.control} />
          </form>
        </FormProvider>
      );
    }

    const { container } = render(<Probe />);
    submitForm(container);
    await waitFor(() => {
      expect(screen.getByRole("alert").textContent).toBe("Required");
    });
  });

  it("keeps a field array right across renders it did not cause", async () => {
    // The regression this is here for: the row snapshot used to be a `useMemo`
    // over a read of the store, and the React Compiler — which uf runs over
    // every `component` and `hook` — held that memo for ever, because nothing
    // it could see said the store had changed. The list then rendered the
    // surviving rows under the removed row's keys. Rendering for an unrelated
    // reason first is what makes a stale memo visible.
    component Probe() {
      const [tick, setTick] = useState(0);
      const { register, control } = useForm({
        defaultValues: { items: [{ name: "a" }, { name: "b" }, { name: "c" }] },
      });
      const { fields, remove } = useFieldArray({ control, name: "items" });
      return (
        <form>
          <output>{String(tick)}</output>
          {fields.map((field, index) => (
            <div key={field.id} data-key={field.id}>
              <input aria-label={`row ${index}`} {...register(`items.${index}.name`)} />
            </div>
          ))}
          <button type="button" onClick={() => setTick((count) => count + 1)}>
            Unrelated
          </button>
          <button type="button" onClick={() => remove(1)}>
            Remove middle
          </button>
        </form>
      );
    }

    const { container } = render(<Probe />);
    await userEvent.click(screen.getByRole("button", { name: "Unrelated" }));
    await userEvent.click(screen.getByRole("button", { name: "Unrelated" }));
    await userEvent.click(screen.getByRole("button", { name: "Remove middle" }));

    expect(elementsIn(container, "input").map(valueIn)).toEqual(["a", "c"]);
  });

  it("scopes a useFormState subscription to the fields it named", async () => {
    let messageRenders = 0;
    component MessageView(control: Control<{ a: string, b: string }>) {
      messageRenders += 1;
      const { errors } = useFormState({ control, name: "a" });
      return <output>{errors.a?.message ?? "none"}</output>;
    }
    // Memoised for the same reason as `Total` above: the claim is about what
    // the subscription wakes, not about what a parent render drags with it.
    const Message = React.memo(MessageView);

    component Probe() {
      const { register, control } = useForm({ defaultValues: { a: "", b: "" }, mode: "onChange" });
      return (
        <form>
          <input aria-label="a" {...register("a", { required: "Required" })} />
          <input aria-label="b" {...register("b", { required: "Required" })} />
          <Message control={control} />
        </form>
      );
    }

    render(<Probe />);
    const start = messageRenders;

    // `b` becoming invalid is not this component's business.
    await userEvent.type(screen.getByLabelText("b"), "x");
    await userEvent.clear(screen.getByLabelText("b"));
    await settle();
    expect(messageRenders).toBe(start);

    await userEvent.type(screen.getByLabelText("a"), "x");
    await userEvent.clear(screen.getByLabelText("a"));
    await waitFor(() => {
      expect(screen.getByText("Required")).toBeInTheDocument();
    });
  });
});

// What `uf check` says about the promise this package's types make, which is
// the one promise no amount of rendering can hold it to: that a per-field read
// comes back as the type that field actually holds, and that a misspelt segment
// or a wrongly typed write is an error at the call.
//
// `tests/type-tests/field-paths.js` is the misuse, written down. It is
// *supposed* to fail `uf check`, it marks each line that must fail with a
// `// expect:` comment, and this reads both and compares them — so a change
// that makes one of them stop being an error fails here, and so does one that
// makes something else in that file start being one.

// This checkout, found by a file it has rather than by counting `..`, for the
// reason `ui.test.js` sets out at length: which project `uf test` selected
// depends on how the command was typed, and two levels above the worker's
// project is this repository only under one of them.
const repository: string = (() => {
  const wanted = path.join("packages", "form", "internal", "field-path.js");
  const from = process.env.UF_PROJECT_ROOT ?? process.cwd();
  let directory = from;
  for (let up = 0; up < 8; up += 1) {
    if (fs.existsSync(path.join(directory, wanted))) return directory;
    directory = path.dirname(directory);
  }
  throw new Error(`could not find ${wanted} above ${from}`);
})();

// The binary running this suite: `uf test` puts its own path in `UF_BINARY`, so
// this checks *this* build rather than whatever `uf` is on PATH.
const UF: string = (() => {
  const binary = process.env.UF_BINARY;
  if (binary == null || binary === "") {
    throw new Error("UF_BINARY is not set: this test runs `uf check`, and `uf test` names it");
  }
  return binary;
})();

// The part of `uf check --json` this reads. A message arrives as spans rather
// than a string so that a renderer can mark the code inside it, which is why
// the comparison below joins it back together first.
type Diagnostic = {
  primary: { path: string, start: { line: number, column: number } },
  message: Array<{ kind: string, text: string }>,
};
type Report = {
  typeCheck: { status: string, filesChecked: number, diagnostics: Array<Diagnostic> },
};

describe("a field path is checked against the shape of the values", () => {
  const fixture = path.join("tests", "type-tests", "field-paths.js");

  it("reports every misuse, and only the misuses", () => {
    const source = fs.readFileSync(path.join(repository, fixture), "utf8").split("\n");
    const wanted = new Map<number, string>();
    source.forEach((line, index) => {
      const marker = line.match(/^\s*\/\/ expect: (.+)$/);
      if (marker != null) {
        // Lines are one-based, and the line that must fail is the next one.
        wanted.set(index + 2, marker[1]);
      }
    });
    // Without this the test would pass on a fixture somebody had emptied.
    expect(wanted.size).toBeGreaterThan(10);

    // Both paths in one command, and that is load-bearing: `uf check` builds
    // its module map from the files it is asked about, so a relative import
    // that leaves that set resolves to an any-typed value — after which
    // `TValues` is `any` and every line of the fixture passes.
    const run = spawnSync(UF, ["check", "tests/type-tests", "packages/form", "--json"], {
      cwd: repository,
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
    });
    if (run.stdout === "") {
      throw new Error(
        `\`uf check tests/type-tests packages/form --json\` in ${repository} printed ` +
          `nothing: status ${String(run.status)}, stderr ${JSON.stringify(run.stderr)}`,
      );
    }
    const report: Report = JSON.parse(run.stdout);
    expect(report.typeCheck.status).toBe("checked");

    const reported = new Map<number, string>();
    for (const diagnostic of report.typeCheck.diagnostics) {
      if (diagnostic.primary.path.endsWith(fixture)) {
        reported.set(
          diagnostic.primary.start.line,
          diagnostic.message.map((span) => span.text).join(""),
        );
      }
    }

    const missing = [];
    for (const [line, expected] of wanted) {
      const said = reported.get(line);
      if (said == null || !said.includes(expected)) {
        missing.push(`${fixture}:${String(line)} should say "${expected}", said ${String(said)}`);
      }
    }
    // Every marked line is an error, with the message the fixture predicted.
    expect(missing).toEqual([]);

    // And nothing else in the file is. This is the half that says the typed
    // reads *work*: every correct segment read, every correct write, and every
    // dotted string beside them is silent.
    const unexpected = [...reported.keys()]
      .filter((line) => !wanted.has(line))
      .map((line) => `${fixture}:${String(line)} ${String(reported.get(line))}`);
    expect(unexpected).toEqual([]);
  });
});
