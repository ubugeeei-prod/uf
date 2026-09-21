"use client";
// @flow

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { graphql, useFragment } from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";

import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import { UserAvatar } from "../_shared/avatar.client.js";

import type {
  SnsThreadsQuery$variables,
  SnsThreadsQuery$data,
} from "./__generated__/SnsThreadsQuery.graphql.js";
import type { SnsThread_thread$key } from "./__generated__/SnsThread_thread.graphql.js";

const threadsQuery = graphql`
  query SnsThreadsQuery {
    threads {
      id
      ...SnsThread_thread
    }
  }
`;

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

/** The list composes fragment references; it cannot read its rows' fields. */

export component Threads(
  queryRef: PreloadedQueryRef<SnsThreadsQuery$variables, SnsThreadsQuery$data>,
  selected: string,
) {
  const { threads } = useQueryFromServer(threadsQuery, queryRef);

  return (
    <nav className="thread-list" aria-label="Conversations">
      {threads.map((thread) => (
        <Thread key={thread.id} threadRef={thread} selected={thread.id === selected} />
      ))}
    </nav>
  );
}

component Thread(threadRef: SnsThread_thread$key, selected: boolean) {
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
