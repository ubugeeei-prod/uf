"use client";
// @flow

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { graphql, useFragment, useMutation } from "@uniflowed/relay";

import { Message } from "./message.client.js";

import type { SnsConversation_conversation$key } from "./__generated__/SnsConversation_conversation.graphql.js";
import type { SnsSendMessageMutation } from "./__generated__/SnsSendMessageMutation.graphql.js";

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

export component Conversation(conversationRef: SnsConversation_conversation$key) {
  const conversation = useFragment(conversationFragment, conversationRef);
  const [commit, pending] = useMutation<
    SnsSendMessageMutation["variables"],
    SnsSendMessageMutation["response"],
  >(sendMessage);
  const [body, setBody] = useState("");
  const [requestId, setRequestId] = useState("");
  const [error, setError] = useState("");

  return (
    <section className="conversation">
      <header className="conversation-heading">
        <strong>{conversation.thread.participant.name}</strong>
      </header>
      <div className="message-log" role="log" aria-label="Messages">
        {conversation.messages.map((message) => (
          <Message key={message.id} messageRef={message} />
        ))}
      </div>
      <form
        className="message-composer"
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
            },
            onError: (failure: Error) => setError(failure.message),
          });
        }}
      >
        <textarea
          aria-label="Message"
          value={body}
          maxLength={2000}
          required
          onChange={(event) => {
            setBody(event.currentTarget.value);
            setRequestId("");
          }}
        />
        <button type="submit" className="button primary" disabled={pending}>
          {pending ? "Sending…" : "Send message"}
        </button>
        {error ? <p role="alert">{error}</p> : null}
      </form>
    </section>
  );
}
