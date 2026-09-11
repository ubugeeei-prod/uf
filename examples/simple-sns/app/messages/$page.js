// @flow

import * as React from "@uniflowed/react";
import { InboxRegions } from "./inbox.client.js";
import type { LoaderArgs } from "@uniflowed/router";
import { SocialFrame } from "../social-frame.js";
import { sessionData, threadsData, messagesData } from "../social-queries.js";
import { SignInPrompt } from "../ui.js";
import type { Session, InboxData, ConversationData, MessageThread } from "../social-model.js";

/** Resolved shell identity with independently deferred inbox and conversation reads. */
export type Data = {|
  readonly session: Session,
  readonly threadId: string,
  readonly threads: Promise<InboxData>,
  readonly conversation: Promise<ConversationData>,
|};

/** Start the inbox and conversation together, awaiting only the identity used by navigation. */
export async function loader({ searchParams }: LoaderArgs): Promise<Data> {
  const threadId = String(searchParams.thread ?? "");
  // Start both reads before awaiting identity; one pane never waits for the other.
  const session = sessionData();
  const threads = threadsData();
  const conversation = messagesData(threadId);

  return { session: await session, threadId, threads, conversation };
}

/** Compose the inbox shell without serializing the two independent content reads. */
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
              threads={data.threads}
              conversation={data.conversation}
              threadId={data.threadId}
            />,
        }
      }
    </SocialFrame>
  );
}
