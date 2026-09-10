"use client";
// @flow

import * as React from "@uniflowed/react";
import {
  startTransition,
  useActionState,
  useMemo,
  useOptimistic,
  useState,
} from "@uniflowed/react";
import { useFormStatus } from "react-dom";
import { props, stylex } from "@uniflowed/stylex";

import { createPost, likePost } from "./social-actions.js";
import {
  MAX_POST_LENGTH,
  TOPICS,
  optimisticPost,
  topicAccent,
  topicLabel,
  visiblePosts,
  type FormState,
  type Post,
  type Topic,
  type User,
} from "./social-model.js";

const EMPTY_POST_STATE: FormState<Post> = { status: "idle", message: "" };

component SubmitPost() renders React.Node {
  const { pending } = useFormStatus();
  return (
    <button type="submit" disabled={pending} {...props(styles.primaryButton)}>
      {pending ? "Posting" : "Post"}
    </button>
  );
}

component LikeButton(post: Post) renders React.Node {
  const [optimistic, addLike] = useOptimistic<
    {| readonly likes: number, readonly liked: boolean |},
    void,
  >({ likes: post.likes, liked: post.liked }, (current) => ({
    likes: current.likes + 1,
    liked: true,
  }));

  return (
    <button
      type="button"
      aria-pressed={optimistic.liked}
      onClick={() => {
        startTransition(() => {
          addLike();
          void likePost(post.id);
        });
      }}
      {...props(styles.iconButton, optimistic.liked && styles.iconButtonActive)}
    >
      {optimistic.liked ? "Liked" : "Like"} · {optimistic.likes}
    </button>
  );
}

component PostCard(post: Post) renders React.Node {
  return (
    <article {...props(styles.postCard)} style={{ borderLeftColor: topicAccent(post.topic) }}>
      <header {...props(styles.postHeader)}>
        <div {...props(styles.author)}>
          <span {...props(styles.avatar)}>{post.author.avatar}</span>
          <span>
            <strong {...props(styles.name)}>{post.author.name}</strong>
            <span {...props(styles.handle)}>@{post.author.handle}</span>
          </span>
        </div>
        <span {...props(styles.time)}>{post.createdAt.slice(0, 16).replace("T", " ")}</span>
      </header>
      <p {...props(styles.body)}>{post.body}</p>
      <footer {...props(styles.postFooter)}>
        <span {...props(styles.topic)}>{topicLabel(post.topic)}</span>
        <span {...props(styles.metric)}>{post.replies} replies</span>
        <LikeButton post={post} />
      </footer>
    </article>
  );
}

component Composer(
  viewer: User,
  onOptimistic: (Post) => void,
  onCommitted: (Post) => void,
) renders React.Node {
  const [state, action] = useActionState<FormState<Post>, FormData>(createPost, EMPTY_POST_STATE);

  React.useEffect(() => {
    if (state.status !== "success" || state.value == null) {
      return;
    }
    onCommitted(state.value);
  }, [state, onCommitted]);

  return (
    <form
      suppressHydrationWarning
      action={(formData) => {
        const body = String(formData.get("body") ?? "");
        const topic = (String(formData.get("topic") ?? "community"): any);
        if (body.trim().length > 0) {
          onOptimistic(optimisticPost(body, topic, viewer, new Date()));
        }
        action(formData);
      }}
      {...props(styles.composer)}
    >
      <div {...props(styles.composerTop)}>
        <span {...props(styles.avatar)}>{viewer.avatar}</span>
        <textarea
          name="body"
          aria-label="Post body"
          maxLength={MAX_POST_LENGTH}
          placeholder="Share a build note"
          rows={4}
          required
          {...props(styles.textarea)}
        />
      </div>
      <div {...props(styles.toolbar)}>
        <select
          name="topic"
          aria-label="Post topic"
          defaultValue="community"
          {...props(styles.select)}
        >
          {TOPICS.map((topic) => (
            <option key={topic} value={topic}>
              {topicLabel(topic)}
            </option>
          ))}
        </select>
        <span {...props(styles.formMessage, state.status === "error" && styles.error)}>
          {state.message}
        </span>
        <SubmitPost />
      </div>
    </form>
  );
}

export component TimelineClient(
  initialPosts: $ReadOnlyArray<Post>,
  viewer: User,
) renders React.Node {
  const [topic, setTopic] = useState<Topic | "all">("all");
  const [query, setQuery] = useState<string>("");
  const [committedPosts, setCommittedPosts] = useState<Array<Post>>(() => Array.from(initialPosts));
  const [posts, addOptimisticPost] = useOptimistic<Array<Post>, Post>(
    committedPosts,
    (current, draft) => [draft, ...current.filter((post) => post.id !== draft.id)],
  );
  const commitPost = React.useCallback(
    (post: Post) => {
      setCommittedPosts((current) =>
        current.some((item) => item.id === post.id) ? current : [post, ...current],
      );
    },
    [setCommittedPosts],
  );
  const filtered = useMemo(() => visiblePosts(posts, topic, query), [posts, topic, query]);

  return (
    <div {...props(styles.stack)}>
      <Composer viewer={viewer} onOptimistic={addOptimisticPost} onCommitted={commitPost} />
      <section {...props(styles.filters)} aria-label="Timeline filters">
        <select
          aria-label="Filter by topic"
          value={topic}
          onChange={(event) => setTopic((event.currentTarget.value: any))}
          {...props(styles.select)}
        >
          <option value="all">All topics</option>
          {TOPICS.map((item) => (
            <option key={item} value={item}>
              {topicLabel(item)}
            </option>
          ))}
        </select>
        <input
          aria-label="Search timeline"
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
          placeholder="Search people or posts"
          {...props(styles.input)}
        />
      </section>
      <section {...props(styles.posts)} aria-label="Timeline posts">
        {filtered.map((post) => (
          <PostCard key={post.id} post={post} />
        ))}
      </section>
    </div>
  );
}

const styles = stylex.create({
  stack: {
    display: "grid",
    gap: 14,
  },
  composer: {
    backgroundColor: "#ffffff",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    boxShadow: "0 16px 38px rgba(15, 23, 42, 0.08)",
    display: "grid",
    gap: 12,
    padding: {
      default: 14,
      "@media (min-width: 760px)": 16,
    },
  },
  composerTop: {
    alignItems: "start",
    display: "grid",
    gap: 12,
    gridTemplateColumns: "40px minmax(0, 1fr)",
  },
  avatar: {
    alignItems: "center",
    backgroundColor: "#0f766e",
    borderRadius: 8,
    color: "#ffffff",
    display: "inline-flex",
    fontSize: 13,
    fontWeight: 800,
    height: 40,
    justifyContent: "center",
    width: 40,
  },
  textarea: {
    backgroundColor: "#f8fafc",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: { default: "#0f172a", "::placeholder": "#475467" },
    font: "inherit",
    minHeight: 108,
    paddingBlock: 12,
    paddingInline: 12,
    resize: "vertical",
  },
  toolbar: {
    alignItems: "center",
    display: "grid",
    gap: 12,
    gridTemplateColumns: {
      default: "1fr",
      "@media (min-width: 720px)": "180px minmax(0, 1fr) auto",
    },
  },
  select: {
    backgroundColor: "#ffffff",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: "#1f2937",
    font: "inherit",
    minHeight: 44,
    paddingInline: 12,
  },
  input: {
    backgroundColor: "#ffffff",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: "#1f2937",
    font: "inherit",
    minHeight: 44,
    paddingInline: 14,
  },
  formMessage: {
    color: "#475467",
    fontSize: 14,
    minHeight: 20,
  },
  error: {
    color: "#b42318",
  },
  primaryButton: {
    backgroundColor: "#111827",
    borderColor: "#111827",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: "#ffffff",
    cursor: "pointer",
    font: "inherit",
    fontWeight: 800,
    minHeight: 44,
    paddingInline: 18,
  },
  filters: {
    backgroundColor: "rgba(255, 255, 255, 0.74)",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    boxShadow: "0 10px 28px rgba(15, 23, 42, 0.05)",
    display: "grid",
    gap: 12,
    gridTemplateColumns: {
      default: "1fr",
      "@media (min-width: 720px)": "180px minmax(0, 1fr)",
    },
    padding: 10,
  },
  posts: {
    display: "grid",
    gap: 12,
  },
  postCard: {
    backgroundColor: "#ffffff",
    borderColor: "#d8e0ea",
    borderLeftStyle: "solid",
    borderLeftWidth: 5,
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    display: "grid",
    gap: 12,
    padding: {
      default: 14,
      "@media (min-width: 760px)": 16,
    },
  },
  postHeader: {
    alignItems: "center",
    display: "flex",
    gap: 12,
    justifyContent: "space-between",
  },
  author: {
    alignItems: "center",
    display: "flex",
    gap: 10,
    minWidth: 0,
  },
  name: {
    color: "#111827",
    display: "block",
    fontSize: 15,
  },
  handle: {
    color: "#475467",
    display: "block",
    fontSize: 13,
  },
  time: {
    color: "#475467",
    fontSize: 12,
    whiteSpace: "nowrap",
  },
  body: {
    color: "#1f2937",
    fontSize: 16,
    lineHeight: 1.55,
    marginBlock: 0,
  },
  postFooter: {
    alignItems: "center",
    display: "flex",
    flexWrap: "wrap",
    gap: 10,
  },
  topic: {
    backgroundColor: "#eef6ff",
    borderRadius: 8,
    color: "#1d4ed8",
    fontSize: 12,
    fontWeight: 800,
    paddingBlock: 5,
    paddingInline: 8,
  },
  metric: {
    color: "#475467",
    fontSize: 13,
  },
  iconButton: {
    backgroundColor: "#ffffff",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: "#344054",
    cursor: "pointer",
    font: "inherit",
    fontSize: 13,
    minHeight: 34,
    paddingInline: 10,
  },
  iconButtonActive: {
    backgroundColor: "#ecfdf3",
    borderColor: "#a6f4c5",
    color: "#027a48",
  },
});
