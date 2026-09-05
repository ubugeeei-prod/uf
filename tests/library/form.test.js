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

import { createRequire } from "node:module";

import * as React from "@uniflowed/react";
import { StrictMode, useState } from "@uniflowed/react";
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

const submitForm = (container: mixed) => {
  const form: any = (container as any).querySelector("form");
  fireEvent.submit(form);
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

  it("keeps the values even though nothing rendered", async () => {
    let read = () => ({});
    component Probe() {
      const { register, getValues } = useForm({
        defaultValues: { email: "", nested: { city: "" } },
      });
      read = getValues as any;
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
    let read = () => ({});
    component Probe() {
      const { register, getValues } = useForm({ defaultValues: {} });
      read = getValues as any;
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
    const select: any = container.querySelector("select");
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
    const output: any = container.querySelector("output");

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

    component TotalView(control: mixed) {
      totalRenders += 1;
      const price = useWatch({ control: control as any, name: "price", defaultValue: "" });
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
    const output: any = container.querySelector("output");

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
    const output: any = container.querySelector("output");
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

describe("validation modes: quiet until they should not be", () => {
  component Probe(mode: mixed) {
    const { register, handleSubmit, formState, errorProps } = useForm({
      defaultValues: { email: "" },
      mode: mode as any,
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
  const check = async (rules: mixed, typed: string) => {
    let message = null;
    component Probe() {
      const { register, handleSubmit, formState } = useForm({ defaultValues: { field: "" } });
      message = formState.errors.field?.message ?? null;
      return (
        <form onSubmit={handleSubmit(() => {})}>
          <input aria-label="field" {...register("field", rules as any)} />
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
  const InvalidForm = (onValid: mixed, onInvalid: mixed) => {
    component Probe() {
      const { register, handleSubmit } = useForm({ defaultValues: { email: "" } });
      return (
        <form onSubmit={handleSubmit(onValid as any, onInvalid as any)}>
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
    const [errors] = onInvalid.mock.calls[0].args;
    expect((errors as any).email.message).toBe("Required");
    expect((errors as any).email.type).toBe("required");
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
    let release = () => {};
    const seen: Array<boolean> = [];
    component Probe() {
      const { handleSubmit, formState } = useForm({ defaultValues: {} });
      seen.push(formState.isSubmitting);
      return (
        <form
          onSubmit={handleSubmit(
            () =>
              new Promise((resolve) => {
                release = resolve as any;
              }),
          )}
        >
          <output>{`${String(formState.isSubmitting)}:${String(formState.submitCount)}`}</output>
        </form>
      );
    }

    const { container } = render(<Probe />);
    const output: any = container.querySelector("output");

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
    const output: any = container.querySelector("output");
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

  it("gives two copies of the same form different ids", async () => {
    const { container } = render(
      <div>
        <Probe />
        <Probe />
      </div>,
    );
    for (const form of Array.from((container as any).querySelectorAll("form"))) {
      fireEvent.submit(form as any);
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
    let state = null;
    component Probe() {
      const { register, reset, formState, getValues } = useForm({
        defaultValues: { email: "start" },
      });
      state = { formState, getValues };
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
    const field: any = screen.getByLabelText("email");
    await userEvent.type(field, "!");
    await userEvent.tabAway(field);

    expect((state as any).formState.isDirty).toBe(true);
    expect((state as any).formState.touchedFields.email).toBe(true);

    await userEvent.click(screen.getByRole("button", { name: "Reset" }));

    expect(field.value).toBe("start");
    expect((state as any).getValues()).toEqual({ email: "start" });
    expect((state as any).formState.isDirty).toBe(false);
    expect((state as any).formState.dirtyFields).toEqual({});
    expect((state as any).formState.touchedFields).toEqual({});
    expect((state as any).formState.submitCount).toBe(0);
  });

  it("takes new defaults, which is what a loaded record needs", async () => {
    let read = () => ({});
    component Probe() {
      const { register, reset, getValues } = useForm({ defaultValues: { email: "" } });
      read = getValues as any;
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
    expect((screen.getByLabelText("email") as any).value).toBe("loaded@example.com");
    expect(read()).toEqual({ email: "loaded@example.com" });
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
    expect((screen.getByLabelText("email") as any).value).toBe("set@example.com");
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
    const field: any = screen.getByLabelText("name");

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
    const output: any = container.querySelector("output");
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

  const keysIn = (container: mixed): Array<string> =>
    Array.from((container as any).querySelectorAll("li")).map((node: any) =>
      String(node.getAttribute("data-key")),
    );
  const textIn = (container: mixed): Array<string> =>
    Array.from((container as any).querySelectorAll("input")).map((node: any) => String(node.value));

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
    const output: any = container.querySelector("output");
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
    let read = () => ({});
    component Probe() {
      const { register, unregister, getValues } = useForm({ defaultValues: { a: "", b: "" } });
      read = getValues as any;
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
    let state = null;
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
    expect((state as any)("a")).toEqual({
      invalid: false,
      isDirty: false,
      isTouched: false,
      error: undefined,
    });

    await userEvent.type(screen.getByLabelText("a"), "!");
    await userEvent.tabAway(screen.getByLabelText("a"));
    const after: any = (state as any)("a");
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

  component Probe(onValid: mixed) {
    const { register, handleSubmit, formState } = useForm({
      defaultValues: { email: "", profile: { city: "" }, age: "0" },
      resolver: validatorResolver(account) as any,
    });
    return (
      <form onSubmit={handleSubmit(onValid as any)}>
        <input aria-label="email" {...register("email")} />
        <input aria-label="city" {...register("profile.city")} />
        <input aria-label="age" {...register("age")} />
        <output>{Object.keys(formState.errors).sort().join(",")}</output>
      </form>
    );
  }

  it("turns schema issues into field errors at the path the field is registered at", async () => {
    const { container } = render(<Probe onValid={() => {}} />);
    const output: any = container.querySelector("output");

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
      const { register, formState } = useForm({
        defaultValues: { email: "", profile: { city: "ok" }, age: "1" },
        resolver: validatorResolver(account) as any,
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
    const output: any = container.querySelector("output");
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
    let read = () => ({});
    component Probe() {
      const form = useForm({ defaultValues: { amount: "" }, mode: "onChange" });
      read = form.getValues as any;
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
    const field: any = screen.getByLabelText("amount");
    await userEvent.type(field, "12");
    expect(field.value).toBe("12");
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
    const field: any = screen.getByLabelText("amount");
    await userEvent.clear(field);
    await waitFor(() => {
      expect(screen.getByLabelText("amount")).toHaveAttribute("aria-invalid", "true");
    });
  });
});

describe("server rendering", () => {
  // Loaded the way `@uniflowed/react-testing` loads `react-dom/client`: through
  // a synchronous require, so a test file that never renders on the server does
  // not pay for the module.
  const server: any = createRequire(import.meta.url)("react-dom/server");

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

  it("adopts the value the server put in the markup once it mounts", async () => {
    let read = () => ({});
    component Adopting() {
      const { register, getValues } = useForm({ defaultValues: {} });
      read = getValues as any;
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
    let read = () => ({});
    component Probe() {
      const { register, getValues } = useForm({ defaultValues: { email: "" } });
      read = getValues as any;
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

    component Message(control: mixed) {
      const { errors } = useFormState({ control: control as any, name: "email" });
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

    expect(
      Array.from((container as any).querySelectorAll("input")).map((node: any) => node.value),
    ).toEqual(["a", "c"]);
  });

  it("scopes a useFormState subscription to the fields it named", async () => {
    let messageRenders = 0;
    component MessageView(control: mixed) {
      messageRenders += 1;
      const { errors } = useFormState({ control: control as any, name: "a" });
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
