"use client";
// @flow
import * as React from "@uniflowed/react";
import { useActionState } from "@uniflowed/react";
import { Icon } from "./ui.js";
export component SignOut() {
  const [error, submit, pending] = useActionState<string, FormData>(async () => {
    try {
      const response = await fetch("/auth/session", {
        method: "POST",
        body: new URLSearchParams({ mode: "logout" }),
      });
      if (!response.ok) return "Could not sign out. Try again.";
      window.location.assign("/");
      return "";
    } catch {
      return "You are offline. Try again.";
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
