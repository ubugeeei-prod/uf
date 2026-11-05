"use client";
// @flow

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { graphql, useRefetchableFragment } from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";
import { props, stylex } from "@uniflowed/stylex";

import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import { Icon, EmptyState } from "../_shared/ui.js";
import { ComposerSlot } from "./composer.client.js";
import { PostCard } from "./post.client.js";
import { feedHref, type FeedFilter } from "../_shared/social-model.js";

import type {
  SnsComposerQuery$variables,
  SnsComposerQuery$data,
} from "./__generated__/SnsComposerQuery.graphql.js";
import type {
  SnsTimelineQuery$variables,
  SnsTimelineQuery$data,
} from "./__generated__/SnsTimelineQuery.graphql.js";
import type { SnsTimeline_query$data } from "./__generated__/SnsTimeline_query.graphql.js";

const timelineQuery = graphql`
  query SnsTimelineQuery($topic: String!, $search: String!, $page: Int!) {
    ...SnsTimeline_query @arguments(topic: $topic, search: $search, page: $page)
  }
`;

const timelineFragment = graphql`
  fragment SnsTimeline_query on Query
  @refetchable(queryName: "SnsTimelineRefetchQuery")
  @argumentDefinitions(
    topic: { type: "String!" }
    search: { type: "String!" }
    page: { type: "Int!" }
  ) {
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
 * The composer and the notes.
 *
 * The route query only starts the read. The list is a refetchable fragment, so
 * publishing a note asks that fragment for the same topic, search and page
 * again. The route does not run a second time, and a reader who has moved to
 * another page is not pulled back to this one. Reactions still write the
 * normalized record; this refresh is for a note that did not exist yet.
 * Pagination stays on the URL, which is what a shared link has to carry.
 */

export component Feed(
  composerQueryRef: PreloadedQueryRef<SnsComposerQuery$variables, SnsComposerQuery$data>,
  timelineQueryRef: PreloadedQueryRef<SnsTimelineQuery$variables, SnsTimelineQuery$data>,
  filter          : FeedFilter,
  guest           : React.Node,
) {
  const loaded             = useQueryFromServer(timelineQuery, timelineQueryRef);
  const [untyped, refetch] = useRefetchableFragment(timelineFragment, loaded);
  // The tag's result is the artifact's data. Naming that type keeps `hasNext`
  // a boolean; an untyped fragment result would be `any`, and a match on it
  // would no longer be exhaustive.
  const data: SnsTimeline_query$data = untyped;
  const posts = data.feed.posts;
  const refresh = (): Promise<void> =>
    new Promise((resolve, reject) => {
      refetch(
        { topic: filter.topic, search: filter.query, page: filter.page },
        {
          fetchPolicy: "network-only",
          onComplete: (error) => {
            if (error != null) {
              reject(error);
            } else {
              resolve();
            }
          },
        },
      );
    });

  return (
    <>
      <ComposerSlot
        queryRef={composerQueryRef}
        filter={filter}
        guest={guest}
        onPublished={refresh}
      />
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
