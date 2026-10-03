"use client";
// @flow

import * as React from "@uniflowed/react";
import { useActionState } from "@uniflowed/react";
import { promise, runPromiseExit } from "@uniflowed/effect";

import { props, stylex } from "@uniflowed/stylex";

import { styles as formStyles } from "./form-ui.client.js";
import { Icon } from "./ui.js";

/**
 * Revoke the session through HTTP and leave authenticated React state by reloading the document.
 */

export component SignOut() {
  const [error, submit, pending] = useActionState<string, FormData>(async () => {
    const result = await runPromiseExit(
      promise(() =>
        fetch("/auth/session", {
          method: "POST",
          body  : new URLSearchParams({ mode: "logout" }),
        }),
      ),
    );

    match (result) {
      {kind: "success", value: const response} => {
        if (!response.ok) {
          return "Could not sign out. Try again.";
        }

        window.location.assign("/");
        return "";
      }
      {kind: "failure", ...}                   => {
        return "You are offline. Try again.";
      }
    }
  }, "");

  return (
    <form action={submit}>
      <button type="submit" {...props(styles.iconButton)} aria-label="Sign out" disabled={pending}>
        <Icon name="logout" size={18} />
      </button>
      {
        match (error) {
          ""            => null,
          const message =>
            <span role="alert" {...props(formStyles.fieldError)}>
              {message}
            </span>,
        }
      }
    </form>
  );
}

const styles = stylex.create({
  iconButton: {
    display       : "inline-flex",
    alignItems    : "center",
    justifyContent: "center",
    background    : "transparent",
    border        : "0",
    color         : "#7f7f7f",
    borderRadius  : "5px",
    paddingTop    : "8px",
    paddingRight  : "8px",
    paddingBottom : "8px",
    paddingLeft   : "8px",
    ":hover": {
      background: "#efefef",
      color     : "var(--ink)",
    },
  },
});
