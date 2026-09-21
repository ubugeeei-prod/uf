"use client";
// @flow

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { graphql } from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";

import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import { Icon, EmptyState } from "../_shared/ui.js";
import { PostCard } from "./post.client.js";
import { feedHref, type FeedFilter } from "../_shared/social-model.js";

import type {
  SnsTimelineQuery$variables,
  SnsTimelineQuery$data,
} from "./__generated__/SnsTimelineQuery.graphql.js";

const timelineQuery = graphql`
  query SnsTimelineQuery($topic: String!, $search: String!, $page: Int!) {
    viewer {
      id
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

/**
 * The notes and their pagination. Reactions and the composer's refresh write to the normalized
 * store, so this list lives in the browser; the page around it does not.
 */

export component Timeline(
  queryRef: PreloadedQueryRef<SnsTimelineQuery$variables, SnsTimelineQuery$data>,
  filter: FeedFilter,
) {
  const data = useQueryFromServer(timelineQuery, queryRef);
  const posts = data.feed.posts;

  return (
    <>
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
        {data.feed.hasNext ? (
          <Link to={feedHref(filter.topic, filter.query, filter.page + 1)}>
            Older notes <Icon name="arrow" size={14} />
          </Link>
        ) : null}
      </nav>
    </>
  );
}
