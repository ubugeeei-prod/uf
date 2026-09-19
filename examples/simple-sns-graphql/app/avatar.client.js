"use client";
// @flow
import * as React from "@uniflowed/react";
import { graphql, useFragment } from "@uniflowed/relay";
import { Avatar } from "./ui.js";
import type { SnsAvatar_user$key } from "./__generated__/SnsAvatar_user.graphql.js";

const avatarFragment = graphql`
  fragment SnsAvatar_user on User {
    id
    photo
    avatar
  }
`;

/** Avatar data is masked from every parent, including posts and conversations. */
export component UserAvatar(userRef: SnsAvatar_user$key, small: boolean = false) {
  const user = useFragment(avatarFragment, userRef);
  return <Avatar user={user} small={small} />;
}
