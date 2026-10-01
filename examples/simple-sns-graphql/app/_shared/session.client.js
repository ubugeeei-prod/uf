"use client";
// @flow

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { graphql, useMutation } from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";

import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import type {
  SnsSessionQuery$variables,
  SnsSessionQuery$data,
} from "./__generated__/SnsSessionQuery.graphql.js";

import { UserAvatar } from "./avatar.client.js";
import { Icon } from "./ui.js";

const sessionQuery = graphql`
  query SnsSessionQuery {
    viewer {
      name
      handle
      ...SnsAvatar_user
    }
  }
`;

const logout = graphql`
  mutation SnsLogoutMutation {
    logout
  }
`;

/** The server's viewer preload. Every island below commits and reads the same reference. */

export type SessionRef = PreloadedQueryRef<SnsSessionQuery$variables, SnsSessionQuery$data>;

/** The sidebar's identity-dependent controls; the navigation around them is server-rendered. */

export component SessionControls(queryRef: SessionRef) {
  const { viewer } = useQueryFromServer(sessionQuery, queryRef);

  return (
    <>
      <Link
        className="compose-link"
        to={
          match (viewer) {
            null | undefined => "/login",
            _                => "/#compose",
          }
        }
      >
        <Icon name="compose" size={16} />
        Write a note
      </Link>
      <div className="account">
        {
          match (viewer) {
            null | undefined =>
              <>
                <Link className="button primary" to="/login">
                  Sign in
                </Link>
                <Link className="button secondary" to="/signup">
                  Join
                </Link>
              </>,
            const person     =>
              <>
                <Link to="/settings" className="account-person">
                  <UserAvatar userRef={person} small />
                  <span>
                    <strong>{person.name}</strong>
                    <small>@{person.handle}</small>
                  </span>
                </Link>
                <SignOut />
              </>,
          }
        }
      </div>
    </>
  );
}

/** The one identity-dependent entry in the otherwise server-rendered mobile navigation. */

export component MobileCompose(queryRef: SessionRef) {
  const { viewer } = useQueryFromServer(sessionQuery, queryRef);

  return (
    <Link
      to={
        match (viewer) {
          null | undefined => "/login",
          _                => "/#compose",
        }
      }
    >
      <Icon name="compose" size={20} />
      <span>
        {
          match (viewer) {
            null | undefined => "Sign in",
            _                => "Write",
          }
        }
      </span>
    </Link>
  );
}

/**
 * Choose between two server-rendered subtrees by identity. Private islands inside `children`
 * bring their own preloads; an anonymous request's references carry no private records.
 */

export component SignedIn(queryRef: SessionRef, guest: React.Node, children: React.Node) {
  const { viewer } = useQueryFromServer(sessionQuery, queryRef);

  return viewer == null ? guest : children;
}

/** Reset the entire browser store when identity changes. */

component SignOut() {
  const [commit, pending]  = useMutation(logout);
  const [error,  setError] = useState("");

  return (
    <div>
      <button
        type="button"
        className="icon-button"
        aria-label="Sign out"
        disabled={pending}
        onClick={() =>
          commit({
            variables  : {},
            onCompleted: () => window.location.assign("/"),
            onError    : () => setError("Could not sign out. Try again."),
          })
        }
      >
        <Icon name="logout" size={17} />
      </button>
      {
        match (error) {
          ""            => null,
          const message => <p role="alert">{message}</p>,
        }
      }
    </div>
  );
}
