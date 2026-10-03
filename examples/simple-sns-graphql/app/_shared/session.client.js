"use client";
// @flow

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { graphql, useMutation } from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";
import { props, stylex } from "@uniflowed/stylex";

import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import type {
  SnsSessionQuery$variables,
  SnsSessionQuery$data,
} from "./__generated__/SnsSessionQuery.graphql.js";

import { UserAvatar } from "./avatar.client.js";
import { Icon, styles as uiStyles } from "./ui.js";

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
        {...props(uiStyles.composeLink)}
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
      <div {...props(uiStyles.account)}>
        {
          match (viewer) {
            null | undefined =>
              <>
                <Link {...props(uiStyles.button, uiStyles.primary, styles.accountButton)} to="/login">
                  Sign in
                </Link>
                <Link {...props(uiStyles.button, uiStyles.secondary, styles.accountButton)} to="/signup">
                  Join
                </Link>
              </>,
            const person     =>
              <>
                <Link to="/settings" {...props(styles.accountPerson)}>
                  <UserAvatar userRef={person} small />
                  <span>
                    <strong {...props(styles.accountName)}>{person.name}</strong>
                    <small {...props(styles.accountHandle)}>@{person.handle}</small>
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
      {...props(uiStyles.mobileNavItem)}
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
        {...props(styles.iconButton)}
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

const styles = stylex.create({
  accountButton: {
    flex         : "1",
    fontSize     : "12px",
    paddingTop   : "9px",
    paddingRight : "9px",
    paddingBottom: "9px",
    paddingLeft  : "9px",
  },
  accountPerson: {
    display   : "flex",
    gap       : "9px",
    alignItems: "center",
    minWidth  : "0",
    flex      : "1",
  },
  accountName: {
    display     : "block",
    maxWidth    : "106px",
    overflow    : "hidden",
    textOverflow: "ellipsis",
    whiteSpace  : "nowrap",
    fontSize    : "12px",
    fontWeight  : "600",
  },
  accountHandle: {
    display     : "block",
    maxWidth    : "106px",
    overflow    : "hidden",
    textOverflow: "ellipsis",
    whiteSpace  : "nowrap",
    fontSize    : "11px",
    color       : "var(--muted)",
    marginTop   : "3px",
  },
  iconButton: {
    display       : "inline-flex",
    alignItems    : "center",
    justifyContent: "center",
    background    : "transparent",
    border        : "0",
    color         : "#7f7f7f",
    borderRadius  : "5px",
    paddingTop    : "8px",
    paddingRight  : "8px",
    paddingBottom : "8px",
    paddingLeft   : "8px",
    ":hover"      : {
      background: "#efefef",
      color     : "var(--ink)",
    },
  },
});
