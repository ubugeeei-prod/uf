"use client";
// @flow
import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { graphql, useFragment, useMutation } from "@uniflowed/relay";
import { styled, styles as sharedStyles } from "./commonplace.stylex.js";
import { UserAvatar } from "./avatar.client.js";
import { Icon } from "./ui.js";
import { displayDate, topicFrom, topicLabel, feedHref } from "./social-model.js";
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
  const post = useFragment(postFragment, postRef);
  const [commit, pending] = useMutation<
    SnsAppreciateMutation["variables"],
    SnsAppreciateMutation["response"],
  >(appreciate);
  const [error, setError] = useState("");
  const topic = topicFrom(post.topic) ?? "community";
  return (
    <article className="post" aria-busy={pending}>
      <UserAvatar userRef={post.author} />
      <div {...styled("post-content", sharedStyles.postContent)}>
        <header className="post-header">
          <strong>{post.author.name}</strong>
          <span className="handle">@{post.author.handle}</span>
          <time dateTime={post.createdAt}>{displayDate(post.createdAt)}</time>
        </header>
        <p className="post-body">{post.body}</p>
        <footer {...styled("post-footer", sharedStyles.postFooter)}>
          <Link className="channel-badge" to={feedHref(topic)}>
            <span className={`channel-dot ${topic}`} />
            {topicLabel(topic)}
          </Link>
          {signedIn ? (
            <button
              type="button"
              className="reaction"
              disabled={pending}
              aria-pressed={post.liked}
              aria-label={`${post.liked ? "Remove appreciation" : "Appreciate"} · ${post.likes}`}
              onClick={() => {
                setError("");
                commit({
                  variables: { id: post.id, liked: !post.liked },
                  optimisticResponse: {
                    setAppreciation: {
                      id: post.id,
                      liked: !post.liked,
                      likes: post.likes + (post.liked ? -1 : 1),
                    },
                  },
                  onError: () => setError("Could not save. Try again."),
                });
              }}
            >
              <Icon name="heart" size={16} />
              <span>{post.likes}</span>
            </button>
          ) : (
            <Link
              className="reaction"
              to="/login"
              aria-label={`Sign in to appreciate · ${post.likes}`}
            >
              <Icon name="heart" size={16} />
              <span>{post.likes}</span>
            </Link>
          )}
        </footer>
        {error ? (
          <p role="alert" {...styled("post-error", sharedStyles.postError)}>
            {error}
          </p>
        ) : null}
      </div>
    </article>
  );
}
