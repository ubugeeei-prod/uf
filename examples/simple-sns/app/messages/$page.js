// @flow

import * as React from "@uniflowed/react";
import { Suspense, use } from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";

import { DirectMessagesClient } from "./direct-messages-client.js";
import { messagesData } from "../social-queries.js";
import { SocialFrame } from "../social-frame.js";
import { type Message, type MessageThread } from "../social-model.js";

type MessagesData = {|
  readonly messages: Array<Message>,
  readonly threads: Array<MessageThread>,
|};

type MessagesPageData = {|
  readonly messages: Promise<MessagesData>,
|};

export function loader(): MessagesPageData {
  return { messages: messagesData() };
}

component Messages(data: Promise<MessagesData>) renders React.Node {
  const value = use(data);
  return <DirectMessagesClient threads={value.threads} initialMessages={value.messages} />;
}

component Skeleton() renders React.Node {
  return (
    <div {...props(styles.skeleton)}>
      <span />
      <span />
      <span />
    </div>
  );
}

export default component MessagesPage(data: MessagesPageData) renders React.Node {
  return (
    <SocialFrame active="messages">
      <Suspense fallback={<Skeleton />}>
        <Messages data={data.messages} />
      </Suspense>
    </SocialFrame>
  );
}

const styles = stylex.create({
  skeleton: {
    backgroundColor: "#ffffff",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    display: "grid",
    gap: 12,
    minHeight: 420,
    padding: 16,
  },
});
