"use client";
// @flow
import { styled, styles as sharedStyles } from "./commonplace.stylex.js";
import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import {
  graphql,
  useFragment,
  useMutation,
  useRelayEnvironment,
  fetchQuery,
} from "@uniflowed/relay";
import { appreciate, createPost, screenQuery } from "./operations.js";
import { Avatar, Icon, ActionLink, EmptyState } from "./ui.js";
import { SearchNotes } from "./search.client.js";
import {
  displayDate,
  topicFrom,
  topicLabel,
  TOPICS,
  feedHref,
  type FeedFilter,
} from "./social-model.js";
import type { SnsScreenQuery$data } from "./__generated__/SnsScreenQuery.graphql.js";
import type { SnsPost_post$key } from "./__generated__/SnsPost_post.graphql.js";
import type { SnsAppreciateMutation } from "./__generated__/SnsAppreciateMutation.graphql.js";
import type { SnsCreatePostMutation } from "./__generated__/SnsCreatePostMutation.graphql.js";

const postFragment = graphql`
  fragment SnsPost_post on Post {
    id
    body
    topic
    likes
    liked
    createdAt
    author {
      id
      name
      handle
      avatar
      photo
      bio
    }
  }
`;

/** Relay owns the optimistic layer and rolls it back when a request fails. */
component PostCard(postRef: SnsPost_post$key, signedIn: boolean) {
  const post = useFragment(postFragment, postRef);
  const [commit, pending] = useMutation<
    SnsAppreciateMutation["variables"],
    SnsAppreciateMutation["response"],
  >(appreciate);
  const [error, setError] = useState("");
  const topic = topicFrom(post.topic) ?? "community";
  return (
    <article className="post" aria-busy={pending}>
      <Avatar user={post.author} />
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

component Composer(viewer: NonNullable<SnsScreenQuery$data["viewer"]>, filter: FeedFilter) {
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
              screenQuery,
              {
                topic: filter.topic,
                search: filter.query,
                page: filter.page,
                thread: "thread-mika",
                feed: true,
                messages: false,
                settings: false,
              },
              { fetchPolicy: "network-only" },
            ).subscribe({ error: () => setError("Published. Refresh the feed to see your note.") });
          },
        });
      }}
    >
      <div className="composer-body">
        <Avatar user={viewer} />
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
        <button className="button primary" disabled={pending}>
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

/** The same Commonplace feed; data and fragment ownership belong to Relay. */
export component Timeline(data: SnsScreenQuery$data, filter: FeedFilter) {
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
          <Composer viewer={data.viewer} filter={filter} />
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
