// @flow

import * as React from "@uniflowed/react";
import { Suspense } from "@uniflowed/react";
import { Link } from "@uniflowed/router";

import type { SearchParams } from "@uniflowed/router";

import { relay } from "../_server/relay.server.js";
import { styled, styles as sharedStyles } from "../_shared/commonplace.stylex.js";
import { SocialFrame } from "../_shared/social-frame.js";
import { ActionLink, LoadingState } from "../_shared/ui.js";
import { TOPICS, feedFilter, feedHref, topicLabel } from "../_shared/social-model.js";
import composerQuery from "./__generated__/SnsComposerQuery.graphql.js";
import timelineQuery from "./__generated__/SnsTimelineQuery.graphql.js";
import { ComposerSlot } from "./composer.client.js";
import { SearchNotes } from "./search.client.js";
import { Timeline } from "./timeline.client.js";

export const dynamic = "force-dynamic";

/**
 * The feed's heading, channels and URL state render on the server. The composer and the timeline
 * are separate islands: both preloads start in this pass and reveal under one boundary.
 */

export component Page(searchParams: SearchParams) {
  const filter = feedFilter(
    String(searchParams.topic ?? "all"),
    String(searchParams.q ?? ""),
    String(searchParams.page ?? "1"),
  );

  return (
    <SocialFrame active="timeline">
      <header className="page-heading">
        <div>
          <h1>Feed</h1>
          <p>Notes from the people in your community.</p>
        </div>
      </header>
      <div {...styled("feed-toolbar", sharedStyles.feedToolbar)}>
        <nav className="feed-tabs" aria-label="Feed channels">
          <Link
            to={feedHref("all", filter.query)}
            aria-current={filter.topic === "all" ? "page" : undefined}
          >
            All notes
          </Link>
          {TOPICS.map((topic) => (
            <Link
              key={topic}
              to={feedHref(topic, filter.query)}
              aria-current={filter.topic === topic ? "page" : undefined}
            >
              {topicLabel(topic)}
            </Link>
          ))}
        </nav>
      </div>
      <SearchNotes filter={filter} key={filter.query} />
      {filter.query ? (
        <p {...styled("result-label", sharedStyles.resultLabel)}>Results for “{filter.query}”</p>
      ) : null}
      <div className="feed-content">
        <Suspense fallback={<LoadingState kind="feed" />}>
          <ComposerSlot
            queryRef={relay.serverPreloadQuery(composerQuery, {})}
            filter={filter}
            guest={
              <div className="sign-in-composer">
                <div>
                  <h2>What are you working on?</h2>
                  <p>Sign in to post an update or ask a question.</p>
                </div>
                <ActionLink to="/signup">Create account</ActionLink>
              </div>
            }
          />
          <Timeline
            queryRef={relay.serverPreloadQuery(timelineQuery, {
              topic: filter.topic,
              search: filter.query,
              page: filter.page,
            })}
            filter={filter}
          />
        </Suspense>
      </div>
    </SocialFrame>
  );
}
