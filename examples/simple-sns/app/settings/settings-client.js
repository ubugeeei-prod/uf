"use client";
// @flow

import * as React from "@uniflowed/react";
import { useActionState } from "@uniflowed/react";
import { useFormStatus } from "react-dom";
import { props, stylex } from "@uniflowed/stylex";

import { updateSettings } from "../social-actions.js";
import { type FormState, type Settings, profileInitials } from "../social-model.js";

component SaveButton() renders React.Node {
  const { pending } = useFormStatus();
  return (
    <button type="submit" disabled={pending} {...props(styles.button)}>
      {pending ? "Saving" : "Save settings"}
    </button>
  );
}

export component SettingsClient(initial: Settings) renders React.Node {
  const [state, action] = useActionState<FormState<Settings>, FormData>(updateSettings, {
    status: "idle",
    message: "",
    value: initial,
  });
  const current = state.value ?? initial;

  return (
    <form action={action} suppressHydrationWarning {...props(styles.form)}>
      <section {...props(styles.profile)}>
        <span {...props(styles.avatar)}>{profileInitials(current)}</span>
        <div>
          <h2 {...props(styles.title)}>{current.displayName}</h2>
          <p {...props(styles.handle)}>@{current.handle}</p>
        </div>
      </section>
      <div {...props(styles.grid)}>
        <label {...props(styles.field)}>
          <span {...props(styles.label)}>Display name</span>
          <input
            name="displayName"
            defaultValue={current.displayName}
            required
            {...props(styles.input)}
          />
        </label>
        <label {...props(styles.field)}>
          <span {...props(styles.label)}>Handle</span>
          <input name="handle" defaultValue={current.handle} required {...props(styles.input)} />
        </label>
      </div>
      <label {...props(styles.field)}>
        <span {...props(styles.label)}>Email</span>
        <input
          name="email"
          type="email"
          defaultValue={current.email}
          required
          {...props(styles.input)}
        />
      </label>
      <label {...props(styles.field)}>
        <span {...props(styles.label)}>Bio</span>
        <textarea name="bio" defaultValue={current.bio} rows={4} {...props(styles.textarea)} />
      </label>
      <div {...props(styles.switches)}>
        <label {...props(styles.check)}>
          <input name="digest" type="checkbox" defaultChecked={current.digest} />
          <span>Daily digest</span>
        </label>
        <label {...props(styles.check)}>
          <input name="quietMode" type="checkbox" defaultChecked={current.quietMode} />
          <span>Quiet mode</span>
        </label>
      </div>
      <div {...props(styles.footer)}>
        <span {...props(styles.status, state.status === "error" && styles.error)}>
          {state.message}
        </span>
        <SaveButton />
      </div>
    </form>
  );
}

const styles = stylex.create({
  form: {
    backgroundColor: "#ffffff",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    boxShadow: "0 16px 38px rgba(15, 23, 42, 0.08)",
    display: "grid",
    gap: 16,
    padding: {
      default: 16,
      "@media (min-width: 760px)": 20,
    },
  },
  profile: {
    alignItems: "center",
    borderBottomColor: "#eaecf0",
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "flex",
    gap: 12,
    paddingBottom: 16,
  },
  avatar: {
    alignItems: "center",
    backgroundColor: "#be123c",
    borderRadius: 8,
    color: "#ffffff",
    display: "inline-flex",
    fontSize: 16,
    fontWeight: 800,
    height: 52,
    justifyContent: "center",
    width: 52,
  },
  title: {
    color: "#111827",
    fontSize: 22,
    lineHeight: 1.15,
    marginBlock: 0,
  },
  handle: {
    color: "#667085",
    marginBlock: 0,
  },
  grid: {
    display: "grid",
    gap: 14,
    gridTemplateColumns: {
      default: "1fr",
      "@media (min-width: 720px)": "1fr 1fr",
    },
  },
  field: {
    display: "grid",
    gap: 6,
  },
  label: {
    color: "#344054",
    fontSize: 13,
    fontWeight: 800,
  },
  input: {
    backgroundColor: "#f8fafc",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: "#0f172a",
    font: "inherit",
    minHeight: 46,
    paddingInline: 14,
  },
  textarea: {
    backgroundColor: "#f8fafc",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: "#0f172a",
    font: "inherit",
    padding: 14,
    resize: "vertical",
  },
  switches: {
    display: "grid",
    gap: 10,
  },
  check: {
    alignItems: "center",
    color: "#344054",
    display: "flex",
    gap: 10,
    fontWeight: 700,
  },
  footer: {
    alignItems: {
      default: "stretch",
      "@media (min-width: 560px)": "center",
    },
    display: {
      default: "grid",
      "@media (min-width: 560px)": "flex",
    },
    gap: 12,
    justifyContent: "space-between",
  },
  status: {
    color: "#667085",
    fontSize: 14,
  },
  error: {
    color: "#b42318",
  },
  button: {
    backgroundColor: "#111827",
    borderColor: "#111827",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: "#ffffff",
    cursor: "pointer",
    font: "inherit",
    fontWeight: 800,
    minHeight: 46,
    paddingInline: 18,
  },
});
