"use client";
"use flow";

import * as React from "@uniflowed/react";
import { startTransition, useLayoutEffect, useRef, useState } from "@uniflowed/react";
import {
  fetchQuery,
  graphql,
  useFragment,
  useMutation,
  useRefetchableFragment,
  useRelayEnvironment,
} from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";
import { props, stylex } from "@uniflowed/stylex";

import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import { RefreshBoundary } from "../_shared/refresh-boundary.client.js";
import { styles as uiStyles } from "../_shared/ui.js";
import threadsRefetchQuery from "./__generated__/SnsThreadsRefetchQuery.graphql.js";
import { Message } from "./message.client.js";

import type {
  SnsConversationQuery$variables,
  SnsConversationQuery$data,
} from "./__generated__/SnsConversationQuery.graphql.js";
import type { SnsConversation_conversation$key } from "./__generated__/SnsConversation_conversation.graphql.js";
import type { SnsConversation_query$data } from "./__generated__/SnsConversation_query.graphql.js";
import type { SnsSendMessageMutation } from "./__generated__/SnsSendMessageMutation.graphql.js";

const conversationQuery = graphql`
  query SnsConversationQuery($thread: ID!) {
    ...SnsConversation_query @arguments(thread: $thread)
  }
`;

const conversationQueryFragment = graphql`
  fragment SnsConversation_query on Query
  @refetchable(queryName: "SnsConversationRefetchQuery")
  @argumentDefinitions(thread: { type: "ID!" }) {
    conversation(id: $thread) {
      thread {
        id
      }
      ...SnsConversation_conversation
    }
  }
`;

const conversationFragment = graphql`
  fragment SnsConversation_conversation on Conversation {
    thread {
      id
      participant {
        name
      }
    }
    messages {
      id
      ...SnsMessage_message
    }
  }
`;

const sendMessage = graphql`
  mutation SnsSendMessageMutation($input: MessageInput!) {
    sendMessage(input: $input) {
      id
      ...SnsMessage_message
    }
  }
`;

/**
 * The selected conversation's own preload. A new thread remounts the draft
 * with its log.
 *
 * The store updater appends the message that was just accepted, so the log
 * moves before another round trip. The fragment is then refetched, inside a
 * transition, so the log stays while the server's copy arrives, including a
 * message the other person sent while this one was open. That refetch runs
 * only when its thread is still the one on screen. A send that finishes after
 * the reader opened another thread must not replace this pane.
 *
 * The inbox preview is the other preload, so its refetch query runs from here.
 * A later send waits until the refresh already in flight settles: the same
 * query would otherwise be joined, and the preview would keep the earlier
 * message. A failure leaves the previous preview. The log already has the
 * message, and a failed read of this pane mounts it again from the store.
 */

export component ConversationPane(
  queryRef: PreloadedQueryRef<SnsConversationQuery$variables, SnsConversationQuery$data>,
) {
  const loaded = useQueryFromServer(conversationQuery, queryRef);

  return (
    <RefreshBoundary fallback={<p role="alert">This conversation could not be refreshed.</p>}>
      <OpenConversation loaded={loaded} />
    </RefreshBoundary>
  );
}

component OpenConversation(loaded: SnsConversationQuery$data) {
  const [untyped, refetch] = useRefetchableFragment(conversationQueryFragment, loaded);
  const data: SnsConversation_query$data = untyped;
  const environment = useRelayEnvironment();
  const openThread = match (data.conversation) {
    null | undefined => null,
    const found      => found.thread.id,
  };
  const openThreadRef = useRef<null | string>(null);
  const inboxRefresh  = useRef<Promise<void>>(Promise.resolve());

  useLayoutEffect(() => {
    openThreadRef.current = openThread;
  }, [openThread]);

  const refresh = (thread: string) => {
    if (openThreadRef.current === thread) {
      startTransition(() => {
        refetch({ thread }, { fetchPolicy: "network-only" });
      });
    }
    const pending = inboxRefresh.current.then(
      () =>
        new Promise((resolve) => {
          fetchQuery(
            environment,
            threadsRefetchQuery,
            {},
            { fetchPolicy: "network-only" },
          ).subscribe({
            complete: () => resolve(),
            error() {
              resolve();
            },
          });
        }),
    );
    // A rejected refresh must not stop the next send from starting one.
    inboxRefresh.current = pending.then(
      () => {},
      () => {},
    );
  };

  return match (data.conversation) {
    null | undefined => <p>Conversation not found.</p>,
    const found      =>
      <Conversation
        key={found.thread.id}
        conversationRef={found}
        onSent={() => refresh(found.thread.id)}
      />,
  };
}

component Conversation(conversationRef: SnsConversation_conversation$key, onSent: () => void) {
  const conversation              = useFragment(conversationFragment, conversationRef);
  const [commit,    pending]      = useMutation<
    SnsSendMessageMutation["variables"],
    SnsSendMessageMutation["response"],
  >(sendMessage);
  const [body,      setBody]      = useState("");
  const [requestId, setRequestId] = useState("");
  const [error,     setError]     = useState("");

  return (
    <section {...props(styles.conversation)}>
      <header>
        <strong>{conversation.thread.participant.name}</strong>
      </header>
      <div role="log" aria-label="Messages">
        {conversation.messages.map((message) => (
          <Message key={message.id} messageRef={message} />
        ))}
      </div>
      <form
        {...props(uiStyles.messageComposer)}
        onSubmit={(event) => {
          event.preventDefault();
          const id = requestId || crypto.randomUUID();
          setRequestId(id);
          setError("");
          commit({
            variables: { input: { threadId: conversation.thread.id, body, requestId: id } },
            updater: (store) => {
              const current = store
                .getRoot()
                .getLinkedRecord("conversation", { id: conversation.thread.id });
              const message = store.getRootField("sendMessage");
              if (current && message) {
                const previous = current.getLinkedRecords("messages") ?? [];
                if (!previous.some((item) => item?.getDataID() === message.getDataID()))
                  current.setLinkedRecords([...previous, message], "messages");
              }
            },
            onCompleted: () => {
              setBody("");
              setRequestId("");
              onSent();
            },
            onError: (failure: Error) => setError(failure.message),
          });
        }}
      >
        <textarea
          {...props(styles.messageField)}
          aria-label="Message"
          value={body}
          maxLength={2000}
          required
          onChange={(event) => {
            setBody(event.currentTarget.value);
            setRequestId("");
          }}
        />
        <button type="submit" {...props(uiStyles.button, uiStyles.primary)} disabled={pending}>
          {
            match (pending) {
              true  => "Sending…",
              false => "Send message",
            }
          }
        </button>
        {
          match (error) {
            ""            => null,
            const message => <p role="alert">{message}</p>,
          }
        }
      </form>
    </section>
  );
}

const styles = stylex.create({
  conversation: {
    minWidth     : "0",
    display      : "flex",
    flexDirection: "column",
  },
  messageField: {
    width        : "100%",
    border       : "0",
    background   : "transparent",
    fontSize     : { default: "13px", "@media (max-width: 760px)": "16px" },
    lineHeight   : "1.7",
    minHeight    : "52px",
    maxHeight    : "180px",
    paddingTop   : "6px",
    paddingRight : "0",
    paddingBottom: "6px",
    paddingLeft  : "0",
  },
});
