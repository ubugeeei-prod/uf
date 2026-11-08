"use client";
"use flow";

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { graphql, useFragment, useRefetchableFragment } from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";
import { props } from "@uniflowed/stylex";

import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import { UserAvatar } from "../_shared/avatar.client.js";
import { styles as uiStyles } from "../_shared/ui.js";

import type {
  SnsThreadsQuery$variables,
  SnsThreadsQuery$data,
} from "./__generated__/SnsThreadsQuery.graphql.js";
import type { SnsThread_thread$key } from "./__generated__/SnsThread_thread.graphql.js";
import type { SnsThreads_query$data } from "./__generated__/SnsThreads_query.graphql.js";

const threadsQuery = graphql`
  query SnsThreadsQuery {
    ...SnsThreads_query
  }
`;

const threadsFragment = graphql`
  fragment SnsThreads_query on Query @refetchable(queryName: "SnsThreadsRefetchQuery") {
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

/**
 * The list composes fragment references; it cannot read its rows' fields.
 *
 * The fragment is refetchable so a message sent in the other pane can refresh
 * the preview. That pane executes the generated refetch query: the two reads
 * stream in separate boundaries, and this one does not hold the conversation.
 */

export component Threads(
  queryRef: PreloadedQueryRef<SnsThreadsQuery$variables, SnsThreadsQuery$data>,
  selected: string,
) {
  const loaded    = useQueryFromServer(threadsQuery, queryRef);
  const [untyped] = useRefetchableFragment(threadsFragment, loaded);
  const data: SnsThreads_query$data = untyped;
  const threads = data.threads;

  return (
    <nav {...props(uiStyles.threadList)} aria-label="Conversations">
      {threads.map((thread) => (
        <Thread key={thread.id} threadRef={thread} selected={thread.id === selected} />
      ))}
    </nav>
  );
}

component Thread(threadRef: SnsThread_thread$key, selected: boolean) renders Link {
  const thread = useFragment(threadFragment, threadRef);

  return (
    <Link
      to={`/messages?thread=${encodeURIComponent(thread.id)}`}
      aria-current={
        match (selected) {
          true  => "page",
          false => undefined,
        }
      }
    >
      <UserAvatar userRef={thread.participant} small />
      <span>
        <strong>{thread.participant.name}</strong>
        <small>{thread.lastMessage}</small>
      </span>
    </Link>
  );
}
