"use client";
// @flow

import * as React from "@uniflowed/react";
import { graphql, useMutation } from "@uniflowed/relay";
import { useState } from "@uniflowed/react";

import { Icon } from "./ui.js";

const logout = graphql`
  mutation SnsLogoutMutation {
    logout
  }
`;

/** Reset the entire browser store when identity changes. */

export component SignOut() {
  const [commit, pending] = useMutation(logout);
  const [error, setError] = useState("");

  return (
    <div>
      <button
        type="button"
        className="icon-button"
        aria-label="Sign out"
        disabled={pending}
        onClick={() =>
          commit({
            variables: {},
            onCompleted: () => window.location.assign("/"),
            onError: () => setError("Could not sign out. Try again."),
          })
        }
      >
        <Icon name="logout" size={17} />
      </button>
      {error ? <p role="alert">{error}</p> : null}
    </div>
  );
}
