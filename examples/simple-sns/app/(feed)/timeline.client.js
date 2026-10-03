"use client";
// @flow

import * as React from "@uniflowed/react";

import { callAction } from "../_shared/action-result.client.js";

import { Link } from "@uniflowed/router";
import { props, stylex } from "@uniflowed/stylex";
import {
  Activity,
  ViewTransition,
  startTransition,
  useActionState,
  useOptimistic,
  useState,
} from "@uniflowed/react";
import { Collapsible } from "@uniflowed/ui";

import { AsyncRegion, useRetryableResource } from "../_shared/async-region.client.js";
import { timelineData } from "../_server/social-queries.js";
import { createPost, likePost } from "../_server/social-actions.js";
import { channelDot } from "../_shared/social-frame.js";
import {
  Avatar,
  ActionLink,
  EmptyState,
  Icon,
  LoadingState,
  styles as uiStyles,
} from "../_shared/ui.js";
import { FieldError, FormStatus, SubmitButton } from "../_shared/form-ui.client.js";
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
} from "../_shared/social-model.js";

/** Optimistically set a reaction and restore the committed value if its action fails. */

component Appreciation(post: Post, signedIn: boolean) {
  const [current,    setCurrent]       = useState<Post>(post);
  const [optimistic, changeOptimistic] = useOptimistic<Post, boolean>(current, (value, liked) => ({
    ...value,
    liked,
    likes: value.likes + (liked === value.liked ? 0 : liked ? 1 : -1),
  }));
  const [state, submit, pending]       = useActionState<FormState<Post>, boolean>(
    async (_previous: FormState<Post>, liked: boolean): Promise<FormState<Post>> => {
      changeOptimistic(liked);
      const result = await callAction(() => likePost(post.id, liked), "Could not save. Try again.");
      match (result) {
        {status: "success", value: const value, ...} => {
          setCurrent(value);
        }
        {status: "error", ...}                       => {}
      }
      return result;
    },
    IDLE,
  );

  return (
    <div>
      {
        match (signedIn) {
          true  =>
            <button
              type="button"
              {...props(styles.reaction)}
              aria-label={`${match (optimistic.liked) {
                true  => "Remove appreciation",
                false => "Appreciate",
              }} · ${optimistic.likes}`}
              aria-pressed={optimistic.liked}
              disabled={pending}
              onClick={() => startTransition(() => submit(!optimistic.liked))}
            >
              <Icon name="heart" size={16} {...props(optimistic.liked && styles.reactionIcon)} />
              <span>{optimistic.likes}</span>
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
      {
        match (state) {
          {status: "error", message: const message, ...} =>
            <p role="alert" {...props(styles.postError)}>
              {message}
            </p>,
          {status: "idle"} | {status: "success", ...}    => null,
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
      <article {...props(uiStyles.post, pending && styles.optimistic)} aria-busy={pending}>
        <Avatar user={post.author} />
        <div {...props(uiStyles.postContent)}>
          <header {...props(uiStyles.postHeader)}>
            <strong {...props(styles.postName)}>{post.author.name}</strong>
            <span {...props(styles.handle)}>@{post.author.handle}</span>
            <time {...props(styles.postTime)} dateTime={post.createdAt}>
              {
                match (pending) {
                  true  => "Publishing…",
                  false => displayDate(post.createdAt),
                }
              }
            </time>
          </header>
          <p {...props(uiStyles.postBody)}>{post.body}</p>
          <footer {...props(uiStyles.postFooter)}>
            <Link {...props(styles.channelBadge)} to={`/?topic=${post.topic}`}>
              <span {...props(channelDot, styles.badgeDot)} />
              {topicLabel(post.topic)}
            </Link>
            {
              match (pending) {
                true  => <span {...props(styles.counter)}>Publishing…</span>,
                false => <Appreciation post={post} signedIn={signedIn} />,
              }
            }
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
  viewer      : User,
  onOptimistic: (LocalPost) => void,
  onPublished : (LocalPost) => void,
) {
  const [body,      setBody]      = useState("");
  const [topic,     setTopic]     = useState("community");
  const [requestId, setRequestId] = useState("");
  const [state, submit, pending]  = useActionState<FormState<Post>, FormData>(
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
            id       : `pending-${submissionId}`,
            author   : viewer,
            body     : text,
            topic    : TOPICS.find((value) => value === topic) ?? "community",
            likes    : 0,
            liked    : false,
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
        {status: "error", ...}                       => {}
      }
      return result;
    },
    IDLE,
  );

  return (
    <form {...props(styles.composer)} action={submit} aria-label="Publish a note">
      <div {...props(styles.composerBody)}>
        <Avatar user={viewer} />
        <textarea
          {...props(styles.composerText)}
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
      <div {...props(styles.composerFooter)}>
        <select
          {...props(styles.composerTopic)}
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
        <span {...props(styles.counter, styles.composerCounter)}>
          {body.length} / {MAX_POST_LENGTH}
        </span>
        <SubmitButton
          pendingLabel="Publishing…"
          disabled={body.trim().length === 0}
          xstyle={styles.composerSubmit}
        >
          Publish note
        </SubmitButton>
      </div>
      <FieldError state={state} name="body" xstyle={styles.composerPad} />
      <FormStatus state={state} xstyle={styles.composerPad} />
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
  const [committed,    setCommitted]    = useState<Array<LocalPost>>([]);
  const [posts,        addOptimistic]   = useOptimistic<Array<LocalPost>, LocalPost>(
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
      {
        match (showComposer) {
          false => null,
          true  =>
            match (session) {
              {kind: "authenticated", user: const user} =>
                <div id="compose" {...props(styles.composeRegion)}>
                  <Collapsible.Root
                    open={composerOpen}
                    onOpenChange={(open) => startTransition(() => setComposerOpen(open))}
                  >
                    <Collapsible.Trigger {...props(styles.composeToggle)}>
                      <Icon name="compose" size={16} />
                      Write a note
                      <Icon
                        {...props(styles.composeChevron)}
                        name={
                          match (composerOpen) {
                            true  => "chevron-up",
                            false => "chevron-down",
                          }
                        }
                        size={15}
                      />
                    </Collapsible.Trigger>
                    <ViewTransition
                      name="note-composer"
                      enter="composer-panel"
                      exit="composer-panel"
                    >
                      <Activity
                        mode={
                          match (composerOpen) {
                            true  => "visible",
                            false => "hidden",
                          }
                        }
                      >
                        <Collapsible.Content>
                          <PostComposer
                            viewer={user}
                            onOptimistic={addOptimistic}
                            onPublished={published}
                          />
                        </Collapsible.Content>
                      </Activity>
                    </ViewTransition>
                  </Collapsible.Root>
                </div>,
              {kind: "guest"}                           =>
                <section {...props(styles.signInComposer)}>
                  <div>
                    <h2 {...props(styles.signInTitle)}>What are you working on?</h2>
                    <p {...props(styles.signInCopy)}>
                      Sign in to post an update or ask a question.
                    </p>
                  </div>
                  <ActionLink
                    to="/signup"
                    xstyle={styles.signInButton}
                    iconStyle={styles.signInIcon}
                  >
                    Create account
                  </ActionLink>
                </section>,
            },
        }
      }
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
      {
        match (posts.length === 0) {
          true  =>
            <EmptyState
              title="No notes found"
              action={
                <ActionLink to="/" primary={false}>
                  Back to all notes
                </ActionLink>
              }
            >
              Try a different channel or a shorter search.
            </EmptyState>,
          false =>
            <PostList>
              {posts.map((post) => (
                <PostCard key={post.id} post={post} signedIn={signedIn} />
              ))}
            </PostList>,
        }
      }
      <nav {...props(styles.pagination)} aria-label="Feed pagination">
        {
          match (feed.page > 1) {
            true  =>
              <Link
                {...props(styles.paginationLink)}
                to={feedHref(feed.topic, feed.query, feed.page - 1)}
              >
                ← Newer notes
              </Link>,
            false => <span>Latest notes</span>,
          }
        }
        {
          match (feed.hasNext) {
            true  =>
              <Link
                {...props(styles.paginationLink)}
                to={feedHref(feed.topic, feed.query, feed.page + 1)}
              >
                Older notes
                <Icon name="arrow" size={14} />
              </Link>,
            false => null,
          }
        }
      </nav>
    </>
  );
}

const styles = stylex.create({
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
    ":hover": {
      color: "#252525",
    },
    ":is([aria-pressed=true])": {
      color: "#252525",
    },
  },
  reactionIcon: {
    fill: "currentColor",
  },
  postError: {
    fontSize : "11px",
    color    : "#b13749",
    marginTop: "8px",
  },
  optimistic: {
    opacity: "0.6",
  },
  postName: {
    fontSize  : { default: "13px", "@media (max-width: 760px)": "12px" },
    fontWeight: "600",
  },
  handle: {
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
  badgeDot: {
    width : "4px",
    height: "4px",
  },
  counter: {
    fontVariantNumeric: "tabular-nums",
    fontSize          : "11px",
    color             : "var(--muted)",
  },
  composerCounter: {
    display: { "@media (max-width: 760px)": "none" },
  },
  composer: {
    overflow       : "hidden",
    scrollMarginTop: "20px",
    background     : "transparent",
    border         : "0",
    borderBottom   : "1px solid var(--line)",
    borderRadius   : "0",
    marginBottom   : "9px",
  },
  composerBody: {
    display      : "flex",
    gap          : { default: "14px", "@media (max-width: 760px)": "11px" },
    paddingTop   : { default: "16px", "@media (max-width: 760px)": "17px" },
    paddingRight : { default: "0", "@media (max-width: 760px)": "14px" },
    paddingBottom: { default: "10px", "@media (max-width: 760px)": "9px" },
    paddingLeft  : { default: "0", "@media (max-width: 760px)": "14px" },
  },
  composerText: {
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
  composerTopic: {
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
  composerSubmit: {
    minHeight    : "33px",
    fontSize     : "11px",
    paddingTop   : "7px",
    paddingRight : "12px",
    paddingBottom: "7px",
    paddingLeft  : "12px",
    marginLeft   : "auto",
  },
  composerPad: {
    paddingRight: "18px",
    paddingLeft : "18px",
  },
  composeRegion: {
    scrollMarginTop: "85px",
  },
  composeToggle: {
    display      : "flex",
    alignItems   : "center",
    gap          : "8px",
    width        : "100%",
    border       : "0",
    background   : "transparent",
    fontSize     : "12px",
    fontWeight   : "550",
    color        : "#686868",
    paddingTop   : "3px",
    paddingRight : "0",
    paddingBottom: "13px",
    paddingLeft  : "0",
  },
  composeChevron: {
    marginLeft: "auto",
  },
  signInComposer: {
    display       : { default: "flex", "@media (max-width: 760px)": "none" },
    alignItems    : "center",
    justifyContent: "space-between",
    gap           : { default: "18px", "@media (max-width: 760px)": "12px" },
    background    : "transparent",
    border        : "0",
    borderBottom  : "1px solid var(--line)",
    borderRadius  : "0",
    paddingTop    : { default: "12px", "@media (max-width: 760px)": "14px" },
    paddingRight  : { default: "0", "@media (max-width: 760px)": "12px" },
    paddingBottom : { default: "24px", "@media (max-width: 760px)": "14px" },
    paddingLeft   : { default: "0", "@media (max-width: 760px)": "12px" },
    marginBottom  : "5px",
  },
  signInTitle: {
    fontSize     : { default: "14px", "@media (max-width: 760px)": "12px" },
    fontWeight   : "600",
    letterSpacing: "-0.1px",
  },
  signInCopy: {
    fontSize  : { default: "11px", "@media (max-width: 760px)": "10px" },
    color     : "var(--muted)",
    lineHeight: "1.6",
    marginTop : "6px",
  },
  signInButton: {
    fontSize     : { default: "11px", "@media (max-width: 760px)": "10px" },
    minHeight    : "34px",
    paddingTop   : { default: "9px", "@media (max-width: 760px)": "8px" },
    paddingRight : { default: "15px", "@media (max-width: 760px)": "8px" },
    paddingBottom: { default: "9px", "@media (max-width: 760px)": "8px" },
    paddingLeft  : { default: "15px", "@media (max-width: 760px)": "8px" },
  },
  signInIcon: {
    display: { "@media (max-width: 760px)": "none" },
  },
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
