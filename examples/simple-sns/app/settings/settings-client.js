"use client";
// @flow
import * as React from "@uniflowed/react";
import { callAction } from "../action-result.client.js";
import { useActionState, useState } from "@uniflowed/react";
import { FieldControl } from "@uniflowed/ui/field";
import { updateSettings } from "../social-actions.js";
import { FormField, FormStatus, SubmitButton } from "../form-ui.client.js";
import { Avatar } from "../ui.js";
import {
  IDLE,
  fieldError,
  profileInitials,
  type FormState,
  type Settings,
} from "../social-model.js";

export component SettingsClient(initial: Settings) {
  const [draft, setDraft] = useState<Settings>(initial);
  const [state, submit, pending] = useActionState<FormState<Settings>, FormData>(
    async (_previous: FormState<Settings>, form: FormData): Promise<FormState<Settings>> => {
      const result = await callAction(
        () => updateSettings(IDLE, form),
        "Could not save. Your edits are still here; try again.",
      );
      match (result) {
        {status: "success", value: const saved, ...} => {
          setDraft(saved);
        }
        {status: "error", ...} => {}
      }
      return result;
    },
    IDLE,
  );
  return (
    <form action={submit} className="settings-panel" aria-label="Profile settings">
      <div className="settings-profile">
        <Avatar
          user={{
            id: "profile",
            name: draft.displayName,
            handle: draft.handle,
            avatar: profileInitials(draft),
            bio: draft.bio,
          }}
        />
        <div>
          <h2>{draft.displayName}</h2>
          <p>@{draft.handle}</p>
        </div>
      </div>
      <section className="form-section">
        <h3>Your profile</h3>
        <p>Your name and bio are visible to the community.</p>
        <div className="form-grid">
          <FormField label="Display name" error={fieldError(state, "displayName")}>
            <FieldControl
              render={(props) => (
                <input
                  {...props}
                  name="displayName"
                  value={draft.displayName}
                  onChange={(event) => setDraft({ ...draft, displayName: event.target.value })}
                  required
                  maxLength={80}
                  autoComplete="name"
                  disabled={pending}
                />
              )}
            />
          </FormField>
          <FormField label="Handle" error={fieldError(state, "handle")}>
            <FieldControl
              render={(props) => (
                <input
                  {...props}
                  name="handle"
                  value={draft.handle}
                  onChange={(event) => setDraft({ ...draft, handle: event.target.value })}
                  required
                  minLength={3}
                  maxLength={20}
                  pattern="[a-z][a-z0-9_]{2,19}"
                  autoComplete="username"
                  disabled={pending}
                />
              )}
            />
          </FormField>
        </div>
        <FormField label="Bio" error={fieldError(state, "bio")} hint="Up to 160 characters.">
          <FieldControl
            render={(props) => (
              <textarea
                {...props}
                name="bio"
                value={draft.bio}
                onChange={(event) => setDraft({ ...draft, bio: event.target.value })}
                rows={3}
                maxLength={160}
                disabled={pending}
              />
            )}
          />
        </FormField>
      </section>
      <section className="form-section">
        <h3>Account details</h3>
        <p>Your email is private.</p>
        <FormField label="Email address" error={fieldError(state, "email")}>
          <FieldControl
            render={(props) => (
              <input
                {...props}
                name="email"
                type="email"
                value={draft.email}
                onChange={(event) => setDraft({ ...draft, email: event.target.value })}
                required
                maxLength={254}
                autoComplete="email"
                disabled={pending}
              />
            )}
          />
        </FormField>
      </section>
      <footer className="settings-footer">
        <FormStatus state={state} />
        <SubmitButton pendingLabel="Saving…">Save changes</SubmitButton>
      </footer>
    </form>
  );
}
