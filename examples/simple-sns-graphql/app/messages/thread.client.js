"use client";
// @flow
import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { graphql, useFragment } from "@uniflowed/relay";
import { UserAvatar } from "../_shared/avatar.client.js";
import type { SnsThread_thread$key } from "./__generated__/SnsThread_thread.graphql.js";

const threadFragment = graphql`
  fragment SnsThread_thread on Thread {
    id
    lastMessage
    participant {
      name
      ...SnsAvatar_user
    }
  }
`;

export component Thread(threadRef: SnsThread_thread$key, selected: boolean) {
  const thread = useFragment(threadFragment, threadRef);
  return (
    <Link
      className="thread"
      to={`/messages?thread=${encodeURIComponent(thread.id)}`}
      aria-current={selected ? "page" : undefined}
    >
      <UserAvatar userRef={thread.participant} small />
      <span>
        <strong>{thread.participant.name}</strong>
        <small>{thread.lastMessage}</small>
      </span>
    </Link>
  );
}
