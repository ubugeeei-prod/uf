"use client";
// @flow
import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { graphql, useFragment, useMutation } from "@uniflowed/relay";
import { SignInPrompt } from "../_shared/ui.js";
import type { SnsSettings_query$key } from "./__generated__/SnsSettings_query.graphql.js";
import type { SnsUpdateSettingsMutation } from "./__generated__/SnsUpdateSettingsMutation.graphql.js";

const settingsFragment = graphql`
  fragment SnsSettings_query on Query {
    settings {
      displayName
      bio
      email
    }
  }
`;

const updateSettings = graphql`
  mutation SnsUpdateSettingsMutation($input: SettingsInput!) {
    updateSettings(input: $input) {
      id
      displayName
      bio
      email
    }
  }
`;

/** The service derives the account to update; no account ID enters the input. */
export component SettingsForm(queryRef: SnsSettings_query$key) {
  const { settings } = useFragment(settingsFragment, queryRef);
  const [commit, pending] = useMutation<
    SnsUpdateSettingsMutation["variables"],
    SnsUpdateSettingsMutation["response"],
  >(updateSettings);
  const [feedback, setFeedback] = useState("");
  if (settings == null) return <SignInPrompt />;
  return (
    <section className="settings-panel">
      <header className="page-heading">
        <div>
          <h1>Settings</h1>
          <p>Your profile and account.</p>
        </div>
      </header>
      <form
        className="settings-form"
        onSubmit={(event) => {
          event.preventDefault();
          setFeedback("");
          const values = new FormData(event.currentTarget);
          commit({
            variables: {
              input: {
                displayName: String(values.get("displayName")),
                bio: String(values.get("bio")),
                email: String(values.get("email")),
              },
            },
            onCompleted: () => setFeedback("Your changes are saved."),
            onError: (failure: Error) => setFeedback(failure.message),
          });
        }}
      >
        <label className="field">
          <span>Display name</span>
          <input name="displayName" defaultValue={settings.displayName} required maxLength={80} />
        </label>
        <label className="field">
          <span>Bio</span>
          <textarea name="bio" defaultValue={settings.bio} maxLength={240} />
        </label>
        <label className="field">
          <span>Email</span>
          <input type="email" name="email" defaultValue={settings.email} required />
        </label>
        <button type="submit" className="button primary" disabled={pending}>
          {pending ? "Saving…" : "Save changes"}
        </button>
        {feedback ? <p role="status">{feedback}</p> : null}
      </form>
    </section>
  );
}
