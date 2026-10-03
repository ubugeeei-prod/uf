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
import { props, stylex } from "@uniflowed/stylex";

import type { PreloadedQueryRef } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import { UserAvatar } from "../_shared/avatar.client.js";
import { styles as uiStyles } from "../_shared/ui.js";
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
  filter  : FeedFilter,
  guest   : React.Node,
) {
  const { viewer } = useQueryFromServer(composerQuery, queryRef);

  return viewer == null ? guest : <Composer viewerRef={viewer} filter={filter} />;
}

component Composer(viewerRef: SnsComposer_viewer$key, filter: FeedFilter) {
  const viewer                    = useFragment(composerFragment, viewerRef);
  const [commit,    pending]      = useMutation<
    SnsCreatePostMutation["variables"],
    SnsCreatePostMutation["response"],
  >(createPost);
  const environment               = useRelayEnvironment();
  const [body,      setBody]      = useState("");
  const [topic,     setTopic]     = useState(
    match (filter.topic) {
      "all"         => "community",
      const channel => channel,
    },
  );
  const [requestId, setRequestId] = useState("");
  const [error,     setError]     = useState("");

  return (
    <form
      {...props(styles.composer)}
      id="compose"
      aria-label="Publish a note"
      onSubmit={(event) => {
        event.preventDefault();
        const id = requestId || crypto.randomUUID();
        setRequestId(id);
        setError("");
        commit({
          variables: { input: { body, topic, requestId: id } },
          onError  : (failure: Error) => setError(failure.message),
          onCompleted: () => {
            setBody("");
            setRequestId("");
            fetchQuery(
              environment,
              timelineQuery,
              {
                topic : filter.topic,
                search: filter.query,
                page  : filter.page,
              },
              { fetchPolicy: "network-only" },
            ).subscribe({ error: () => setError("Published. Refresh the feed to see your note.") });
          },
        });
      }}
    >
      <div {...props(styles.composerBody)}>
        <UserAvatar userRef={viewer} />
        <textarea
          {...props(styles.composerField)}
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
      <footer {...props(styles.composerFooter)}>
        <select
          {...props(styles.composerSelect)}
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
        <span {...props(styles.counter, styles.footerCounter)}>{body.length}/500</span>
        <button type="submit" {...props(uiStyles.button, uiStyles.primary, styles.composerSubmit)} disabled={pending}>
          {
            match (pending) {
              true  => "Publishing…",
              false => "Publish note",
            }
          }
        </button>
      </footer>
      {
        match (error) {
          ""            => null,
          const message =>
            <p role="alert" {...props(uiStyles.postError)}>
              {message}
            </p>,
        }
      }
    </form>
  );
}

const styles = stylex.create({
  composer: {
    overflow      : "hidden",
    scrollMarginTop: "20px",
    background    : "transparent",
    border        : "0",
    borderBottom  : "1px solid var(--line)",
    borderRadius  : "0",
    marginBottom  : "9px",
  },
  composerBody: {
    display      : "flex",
    gap          : { default: "14px", "@media (max-width: 760px)": "11px" },
    paddingTop   : { default: "16px", "@media (max-width: 760px)": "17px" },
    paddingRight : { default: "0", "@media (max-width: 760px)": "14px" },
    paddingBottom: { default: "10px", "@media (max-width: 760px)": "9px" },
    paddingLeft  : { default: "0", "@media (max-width: 760px)": "14px" },
  },
  composerField: {
    background   : "transparent",
    border       : "0",
    flex         : "1",
    minHeight    : { default: "65px", "@media (max-width: 760px)": "70px" },
    fontSize     : { default: "14px", "@media (max-width: 760px)": "16px" },
    lineHeight   : "1.65",
    outlineOffset: "2px",
    paddingTop   : "8px",
    paddingRight : "0",
    paddingBottom: "8px",
    paddingLeft  : "0",
    "::placeholder": {
      color: "#8d8d8d",
    },
  },
  composerFooter: {
    display      : "flex",
    alignItems   : "center",
    gap          : "12px",
    borderTop    : "0",
    paddingTop   : "12px",
    paddingRight : "0",
    paddingBottom: "12px",
    paddingLeft  : "0",
    marginTop    : "0",
    marginRight  : { default: "0", "@media (max-width: 760px)": "14px" },
    marginBottom : "0",
    marginLeft   : { default: "0", "@media (max-width: 760px)": "14px" },
  },
  composerSelect: {
    border       : "1px solid var(--line)",
    background   : "var(--subtle)",
    color        : "#606060",
    borderRadius : "5px",
    fontSize     : "11px",
    maxWidth     : "155px",
    paddingTop   : "7px",
    paddingRight : "9px",
    paddingBottom: "7px",
    paddingLeft  : "9px",
  },
  counter: {
    fontVariantNumeric: "tabular-nums",
    fontSize          : "11px",
    color             : "var(--muted)",
  },
  footerCounter: {
    display: { "@media (max-width: 760px)": "none" },
  },
  composerSubmit: {
    minHeight    : "33px",
    fontSize     : "11px",
    paddingTop   : "7px",
    paddingRight : "12px",
    paddingBottom: "7px",
    paddingLeft  : "12px",
    marginLeft   : "auto",
  },
});
