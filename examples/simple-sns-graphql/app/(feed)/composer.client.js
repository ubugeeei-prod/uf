"use client";
// @flow

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import {
  graphql,
  useFragment,
  useMutation,
  useRelayEnvironment,
  fetchQuery,
} from "@uniflowed/relay";
import { useQueryFromServer } from "@uniflowed/relay/rsc-client_EXPERIMENTAL";

import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import { styled, styles as sharedStyles } from "../_shared/commonplace.stylex.js";
import { UserAvatar } from "../_shared/avatar.client.js";
import timelineQuery from "./__generated__/SnsTimelineQuery.graphql.js";
import { TOPICS, topicLabel, type FeedFilter } from "../_shared/social-model.js";

import type {
  SnsComposerQuery$variables,
  SnsComposerQuery$data,
} from "./__generated__/SnsComposerQuery.graphql.js";
import type { SnsComposer_viewer$key } from "./__generated__/SnsComposer_viewer.graphql.js";
import type { SnsCreatePostMutation } from "./__generated__/SnsCreatePostMutation.graphql.js";

const composerQuery = graphql`
  query SnsComposerQuery {
    viewer {
      ...SnsComposer_viewer
    }
  }
`;

const composerFragment = graphql`
  fragment SnsComposer_viewer on User {
    name
    ...SnsAvatar_user
  }
`;

const createPost = graphql`
  mutation SnsCreatePostMutation($input: PostInput!) {
    createPost(input: $input) {
      id
      ...SnsPost_post
    }
  }
`;

/** Read the viewer's own preload; `guest` is the server-rendered invitation for everyone else. */

export component ComposerSlot(
  queryRef: PreloadedQueryRef<SnsComposerQuery$variables, SnsComposerQuery$data>,
  filter: FeedFilter,
  guest: React.Node,
) {
  const { viewer } = useQueryFromServer(composerQuery, queryRef);

  return viewer == null ? guest : <Composer viewerRef={viewer} filter={filter} />;
}

component Composer(viewerRef: SnsComposer_viewer$key, filter: FeedFilter) {
  const viewer = useFragment(composerFragment, viewerRef);
  const [commit, pending] = useMutation<
    SnsCreatePostMutation["variables"],
    SnsCreatePostMutation["response"],
  >(createPost);
  const environment = useRelayEnvironment();
  const [body, setBody] = useState("");
  const [topic, setTopic] = useState(filter.topic === "all" ? "community" : filter.topic);
  const [requestId, setRequestId] = useState("");
  const [error, setError] = useState("");

  return (
    <form
      className="composer"
      id="compose"
      aria-label="Publish a note"
      onSubmit={(event) => {
        event.preventDefault();
        const id = requestId || crypto.randomUUID();
        setRequestId(id);
        setError("");
        commit({
          variables: { input: { body, topic, requestId: id } },
          onError: (failure: Error) => setError(failure.message),
          onCompleted: () => {
            setBody("");
            setRequestId("");
            fetchQuery(
              environment,
              timelineQuery,
              {
                topic: filter.topic,
                search: filter.query,
                page: filter.page,
              },
              { fetchPolicy: "network-only" },
            ).subscribe({ error: () => setError("Published. Refresh the feed to see your note.") });
          },
        });
      }}
    >
      <div className="composer-body">
        <UserAvatar userRef={viewer} />
        <textarea
          name="body"
          aria-label="Post body"
          placeholder={`What are you working on, ${viewer.name.split(" ")[0]}?`}
          required
          maxLength={500}
          value={body}
          onChange={(event) => {
            setBody(event.currentTarget.value);
            setRequestId("");
          }}
        />
      </div>
      <footer className="composer-footer">
        <select
          aria-label="Post channel"
          value={topic}
          onChange={(event) => {
            setTopic(event.currentTarget.value);
            setRequestId("");
          }}
        >
          {TOPICS.map((value) => (
            <option key={value} value={value}>
              {topicLabel(value)}
            </option>
          ))}
        </select>
        <span className="counter">{body.length}/500</span>
        <button type="submit" className="button primary" disabled={pending}>
          {pending ? "Publishing…" : "Publish note"}
        </button>
      </footer>
      {error ? (
        <p role="alert" {...styled("post-error", sharedStyles.postError)}>
          {error}
        </p>
      ) : null}
    </form>
  );
}
