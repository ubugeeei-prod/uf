"use client";
// @flow

import * as React from "@uniflowed/react";
import { useActionState } from "@uniflowed/react";
import { promise, runPromiseExit } from "@uniflowed/effect";
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
          body: new URLSearchParams({ mode: "logout" }),
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
      {kind: "failure", ...} => {
        return "You are offline. Try again.";
      }
    }
  }, "");

  return (
    <form action={submit}>
      <button className="icon-button" aria-label="Sign out" disabled={pending}>
        <Icon name="logout" size={18} />
      </button>
      {error ? (
        <span role="alert" className="field-error">
          {error}
        </span>
      ) : null}
    </form>
  );
}
