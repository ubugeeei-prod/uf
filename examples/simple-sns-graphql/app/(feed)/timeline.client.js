"use client";
// @flow
import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { graphql, useFragment } from "@uniflowed/relay";
import { styled, styles as sharedStyles } from "../_shared/commonplace.stylex.js";
import { Icon, ActionLink, EmptyState } from "../_shared/ui.js";
import { SearchNotes } from "./search.client.js";
import { PostCard } from "./post.client.js";
import { Composer } from "./composer.client.js";
import { topicLabel, TOPICS, feedHref, type FeedFilter } from "../_shared/social-model.js";
import type { SnsTimeline_query$key } from "./__generated__/SnsTimeline_query.graphql.js";

const timelineFragment = graphql`
  fragment SnsTimeline_query on Query
  @argumentDefinitions(
    topic: { type: "String!" }
    search: { type: "String!" }
    page: { type: "Int!" }
  ) {
    viewer {
      ...SnsComposer_viewer
    }
    feed(topic: $topic, search: $search, page: $page) {
      posts {
        id
        ...SnsPost_post
      }
      hasNext
    }
  }
`;

/** The same Commonplace feed; data and fragment ownership belong to Relay. */
export component Timeline(queryRef: SnsTimeline_query$key, filter: FeedFilter) {
  const data = useFragment(timelineFragment, queryRef);
  const posts = data.feed?.posts ?? [];
  return (
    <>
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
        {data.viewer ? (
          <Composer viewerRef={data.viewer} filter={filter} />
        ) : (
          <div className="sign-in-composer">
            <div>
              <h2>What are you working on?</h2>
              <p>Sign in to post an update or ask a question.</p>
            </div>
            <ActionLink to="/signup">Create account</ActionLink>
          </div>
        )}
        {posts.length ? (
          <section aria-label="Timeline posts">
            {posts.map((post) => (
              <PostCard key={post.id} postRef={post} signedIn={data.viewer != null} />
            ))}
          </section>
        ) : (
          <EmptyState title="No notes here yet">Try another channel or search.</EmptyState>
        )}
        <nav className="pagination" aria-label="Feed pagination">
          {filter.page > 1 ? (
            <Link to={feedHref(filter.topic, filter.query, filter.page - 1)}>Newer notes</Link>
          ) : (
            <span>Latest notes</span>
          )}
          {data.feed?.hasNext ? (
            <Link to={feedHref(filter.topic, filter.query, filter.page + 1)}>
              Older notes <Icon name="arrow" size={14} />
            </Link>
          ) : null}
        </nav>
      </div>
    </>
  );
}
