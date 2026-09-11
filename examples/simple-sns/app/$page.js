// @flow
import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import type { LoaderArgs } from "@uniflowed/router";
import { SocialFrame } from "./social-frame.js";
import { sessionData, timelineData } from "./social-queries.js";
import { TimelineClient } from "./timeline-client.js";
import { SearchNotes } from "./search.client.js";
import {
  TOPICS,
  feedHref,
  feedFilter,
  topicLabel,
  type FeedData,
  type FeedFilter,
  type Session,
} from "./social-model.js";
export type Data = {|
  readonly session: Session,
  readonly filter: FeedFilter,
  readonly feed: Promise<FeedData>,
|};
export async function loader({ searchParams }: LoaderArgs): Promise<Data> {
  const filter = feedFilter(
    String(searchParams.topic ?? "all"),
    String(searchParams.q ?? ""),
    String(searchParams.page ?? "1"),
  );
  const session = sessionData();
  const feed = timelineData(filter.topic, filter.query, String(filter.page));
  return { session: await session, filter, feed };
}
export component Page(data: Data) {
  const feed = data.filter;
  return (
    <SocialFrame active="timeline" session={data.session}>
      <header className="page-heading">
        <div>
          <h1>Feed</h1>
          <p>Notes from the people in your community.</p>
        </div>
      </header>
      <div className="feed-toolbar">
        <nav className="feed-tabs" aria-label="Feed channels">
          <Link
            to={feedHref("all", feed.query)}
            aria-current={feed.topic === "all" ? "page" : undefined}
          >
            All notes
          </Link>
          {TOPICS.map((topic) => (
            <Link
              to={feedHref(topic, feed.query)}
              key={topic}
              aria-current={feed.topic === topic ? "page" : undefined}
            >
              {topicLabel(topic)}
            </Link>
          ))}
        </nav>
      </div>
      <SearchNotes key={feed.query} filter={feed} />
      {feed.query ? <p className="result-label">Results for “{feed.query}”</p> : null}
      <div className="feed-content">
        <TimelineClient
          key={`${feed.topic}:${feed.query}:${feed.page}`}
          initial={data.feed}
          filter={feed}
          session={data.session}
        />
      </div>
    </SocialFrame>
  );
}
