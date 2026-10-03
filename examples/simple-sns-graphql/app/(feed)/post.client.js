"use client";
// @flow

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { graphql, useFragment, useMutation } from "@uniflowed/relay";
import { props, stylex } from "@uniflowed/stylex";

import { UserAvatar } from "../_shared/avatar.client.js";
import { Icon, styles as uiStyles } from "../_shared/ui.js";
import { displayDate, topicFrom, topicLabel, feedHref } from "../_shared/social-model.js";

import type { SnsPost_post$key } from "./__generated__/SnsPost_post.graphql.js";
import type { SnsAppreciateMutation } from "./__generated__/SnsAppreciateMutation.graphql.js";

const postFragment = graphql`
  fragment SnsPost_post on Post {
    id
    body
    topic
    likes
    liked
    createdAt
    author {
      name
      handle
      ...SnsAvatar_user
    }
  }
`;

const appreciate = graphql`
  mutation SnsAppreciateMutation($id: ID!, $liked: Boolean!) {
    setAppreciation(id: $id, liked: $liked) {
      id
      likes
      liked
    }
  }
`;

/** Relay owns the optimistic layer and rolls it back when a request fails. */

export component PostCard(postRef: SnsPost_post$key, signedIn: boolean) {
  const post               = useFragment(postFragment, postRef);
  const [commit, pending]  = useMutation<
    SnsAppreciateMutation["variables"],
    SnsAppreciateMutation["response"],
  >(appreciate);
  const [error,  setError] = useState("");
  const topic = topicFrom(post.topic) ?? "community";

  return (
    <article {...props(uiStyles.post)} aria-busy={pending}>
      <UserAvatar userRef={post.author} />
      <div {...props(uiStyles.postContent)}>
        <header {...props(uiStyles.postHeader)}>
          <strong {...props(styles.postName)}>{post.author.name}</strong>
          <span {...props(styles.postHandle)}>@{post.author.handle}</span>
          <time {...props(styles.postTime)} dateTime={post.createdAt}>{displayDate(post.createdAt)}</time>
        </header>
        <p {...props(uiStyles.postBody)}>{post.body}</p>
        <footer {...props(uiStyles.postFooter)}>
          <Link {...props(styles.channelBadge)} to={feedHref(topic)}>
            <span {...props(uiStyles.channelDot, styles.channelDotSmall)} />
            {topicLabel(topic)}
          </Link>
          {
            match (signedIn) {
              true  =>
                <button
                  type="button"
                  {...props(styles.reaction)}
                  disabled={pending}
                  aria-pressed={post.liked}
                  aria-label={`${match (post.liked === true) {
                    true  => "Remove appreciation",
                    false => "Appreciate",
                  }} · ${post.likes}`}
                  onClick={() => {
                    setError("");
                    commit({
                      variables: { id: post.id, liked: !post.liked },
                      optimisticResponse: {
                        setAppreciation: {
                          id   : post.id,
                          liked: !post.liked,
                          likes: post.likes + (post.liked ? -1 : 1),
                        },
                      },
                      onError: () => setError("Could not save. Try again."),
                    });
                  }}
                >
                  <Icon name="heart" size={16} {...props(post.liked === true && styles.reactionIcon)} />
                  <span>{post.likes}</span>
                </button>,
              false =>
                <Link
                  {...props(styles.reaction)}
                  to="/login"
                  aria-label={`Sign in to appreciate · ${post.likes}`}
                >
                  <Icon name="heart" size={16} />
                  <span>{post.likes}</span>
                </Link>,
            }
          }
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
      </div>
    </article>
  );
}

const styles = stylex.create({
  postName: {
    fontSize  : { default: "13px", "@media (max-width: 760px)": "12px" },
    fontWeight: "600",
  },
  postHandle: {
    fontSize: { default: "11px", "@media (max-width: 760px)": "10px" },
    color   : "var(--muted)",
  },
  postTime: {
    fontSize  : { default: "11px", "@media (max-width: 760px)": "10px" },
    color     : "var(--muted)",
    marginLeft: "auto",
  },
  channelBadge: {
    display      : "inline-flex",
    gap          : "6px",
    alignItems   : "center",
    borderRadius : "4px",
    fontSize     : "10px",
    color        : "#6b6b6b",
    background   : "transparent",
    border       : "0",
    paddingTop   : "4px",
    paddingRight : "0",
    paddingBottom: "4px",
    paddingLeft  : "0",
  },
  channelDotSmall: {
    width : "4px",
    height: "4px",
  },
  reaction: {
    display      : "inline-flex",
    alignItems   : "center",
    gap          : "7px",
    background   : "transparent",
    border       : "0",
    color        : "#818181",
    fontSize     : "11px",
    minHeight    : "32px",
    paddingTop   : "7px",
    paddingRight : "7px",
    paddingBottom: "7px",
    paddingLeft  : "7px",
    ":hover"     : {
      color: "#252525",
    },
    ":is([aria-pressed=true])": {
      color: "#252525",
    },
  },
  reactionIcon: {
    fill: "currentColor",
  },
});
