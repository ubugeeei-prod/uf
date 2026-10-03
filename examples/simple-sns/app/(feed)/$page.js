// @flow

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { props, stylex } from "@uniflowed/stylex";

import type { LoaderArgs } from "@uniflowed/router";

import { SocialFrame, styles as frameStyles } from "../_shared/social-frame.js";
import { sessionData, timelineData } from "../_server/social-queries.js";
import { TimelineClient } from "./timeline.client.js";
import { SearchNotes } from "./search.client.js";
import {
  TOPICS,
  feedHref,
  feedFilter,
  topicLabel,
  type FeedData,
  type FeedFilter,
  type Session,
} from "../_shared/social-model.js";

/** Resolved shell identity and URL state with an independently deferred feed. */

export type Data = {|
  readonly session: Session,
  readonly filter : FeedFilter,
  readonly feed   : Promise<FeedData>,
|};

/** Start session and feed reads together; await only the identity needed by the page shell. */

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

/** Render the stable feed shell while the timeline region owns its deferred content. */

export component Page(data: Data) {
  const feed = data.filter;

  return (
    <SocialFrame active="timeline" session={data.session}>
      <header {...props(frameStyles.pageHeading, styles.feedPageHeading)}>
        <div>
          <h1 {...props(frameStyles.pageTitle)}>Feed</h1>
          <p {...props(frameStyles.pageLede)}>Notes from the people in your community.</p>
        </div>
      </header>
      <div {...props(styles.feedToolbar)}>
        <nav {...props(styles.feedTabs)} aria-label="Feed channels">
          <Link
            {...props(styles.feedTab)}
            to={feedHref("all", feed.query)}
            aria-current={
              match (feed.topic) {
                "all" => "page",
                _     => undefined,
              }
            }
          >
            All notes
          </Link>
          {TOPICS.map((topic) => (
            <Link
              {...props(styles.feedTab)}
              to={feedHref(topic, feed.query)}
              key={topic}
              aria-current={
                match (feed.topic === topic) {
                  true  => "page",
                  false => undefined,
                }
              }
            >
              {topicLabel(topic)}
            </Link>
          ))}
        </nav>
      </div>
      <SearchNotes key={feed.query} filter={feed} />
      {
        match (feed.query) {
          ""          => null,
          const query =>
            <p {...props(styles.resultLabel)}>Results for “{query}”</p>,
        }
      }
      <div {...props(styles.feedContent)}>
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

const styles = stylex.create({
  feedPageHeading: {
    marginBottom : { default: "22px", "@media (max-width: 760px)": "-1px" },
    position     : { "@media (max-width: 760px)": "absolute" },
    width        : { "@media (max-width: 760px)": "1px" },
    height       : { "@media (max-width: 760px)": "1px" },
    overflow     : { "@media (max-width: 760px)": "hidden" },
    clipPath     : { "@media (max-width: 760px)": "inset(50%)" },
    whiteSpace   : { "@media (max-width: 760px)": "nowrap" },
    paddingTop   : { "@media (max-width: 760px)": "0" },
    paddingRight : { "@media (max-width: 760px)": "0" },
    paddingBottom: { "@media (max-width: 760px)": "0" },
    paddingLeft  : { "@media (max-width: 760px)": "0" },
    marginTop    : { "@media (max-width: 760px)": "-1px" },
    marginRight  : { "@media (max-width: 760px)": "-1px" },
    marginLeft   : { "@media (max-width: 760px)": "-1px" },
  },
  feedToolbar: {
    borderBottom: "1px solid var(--line)",
  },
  feedTabs: {
    display      : "flex",
    gap          : { default: "24px", "@media (max-width: 760px)": "20px" },
    minWidth     : "0",
    overflow     : "auto",
    scrollbarWidth: { "@media (max-width: 760px)": "none" },
  },
  feedTab: {
    fontSize     : "12px",
    whiteSpace   : "nowrap",
    color        : "var(--muted)",
    borderBottom : "2px solid transparent",
    paddingTop   : "14px",
    paddingRight : "0",
    paddingBottom: { default: "13px", "@media (max-width: 760px)": "12px" },
    paddingLeft  : "0",
    minHeight    : { "@media (max-width: 760px)": "44px" },
    ":is([aria-current=page])": {
      color     : "var(--ink)",
      borderColor: "var(--accent)",
      fontWeight: "600",
    },
  },
  resultLabel: {
    fontSize : "12px",
    color    : "var(--muted)",
    marginTop: "18px",
  },
  feedContent: {
    paddingTop: { default: "20px", "@media (max-width: 760px)": "0" },
  },
});
