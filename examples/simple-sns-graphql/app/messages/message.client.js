"use client";
// @flow

import * as React from "@uniflowed/react";
import { graphql, useFragment } from "@uniflowed/relay";

import { displayTime } from "../_shared/social-model.js";

import type { SnsMessage_message$key } from "./__generated__/SnsMessage_message.graphql.js";

const messageFragment = graphql`
  fragment SnsMessage_message on Message {
    author
    body
    sentAt
  }
`;

export component Message(messageRef: SnsMessage_message$key) {
  const message = useFragment(messageFragment, messageRef);

  return (
    <div className={`message ${message.author === "me" ? "mine" : ""}`}>
      <p className="message-content">{message.body}</p>
      <time dateTime={message.sentAt}>{displayTime(message.sentAt)}</time>
    </div>
  );
}
