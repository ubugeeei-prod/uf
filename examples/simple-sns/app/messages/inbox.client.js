"use client";
// @flow

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { AsyncRegion, useRetryableResource } from "../async-region.client.js";
import { threadsData, messagesData } from "../social-queries.js";
import { Avatar, EmptyState, LoadingState, SignInPrompt } from "../ui.js";
import { DirectMessagesClient } from "./direct-messages-client.js";
import type { InboxData, ConversationData, MessageThread } from "../social-model.js";

/** Navigate to one authorized conversation and expose the selected state accessibly. */
export component ThreadLink(thread: MessageThread, selected: boolean) {
  return (
    <Link
      className="thread-link"
      to={`/messages?thread=${encodeURIComponent(thread.id)}`}
      aria-current={selected ? "page" : undefined}
    >
      <Avatar
        user={{
          id: thread.id,
          name: thread.name,
          handle: thread.handle,
          avatar: thread.avatar,
          photo: thread.photo,
          bio: "",
        }}
        small
      />
      <div className="thread-text">
        <strong>{thread.name}</strong>
        <p>{thread.lastMessage || "Start a conversation"}</p>
      </div>
    </Link>
  );
}

/** Constrain the inbox navigation slot to rendered ThreadLink entries. */
export component ThreadList(children: renders* ThreadLink) {
  return (
    <nav className="thread-list" aria-label="Conversations">
      <h2>YOUR CONVERSATIONS</h2>
      {children}
    </nav>
  );
}

component Threads(data: InboxData, selectedId: string) {
  return match (data) {
    {kind: "unauthenticated"} => <SignInPrompt />,
    {kind: "ready", value: const threads} =>
      <ThreadList>
        {threads.map((thread, index) => (
          <ThreadLink
            key={thread.id}
            thread={thread}
            selected={selectedId === "" ? index === 0 : thread.id === selectedId}
          />
        ))}
      </ThreadList>,
  };
}

component Conversation(data: ConversationData) {
  return match (data) {
    {kind: "unauthenticated"} => <SignInPrompt />,
    {kind: "empty"} =>
      <EmptyState title="No conversations yet">Your conversations will appear here.</EmptyState>,
    {kind: "missing"} =>
      <EmptyState title="Conversation unavailable">
        Choose a conversation from your inbox.
      </EmptyState>,
    {kind: "ready", thread: const thread, messages: const messages} =>
      <DirectMessagesClient key={thread.id} thread={thread} initialMessages={messages} />,
  };
}

/** Reveal and retry inbox previews independently from the selected conversation. */
export component InboxRegions(
  threads: Promise<InboxData>,
  conversation: Promise<ConversationData>,
  threadId: string,
) {
  const list = useRetryableResource(threads, threadsData);
  const detail = useRetryableResource(conversation, () => messagesData(threadId));

  return (
    <div className="conversation-layout">
      <AsyncRegion
        resource={list.resource}
        retry={list.retry}
        label="conversations"
        pending={<LoadingState kind="threads" />}
      >
        {(data) => <Threads data={data} selectedId={threadId} />}
      </AsyncRegion>
      <AsyncRegion
        resource={detail.resource}
        retry={detail.retry}
        label="messages"
        pending={<LoadingState kind="conversation" />}
      >
        {(data) => <Conversation data={data} />}
      </AsyncRegion>
    </div>
  );
}
