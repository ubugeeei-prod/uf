"use client";
// @flow

import * as React from "@uniflowed/react";
import { callAction } from "./action-result.client.js";
import { Link } from "@uniflowed/router";
import {
  Activity,
  ViewTransition,
  startTransition,
  useActionState,
  useOptimistic,
  useState,
} from "@uniflowed/react";
import { CollapsibleRoot, CollapsibleTrigger, CollapsibleContent } from "@uniflowed/ui/collapsible";
import { AsyncRegion, useRetryableResource } from "./async-region.client.js";
import { timelineData } from "./social-queries.js";
import { createPost, likePost } from "./social-actions.js";
import { Avatar, ActionLink, EmptyState, Icon, LoadingState } from "./ui.js";
import { FieldError, FormStatus, SubmitButton } from "./form-ui.client.js";
import {
  MAX_POST_LENGTH,
  TOPICS,
  IDLE,
  fieldError,
  displayDate,
  topicLabel,
  type FormState,
  type Post,
  type User,
  type Session,
  type FeedData,
  type FeedFilter,
  feedHref,
} from "./social-model.js";

/** Optimistically set a reaction and restore the committed value if its action fails. */
component Appreciation(post: Post, signedIn: boolean) {
  const [current, setCurrent] = useState<Post>(post);
  const [optimistic, changeOptimistic] = useOptimistic<Post, boolean>(current, (value, liked) => ({
    ...value,
    liked,
    likes: value.likes + (liked === value.liked ? 0 : liked ? 1 : -1),
  }));
  const [state, submit, pending] = useActionState<FormState<Post>, boolean>(
    async (_previous: FormState<Post>, liked: boolean): Promise<FormState<Post>> => {
      changeOptimistic(liked);
      const result = await callAction(() => likePost(post.id, liked), "Could not save. Try again.");
      match (result) {
        {status: "success", value: const value, ...} => {
          setCurrent(value);
        }
        {status: "error", ...} => {}
      }
      return result;
    },
    IDLE,
  );

  return (
    <div>
      {signedIn ? (
        <button
          type="button"
          className="reaction"
          aria-label={`${optimistic.liked ? "Remove appreciation" : "Appreciate"} · ${optimistic.likes}`}
          aria-pressed={optimistic.liked}
          disabled={pending}
          onClick={() => startTransition(() => submit(!optimistic.liked))}
        >
          <Icon name="heart" size={16} />
          <span>{optimistic.likes}</span>
        </button>
      ) : (
        <Link className="reaction" to="/login" aria-label={`Sign in to appreciate · ${post.likes}`}>
          <Icon name="heart" size={16} />
          <span>{post.likes}</span>
        </Link>
      )}
      {
        match (state) {
          {status: "error", message: const message, ...} =>
            <p role="alert" className="post-error">
              {message}
            </p>,
          {status: "idle"} | {status: "success", ...} => null,
        }
      }
    </div>
  );
}

/** Render one note with its author, timestamp, and viewer-specific reaction control. */
export component PostCard(post: Post, signedIn: boolean) {
  const pending = post.id.startsWith("pending-");

  return (
    <ViewTransition name={`note-${post.id}`} enter="feed-item" exit="feed-item">
      <article className={`post ${pending ? "optimistic" : ""}`} aria-busy={pending}>
        <Avatar user={post.author} />
        <div className="post-content">
          <header className="post-header">
            <strong>{post.author.name}</strong>
            <span className="handle">@{post.author.handle}</span>
            <time dateTime={post.createdAt}>
              {pending ? "Publishing…" : displayDate(post.createdAt)}
            </time>
          </header>
          <p className="post-body">{post.body}</p>
          <footer className="post-footer">
            <Link className="channel-badge" to={`/?topic=${post.topic}`}>
              <span className={`channel-dot ${post.topic}`} />
              {topicLabel(post.topic)}
            </Link>
            {pending ? (
              <span className="counter">Publishing…</span>
            ) : (
              <Appreciation post={post} signedIn={signedIn} />
            )}
          </footer>
        </div>
      </article>
    </ViewTransition>
  );
}
// The list accepts cards, including fragments and arrays of cards, not arbitrary markup.

/** Accept only rendered PostCard children so feed composition stays structurally typed. */
export component PostList(children: renders* PostCard) {
  return <section aria-label="Timeline posts">{children}</section>;
}

type LocalPost = {| readonly requestId: string, readonly post: Post |};

/**
 * Keep the draft and submission ID through failures, clearing them only after a committed post.
 */
component PostComposer(
  viewer: User,
  onOptimistic: (LocalPost) => void,
  onPublished: (LocalPost) => void,
) {
  const [body, setBody] = useState("");
  const [topic, setTopic] = useState("community");
  const [requestId, setRequestId] = useState("");
  const [state, submit, pending] = useActionState<FormState<Post>, FormData>(
    async (_previous: FormState<Post>, form: FormData): Promise<FormState<Post>> => {
      const text = String(form.get("body") ?? "").trim();
      const submissionId = requestId || crypto.randomUUID();
      setRequestId(submissionId);
      const submitted = new FormData();
      submitted.set("body", text);
      submitted.set("topic", topic);
      submitted.set("requestId", submissionId);
      if (text.length > 0 && text.length <= MAX_POST_LENGTH)
        onOptimistic({
          requestId: submissionId,
          post: {
            id: `pending-${submissionId}`,
            author: viewer,
            body: text,
            topic: TOPICS.find((value) => value === topic) ?? "community",
            likes: 0,
            liked: false,
            createdAt: new Date().toISOString(),
          },
        });
      const result = await callAction(
        () => createPost(IDLE, submitted),
        "You seem to be offline. Your draft is still here; try again.",
      );
      match (result) {
        {status: "success", value: const saved, ...} => {
          onPublished({ requestId: submissionId, post: saved });
          setBody("");
          setRequestId("");
        }
        {status: "error", ...} => {}
      }
      return result;
    },
    IDLE,
  );

  return (
    <form className="composer" action={submit} aria-label="Publish a note">
      <div className="composer-body">
        <Avatar user={viewer} />
        <textarea
          name="body"
          aria-label="Post body"
          aria-describedby="body-error"
          aria-invalid={fieldError(state, "body") != null}
          value={body}
          onChange={(event) => {
            setBody(event.currentTarget.value);
            setRequestId((current) => current || crypto.randomUUID());
          }}
          placeholder={`What are you working on, ${viewer.name.split(" ")[0]}?`}
          maxLength={MAX_POST_LENGTH}
          required
          disabled={pending}
          rows={3}
        />
      </div>
      <div className="composer-footer">
        <select
          name="topic"
          aria-label="Post channel"
          onChange={(event) => {
            setTopic(event.currentTarget.value);
            setRequestId((current) => current || crypto.randomUUID());
          }}
          value={topic}
          disabled={pending}
        >
          {TOPICS.map((value) => (
            <option key={value} value={value}>
              {topicLabel(value)}
            </option>
          ))}
        </select>
        <span className="counter">
          {body.length} / {MAX_POST_LENGTH}
        </span>
        <SubmitButton pendingLabel="Publishing…" disabled={body.trim().length === 0}>
          Publish note
        </SubmitButton>
      </div>
      <FieldError state={state} name="body" />
      <FormStatus state={state} />
    </form>
  );
}

/**
 * Keep composition outside feed loading and retry boundaries.
 * Activity preserves a hidden draft; local submission records reconcile optimistic and committed notes once.
 */
export component TimelineClient(initial: Promise<FeedData>, filter: FeedFilter, session: Session) {
  const { resource, retry } = useRetryableResource(initial, () =>
    timelineData(filter.topic, filter.query, String(filter.page)),
  );
  const showComposer = filter.topic === "all" && filter.query === "" && filter.page === 1;
  const [committed, setCommitted] = useState<Array<LocalPost>>([]);
  const [posts, addOptimistic] = useOptimistic<Array<LocalPost>, LocalPost>(
    committed,
    (current, draft) =>
      current.some((entry) => entry.requestId === draft.requestId) ? current : [draft, ...current],
  );
  const [composerOpen, setComposerOpen] = useState(true);

  function published(saved: LocalPost): void {
    setCommitted((current) => [
      saved,
      ...current.filter((entry) => entry.post.id !== saved.post.id),
    ]);
  }

  return (
    <>
      {showComposer
        ? match (session) {
            {kind: "authenticated", user: const user} =>
              <div id="compose" className="compose-region">
                <CollapsibleRoot
                  open={composerOpen}
                  onOpenChange={(open) => startTransition(() => setComposerOpen(open))}
                >
                  <CollapsibleTrigger className="compose-toggle">
                    <Icon name="compose" size={16} />
                    Write a note
                    <Icon name={composerOpen ? "chevron-up" : "chevron-down"} size={15} />
                  </CollapsibleTrigger>
                  <ViewTransition name="note-composer" enter="composer-panel" exit="composer-panel">
                    <Activity mode={composerOpen ? "visible" : "hidden"}>
                      <CollapsibleContent>
                        <PostComposer
                          viewer={user}
                          onOptimistic={addOptimistic}
                          onPublished={published}
                        />
                      </CollapsibleContent>
                    </Activity>
                  </ViewTransition>
                </CollapsibleRoot>
              </div>,
            {kind: "guest"} =>
              <section className="sign-in-composer">
                <div>
                  <h2>What are you working on?</h2>
                  <p>Sign in to post an update or ask a question.</p>
                </div>
                <ActionLink to="/signup">Create account</ActionLink>
              </section>,
          }
        : null}
      <AsyncRegion
        resource={resource}
        retry={retry}
        label="notes"
        pending={<LoadingState kind="feed" />}
      >
        {(feed) => (
          <FeedEntries
            data={feed}
            additions={posts.map((entry) => entry.post)}
            signedIn={session.kind === "authenticated"}
          />
        )}
      </AsyncRegion>
    </>
  );
}

/**
 * Merge local posts with the resolved page by server ID and render the feed or its empty state.
 */
component FeedEntries(data: FeedData, additions: $ReadOnlyArray<Post>, signedIn: boolean) {
  const feed = data;
  const posts = [
    ...additions,
    ...feed.posts.filter((post) => !additions.some((saved) => saved.id === post.id)),
  ];

  return (
    <>
      {posts.length === 0 ? (
        <EmptyState
          title="No notes found"
          action={
            <ActionLink to="/" primary={false}>
              Back to all notes
            </ActionLink>
          }
        >
          Try a different channel or a shorter search.
        </EmptyState>
      ) : (
        <PostList>
          {posts.map((post) => (
            <PostCard key={post.id} post={post} signedIn={signedIn} />
          ))}
        </PostList>
      )}
      <nav className="pagination" aria-label="Feed pagination">
        {feed.page > 1 ? (
          <Link to={feedHref(feed.topic, feed.query, feed.page - 1)}>← Newer notes</Link>
        ) : (
          <span>Latest notes</span>
        )}
        {feed.hasNext ? (
          <Link to={feedHref(feed.topic, feed.query, feed.page + 1)}>
            Older notes
            <Icon name="arrow" size={14} />
          </Link>
        ) : null}
      </nav>
    </>
  );
}
