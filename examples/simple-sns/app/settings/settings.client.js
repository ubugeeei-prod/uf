"use client";
// @flow

import * as React from "@uniflowed/react";

import { callAction } from "../_shared/action-result.client.js";

import { useActionState, useState } from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { Field } from "@uniflowed/ui";

import { updateSettings } from "../_server/social-actions.js";
import {
  FormField,
  FormStatus,
  SubmitButton,
  styles as controlStyles,
} from "../_shared/form-ui.client.js";
import { Avatar, styles as uiStyles } from "../_shared/ui.js";
import {
  IDLE,
  fieldError,
  profileInitials,
  type FormState,
  type Settings,
} from "../_shared/social-model.js";

/**
 * Keep editable profile fields local until a successful server action commits them.
 * Validation failures preserve the draft and associate feedback with the affected fields.
 */

export component SettingsClient(initial: Settings) {
  const [draft, setDraft]        = useState<Settings>(initial);
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
        {status: "error", ...}                       => {}
      }
      return result;
    },
    IDLE,
  );

  return (
    <form {...props(uiStyles.settingsPanel)} action={submit} aria-label="Profile settings">
      <div {...props(uiStyles.settingsProfile)}>
        <Avatar
          user={{
            id    : "profile",
            name  : draft.displayName,
            handle: draft.handle,
            avatar: profileInitials(draft),
            bio   : draft.bio,
          }}
        />
        <div>
          <h2 {...props(styles.profileName)}>{draft.displayName}</h2>
          <p {...props(styles.profileHandle)}>@{draft.handle}</p>
        </div>
      </div>
      <section {...props(uiStyles.formSection)}>
        <h3 {...props(styles.sectionTitle)}>Your profile</h3>
        <p {...props(styles.sectionCopy)}>Your name and bio are visible to the community.</p>
        <div {...props(uiStyles.formGrid)}>
          <FormField label="Display name" error={fieldError(state, "displayName")}>
            <Field.Control
              render={(control) => (
                <input
                  {...control}
                  className={props(controlStyles.fieldControl).className}
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
            <Field.Control
              render={(control) => (
                <input
                  {...control}
                  className={props(controlStyles.fieldControl).className}
                  name="handle"
                  value={draft.handle}
                  onChange={(event) => setDraft({ ...draft, handle: event.target.value })}
                  required
                  minLength={3}
                  maxLength={20}
                  pattern="[a-z][a-z0-9_]{2,19}"
                  autoCapitalize="none"
                  spellCheck={false}
                  autoComplete="username"
                  disabled={pending}
                />
              )}
            />
          </FormField>
        </div>
        <FormField label="Bio" error={fieldError(state, "bio")} hint="Up to 160 characters.">
          <Field.Control
            render={(control) => (
              <textarea
                {...control}
                className={props(controlStyles.fieldControl).className}
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
      <section {...props(uiStyles.formSection)}>
        <h3 {...props(styles.sectionTitle)}>Account details</h3>
        <p {...props(styles.sectionCopy)}>Your email is private.</p>
        <FormField label="Email address" error={fieldError(state, "email")}>
          <Field.Control
            render={(control) => (
              <input
                {...control}
                className={props(controlStyles.fieldControl).className}
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
      <footer {...props(uiStyles.settingsFooter)}>
        <FormStatus state={state} />
        <SubmitButton pendingLabel="Saving…" xstyle={styles.settingsSubmit}>
          Save changes
        </SubmitButton>
      </footer>
    </form>
  );
}

const styles = stylex.create({
  profileName: {
    fontSize     : "17px",
    fontWeight   : "600",
    letterSpacing: "-0.3px",
  },
  profileHandle: {
    fontSize : "12px",
    color    : "var(--muted)",
    marginTop: "5px",
  },
  sectionTitle: {
    fontSize    : "14px",
    fontWeight  : "600",
    marginBottom: "7px",
  },
  sectionCopy: {
    fontSize    : "12px",
    color       : "var(--muted)",
    lineHeight  : "1.7",
    marginBottom: "23px",
  },
  settingsSubmit: {
    alignSelf: { "@media (max-width: 760px)": "flex-end" },
  },
});
