// @flow

import * as React from "@uniflowed/react";
import { Suspense } from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { props, stylex } from "@uniflowed/stylex";

import type { SearchParams } from "@uniflowed/router";

import { relay } from "../_server/relay.server.js";
import { SocialFrame } from "../_shared/social-frame.js";
import { ActionLink, LoadingState, styles as uiStyles } from "../_shared/ui.js";
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
      <header {...props(uiStyles.pageHeading, uiStyles.feedPageHeading)}>
        <div>
          <h1 {...props(uiStyles.pageTitle)}>Feed</h1>
          <p {...props(uiStyles.pageSummary)}>Notes from the people in your community.</p>
        </div>
      </header>
      <div {...props(styles.feedToolbar)}>
        <nav {...props(styles.feedTabs)} aria-label="Feed channels">
          <Link
            {...props(styles.feedTab)}
            to={feedHref("all", filter.query)}
            aria-current={
              match (filter.topic) {
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
              key={topic}
              to={feedHref(topic, filter.query)}
              aria-current={
                match (filter.topic === topic) {
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
      <SearchNotes filter={filter} key={filter.query} />
      {
        match (filter.query) {
          ""          => null,
          const query =>
            <p {...props(styles.resultLabel)}>Results for “{query}”</p>,
        }
      }
      <div {...props(styles.feedContent)}>
        <Suspense fallback={<LoadingState kind="feed" />}>
          <ComposerSlot
            queryRef={relay.serverPreloadQuery(composerQuery, {})}
            filter={filter}
            guest={
              <div {...props(styles.signInComposer)}>
                <div>
                  <h2 {...props(styles.signInTitle)}>What are you working on?</h2>
                  <p {...props(styles.signInCopy)}>Sign in to post an update or ask a question.</p>
                </div>
                <ActionLink to="/signup" xstyle={styles.signInButton} iconStyle={styles.signInIcon}>
                  Create account
                </ActionLink>
              </div>
            }
          />
          <Timeline
            queryRef={relay.serverPreloadQuery(timelineQuery, {
              topic : filter.topic,
              search: filter.query,
              page  : filter.page,
            })}
            filter={filter}
          />
        </Suspense>
      </div>
    </SocialFrame>
  );
}

const styles = stylex.create({
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
  signInComposer: {
    display        : { default: "flex", "@media (max-width: 760px)": "none" },
    alignItems     : "center",
    justifyContent : "space-between",
    gap            : { default: "18px", "@media (max-width: 760px)": "12px" },
    background     : "transparent",
    border         : "0",
    borderBottom   : "1px solid var(--line)",
    borderRadius   : "0",
    paddingTop     : { default: "12px", "@media (max-width: 760px)": "14px" },
    paddingRight   : { default: "0", "@media (max-width: 760px)": "12px" },
    paddingBottom  : { default: "24px", "@media (max-width: 760px)": "14px" },
    paddingLeft    : { default: "0", "@media (max-width: 760px)": "12px" },
    marginBottom   : "5px",
  },
  signInTitle: {
    fontSize     : { default: "14px", "@media (max-width: 760px)": "12px" },
    fontWeight   : "600",
    letterSpacing: "-0.1px",
  },
  signInCopy: {
    fontSize  : { default: "11px", "@media (max-width: 760px)": "10px" },
    color     : "var(--muted)",
    lineHeight: "1.6",
    marginTop : "6px",
  },
  signInButton: {
    fontSize     : { default: "11px", "@media (max-width: 760px)": "10px" },
    minHeight    : "34px",
    paddingTop   : { default: "9px", "@media (max-width: 760px)": "8px" },
    paddingRight : { default: "15px", "@media (max-width: 760px)": "8px" },
    paddingBottom: { default: "9px", "@media (max-width: 760px)": "8px" },
    paddingLeft  : { default: "15px", "@media (max-width: 760px)": "8px" },
  },
  signInIcon: {
    display: { "@media (max-width: 760px)": "none" },
  },
});
