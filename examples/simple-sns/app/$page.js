// @flow
import * as React from "@uniflowed/react";
import { Suspense, use } from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";

import { SocialFrame } from "./social-frame.js";
import { timelineData } from "./social-queries.js";
import { TimelineClient } from "./timeline-client.js";
import { type FeedStats, type Post, type User } from "./social-model.js";

type TimelineData = {|
  readonly posts: Array<Post>,
  readonly stats: FeedStats,
  readonly viewer: User,
|};

type HomeData = {|
  readonly timeline: Promise<TimelineData>,
|};

export function loader(): HomeData {
  return { timeline: timelineData() };
}

component Feed(data: Promise<TimelineData>) {
  const value = use(data);
  return <TimelineClient initialPosts={value.posts} viewer={value.viewer} />;
}

component Stats(data: Promise<TimelineData>) {
  const value = use(data).stats;
  return (
    <section {...props(styles.stats)} aria-label="Timeline summary">
      <h2 {...props(styles.asideTitle)}>Today</h2>
      <dl {...props(styles.statGrid)}>
        <div {...props(styles.stat)}>
          <dt {...props(styles.statLabel)}>Posts</dt>
          <dd {...props(styles.statValue)}>{value.posts}</dd>
        </div>
        <div {...props(styles.stat)}>
          <dt {...props(styles.statLabel)}>Authors</dt>
          <dd {...props(styles.statValue)}>{value.authors}</dd>
        </div>
        <div {...props(styles.stat)}>
          <dt {...props(styles.statLabel)}>Likes</dt>
          <dd {...props(styles.statValue)}>{value.likes}</dd>
        </div>
        <div {...props(styles.stat)}>
          <dt {...props(styles.statLabel)}>Replies</dt>
          <dd {...props(styles.statValue)}>{value.replies}</dd>
        </div>
      </dl>
    </section>
  );
}

component Skeleton() {
  return (
    <div {...props(styles.skeleton)}>
      <span />
      <span />
      <span />
    </div>
  );
}

export default component Home(data: HomeData) {
  return (
    <SocialFrame
      active="timeline"
      aside={
        <Suspense fallback={<Skeleton />}>
          <Stats data={data.timeline} />
        </Suspense>
      }
    >
      <Suspense fallback={<Skeleton />}>
        <Feed data={data.timeline} />
      </Suspense>
    </SocialFrame>
  );
}

const styles = stylex.create({
  stats: {
    backgroundColor: "#ffffff",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    boxShadow: "0 16px 38px rgba(15, 23, 42, 0.08)",
    display: "grid",
    gap: 14,
    padding: 16,
  },
  asideTitle: {
    color: "#111827",
    fontSize: 18,
    lineHeight: 1.2,
    marginBlock: 0,
  },
  statGrid: {
    display: "grid",
    gap: 10,
    marginBlock: 0,
  },
  stat: {
    alignItems: "center",
    borderBottomColor: "#eaecf0",
    borderBottomStyle: "solid",
    borderBottomWidth: 1,
    display: "flex",
    justifyContent: "space-between",
    paddingBottom: 10,
  },
  statLabel: {
    color: "#475467",
    fontWeight: 700,
  },
  statValue: {
    color: "#111827",
    fontWeight: 800,
    marginBlock: 0,
  },
  skeleton: {
    backgroundColor: "#ffffff",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    display: "grid",
    gap: 12,
    padding: 16,
  },
});
