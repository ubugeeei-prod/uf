// @flow
//
// The block `uf ui add sign-in-form` writes, held to what its header promises:
// errors said where they happened, the email kept and the password dropped
// after a failed attempt, a focusable button while pending, and nothing that
// fails an accessibility audit.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import { type SignInResult, SignInForm } from "./sign-in-form.js";
import { Example } from "./sign-in-form.example.js";

afterEach(() => {
  cleanup();
});

describe("SignInForm", () => {
  it("names the form by its heading and lets the browser fill both fields", () => {
    render(<Example />);
    const form = screen.getByRole("form", { name: "Sign in" });
    expect(form).toBeTruthy();
    expect(screen.getByRole("heading", { level: 2, name: "Sign in" })).toBeTruthy();
    expect(screen.getByLabelText("Email")).toHaveAttribute("autocomplete", "email");
    expect(screen.getByLabelText("Password")).toHaveAttribute("autocomplete", "current-password");
  });

  it("announces a failure of the whole form, keeps the email and drops the password", async () => {
    render(<Example />);
    await userEvent.type(screen.getByLabelText("Email"), "ada@example.com");
    await userEvent.type(screen.getByLabelText("Password"), "wrong");
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("That email and password do not match an account.");
    expect(screen.getByLabelText("Email")).toHaveValue("ada@example.com");
    expect(screen.getByLabelText("Password")).toHaveValue("");
  });

  it("puts a field's error under that field and marks it invalid", async () => {
    render(<Example />);
    await userEvent.type(screen.getByLabelText("Email"), "ada");
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }));

    const error = await screen.findByText("Enter an email address, like ada@example.com.");
    const email = screen.getByLabelText("Email");
    expect(email).toHaveAttribute("aria-invalid", "true");
    expect(email.getAttribute("aria-describedby") ?? "").toContain(error.id);
    // The field's own error is the one thing announced: no form-level alert.
    expect(screen.getAllByRole("alert")).toEqual([error]);
  });

  it("keeps the button focusable and says so while the action runs", async () => {
    let finish: (result: SignInResult) => void = () => {};
    const action = (): Promise<SignInResult> =>
      new Promise((resolve) => {
        finish = resolve;
      });
    render(<SignInForm action={action} />);
    await userEvent.type(screen.getByLabelText("Email"), "ada@example.com");
    await userEvent.type(screen.getByLabelText("Password"), "secret");
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }));

    const pending = await screen.findByRole("button", { name: "Signing in…" });
    expect(pending).toHaveAttribute("aria-disabled", "true");
    expect(pending.hasAttribute("disabled")).toBe(false);

    finish({});
    expect(await screen.findByRole("button", { name: "Sign in" })).not.toHaveAttribute(
      "aria-disabled",
    );
  });

  it("has no accessibility violations, before and after a failed attempt", async () => {
    const { container } = render(<Example />);
    await expect(container).toHaveNoAxeViolations();

    await userEvent.type(screen.getByLabelText("Email"), "ada@example.com");
    await userEvent.type(screen.getByLabelText("Password"), "wrong");
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }));
    await screen.findByRole("alert");
    await expect(container).toHaveNoAxeViolations();
  });
});
