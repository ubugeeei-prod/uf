// @flow

import * as React from "@uniflowed/react";
import { Suspense } from "@uniflowed/react";

import type { SearchParams } from "@uniflowed/router";

import { preloadSession, relay } from "../_server/relay.server.js";
import { SignedIn } from "../_shared/session.client.js";
import { SocialFrame } from "../_shared/social-frame.js";
import { LoadingState, SignInPrompt } from "../_shared/ui.js";
import conversationQuery from "./__generated__/SnsConversationQuery.graphql.js";
import threadsQuery from "./__generated__/SnsThreadsQuery.graphql.js";
import { ConversationPane } from "./conversation.client.js";
import { Threads } from "./thread.client.js";

export const dynamic = "force-dynamic";

/**
 * Three preloads start together: identity, the thread list and the selected conversation.
 * Each pane streams into its own boundary, so neither waits for the other.
 */

export component Page(searchParams: SearchParams) {
  const thread = String(searchParams.thread ?? "thread-mika");
  const session = preloadSession();
  const threadsPending = <LoadingState kind="threads" />;
  const conversationPending = <LoadingState kind="conversation" />;

  return (
    <SocialFrame active="messages" aside={false} session={session}>
      <header className="page-heading">
        <div>
          <h1>Inbox</h1>
          <p>Your conversations, one at a time.</p>
        </div>
      </header>
      <Suspense
        fallback={
          <div className="conversation-layout">
            {threadsPending}
            {conversationPending}
          </div>
        }
      >
        <SignedIn queryRef={session} guest={<SignInPrompt title="Sign in to read your messages" />}>
          <div className="conversation-layout">
            <Suspense fallback={threadsPending}>
              <Threads queryRef={relay.serverPreloadQuery(threadsQuery, {})} selected={thread} />
            </Suspense>
            <Suspense fallback={conversationPending}>
              <ConversationPane
                queryRef={relay.serverPreloadQuery(conversationQuery, { thread })}
              />
            </Suspense>
          </div>
        </SignedIn>
      </Suspense>
    </SocialFrame>
  );
}
