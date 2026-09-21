"use client";
// @flow

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { graphql, useMutation } from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";

import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import type {
  SnsSettingsQuery$variables,
  SnsSettingsQuery$data,
} from "./__generated__/SnsSettingsQuery.graphql.js";
import type { SnsUpdateSettingsMutation } from "./__generated__/SnsUpdateSettingsMutation.graphql.js";

const settingsQuery = graphql`
  query SnsSettingsQuery {
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

export component SettingsForm(
  queryRef: PreloadedQueryRef<SnsSettingsQuery$variables, SnsSettingsQuery$data>,
) {
  const { settings } = useQueryFromServer(settingsQuery, queryRef);
  const [commit, pending] = useMutation<
    SnsUpdateSettingsMutation["variables"],
    SnsUpdateSettingsMutation["response"],
  >(updateSettings);
  const [feedback, setFeedback] = useState("");
  // The session can end between the identity gate and this read.
  if (settings == null) return null;

  return (
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
  );
}
