"use client";
// @flow

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { graphql } from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";
import { props, stylex } from "@uniflowed/stylex";

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
  filter  : FeedFilter,
) {
  const data = useQueryFromServer(timelineQuery, queryRef);
  const posts = data.feed.posts;

  return (
    <>
      {
        match (posts.length > 0) {
          true  =>
            <section aria-label="Timeline posts">
              {posts.map((post) => (
                <PostCard key={post.id} postRef={post} signedIn={data.viewer != null} />
              ))}
            </section>,
          false =>
            <EmptyState title="No notes here yet">Try another channel or search.</EmptyState>,
        }
      }
      <nav {...props(styles.pagination)} aria-label="Feed pagination">
        {
          match (filter.page > 1) {
            true  =>
              <Link
                {...props(styles.paginationLink)}
                to={feedHref(filter.topic, filter.query, filter.page - 1)}
              >
                Newer notes
              </Link>,
            false => <span>Latest notes</span>,
          }
        }
        {
          match (data.feed.hasNext) {
            true  =>
              <Link
                {...props(styles.paginationLink)}
                to={feedHref(filter.topic, filter.query, filter.page + 1)}
              >
                Older notes <Icon name="arrow" size={14} />
              </Link>,
            false => null,
          }
        }
      </nav>
    </>
  );
}

const styles = stylex.create({
  pagination: {
    display       : "flex",
    justifyContent: "space-between",
    alignItems    : "center",
    gap           : "20px",
    fontSize      : "12px",
    color         : "var(--muted)",
    marginTop     : "24px",
  },
  paginationLink: {
    display   : "flex",
    alignItems: "center",
    gap       : "8px",
  },
});
