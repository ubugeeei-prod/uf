// @flow
import * as React from "@uniflowed/react";
import { InboxRegions } from "./inbox.client.js";
import type { LoaderArgs } from "@uniflowed/router";
import { SocialFrame } from "../social-frame.js";
import { sessionData, threadsData, messagesData } from "../social-queries.js";
import { SignInPrompt } from "../ui.js";
import type { Session, InboxData, ConversationData, MessageThread } from "../social-model.js";
export type Data = {|
  readonly session: Session,
  readonly threadId: string,
  readonly threads: Promise<InboxData>,
  readonly conversation: Promise<ConversationData>,
|};
export async function loader({ searchParams }: LoaderArgs): Promise<Data> {
  const threadId = String(searchParams.thread ?? "");
  // Start both reads before awaiting identity; one pane never waits for the other.
  const session = sessionData();
  const threads = threadsData();
  const conversation = messagesData(threadId);
  return { session: await session, threadId, threads, conversation };
}
export component Page(data: Data) {
  return (
    <SocialFrame active="messages" session={data.session} aside={false}>
      <header className="page-heading">
        <div>
          <h1>Inbox</h1>
          <p>Your private conversations.</p>
        </div>
      </header>
      {
        match (data.session) {
          {kind: "guest"} => <SignInPrompt title="Sign in to open your inbox" />,
          {kind: "authenticated", ...} =>
            <InboxRegions
              key={data.threadId}
              threads={data.threads}
              conversation={data.conversation}
              threadId={data.threadId}
            />,
        }
      }
    </SocialFrame>
  );
}
