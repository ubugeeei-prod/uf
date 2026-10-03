"use client";
// @flow

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { graphql, useMutation } from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";
import { props } from "@uniflowed/stylex";

import { styles as uiStyles } from "../_shared/ui.js";

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
  const { settings }            = useQueryFromServer(settingsQuery, queryRef);
  const [commit,   pending]     = useMutation<
    SnsUpdateSettingsMutation["variables"],
    SnsUpdateSettingsMutation["response"],
  >(updateSettings);
  const [feedback, setFeedback] = useState("");
  // The session can end between the identity gate and this read.
  if (settings == null) return null;

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        setFeedback("");
        const values = new FormData(event.currentTarget);
        commit({
          variables: {
            input: {
              displayName: String(values.get("displayName")),
              bio        : String(values.get("bio")),
              email      : String(values.get("email")),
            },
          },
          onCompleted: () => setFeedback("Your changes are saved."),
          onError    : (failure: Error) => setFeedback(failure.message),
        });
      }}
    >
      <label {...props(uiStyles.field)}>
        <span {...props(uiStyles.fieldLabel)}>Display name</span>
        <input
          {...props(uiStyles.fieldControl)}
          name="displayName"
          defaultValue={settings.displayName}
          required
          maxLength={80}
        />
      </label>
      <label {...props(uiStyles.field)}>
        <span {...props(uiStyles.fieldLabel)}>Bio</span>
        <textarea {...props(uiStyles.fieldControl)} name="bio" defaultValue={settings.bio} maxLength={240} />
      </label>
      <label {...props(uiStyles.field)}>
        <span {...props(uiStyles.fieldLabel)}>Email</span>
        <input
          {...props(uiStyles.fieldControl)}
          type="email"
          name="email"
          defaultValue={settings.email}
          required
        />
      </label>
      <button type="submit" {...props(uiStyles.button, uiStyles.primary)} disabled={pending}>
        {
          match (pending) {
            true  => "Saving…",
            false => "Save changes",
          }
        }
      </button>
      {
        match (feedback) {
          ""            => null,
          const message => <p role="status">{message}</p>,
        }
      }
    </form>
  );
}
