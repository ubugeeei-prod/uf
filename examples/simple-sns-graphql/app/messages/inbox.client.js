"use client";
// @flow
import * as React from "@uniflowed/react";
import { graphql, useFragment } from "@uniflowed/relay";
import { SignInPrompt } from "../_shared/ui.js";
import { Thread } from "./thread.client.js";
import { Conversation } from "./conversation.client.js";
import type { SnsInbox_query$key } from "./__generated__/SnsInbox_query.graphql.js";

const inboxFragment = graphql`
  fragment SnsInbox_query on Query @argumentDefinitions(thread: { type: "ID!" }) {
    viewer {
      id
    }
    threads {
      id
      ...SnsThread_thread
    }
    conversation(id: $thread) {
      thread {
        id
      }
      ...SnsConversation_conversation
    }
  }
`;

/** The inbox composes fragment references; it cannot read its children's fields. */
export component Inbox(queryRef: SnsInbox_query$key) {
  const data = useFragment(inboxFragment, queryRef);
  if (data.viewer == null) return <SignInPrompt title="Sign in to read your messages" />;
  const conversation = data.conversation;
  return (
    <>
      <header className="page-heading">
        <div>
          <h1>Inbox</h1>
          <p>Your conversations, one at a time.</p>
        </div>
      </header>
      <div className="conversation-layout">
        <nav className="thread-list" aria-label="Conversations">
          {data.threads.map((thread) => (
            <Thread
              key={thread.id}
              threadRef={thread}
              selected={conversation?.thread.id === thread.id}
            />
          ))}
        </nav>
        {conversation ? (
          <Conversation key={conversation.thread.id} conversationRef={conversation} />
        ) : (
          <p>Conversation not found.</p>
        )}
      </div>
    </>
  );
}
