"use client";
// @flow
import * as React from "@uniflowed/react";
import type { RouteError } from "@uniflowed/router";

export default component NoteError(error: RouteError, reset: () => void) {
  return (
    <section id="note-error">
      {
        match (error) {
          {kind: "unauthorized"} => "Sign in to read this note.",
          {kind: "forbidden"} => "This note is not yours.",
          {kind: "thrown", ...} => "This note did not load.",
        }
      }
      <button type="button" onClick={reset}>
        Try again
      </button>
    </section>
  );
}
