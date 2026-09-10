// @flow
//
// The examples are not release packages, but they are still code a reader is
// meant to copy. This pins the Simple SNS example's model and architecture so
// a visual tweak cannot quietly turn it back into a static client-only demo.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "@uniflowed/test";

import {
  MAX_MESSAGE_LENGTH,
  MAX_POST_LENGTH,
  TOPICS,
  messagePreview,
  optimisticMessage,
  optimisticPost,
  pageTitle,
  statsFor,
  topicLabel,
  visiblePosts,
} from "../../examples/simple-sns/app/social-model.js";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const SIMPLE_SNS = path.join(REPO, "examples/simple-sns");

describe("the Simple SNS example", () => {
  it("has the files uf needs to run it as an app", () => {
    for (const relative of [
      "package.json",
      "uf.config.js",
      "app.js",
      "app/$layout.js",
      "app/$page.js",
      "app/auth-client.js",
      "app/social-actions.js",
      "app/social-db.server.js",
      "app/social-frame.js",
      "app/social-model.js",
      "app/social-queries.js",
      "app/timeline-client.js",
      "app/messages/$page.js",
      "app/messages/direct-messages-client.js",
      "app/settings/$page.js",
      "app/settings/settings-client.js",
      "app/login/$page.js",
      "app/signup/$page.js",
    ]) {
      expect(fs.existsSync(path.join(SIMPLE_SNS, relative))).toBe(true);
    }
    expect(fs.existsSync(path.join(SIMPLE_SNS, "app/styles.css"))).toBe(false);
    expect(fs.existsSync(path.join(SIMPLE_SNS, "app/_components"))).toBe(false);
    expect(fs.existsSync(path.join(SIMPLE_SNS, "app/_actions"))).toBe(false);
    expect(fs.existsSync(path.join(SIMPLE_SNS, "app/_server"))).toBe(false);
  });

  it("is an SSR app whose React version and styling remain user owned", () => {
    const manifest = JSON.parse(fs.readFileSync(path.join(SIMPLE_SNS, "package.json"), "utf8"));
    expect(manifest.dependencies.react).toBe("^19.3.0");
    expect(manifest.dependencies["react-dom"]).toBe("^19.3.0");
    expect(manifest.dependencies["@uniflowed/stylex"]).toBe("0.0.0-alpha.18");

    const config = fs.readFileSync(path.join(SIMPLE_SNS, "uf.config.js"), "utf8");
    expect(config).toContain('modes: ["ssr"]');
    expect(config).toContain("staticBuild: false");
    expect(config).toContain("reactCompiler");
    expect(config).toContain('style: "style-x"');
  });

  it("uses RSC, Server Actions, Async React, StyleX, and Flow composition features", () => {
    const sources = [
      "app/$page.js",
      "app/$layout.js",
      "app/auth-client.js",
      "app/social-actions.js",
      "app/social-db.server.js",
      "app/social-frame.js",
      "app/social-model.js",
      "app/social-queries.js",
      "app/timeline-client.js",
      "app/messages/direct-messages-client.js",
      "app/settings/settings-client.js",
    ]
      .map((relative) => fs.readFileSync(path.join(SIMPLE_SNS, relative), "utf8"))
      .join("\n");

    for (const required of [
      '"use server"',
      "node:sqlite",
      "export function loader",
      "Suspense",
      "use(",
      "useActionState",
      "useFormStatus",
      "useOptimistic",
      "stylex.create",
      "renders*",
      "match (",
    ]) {
      expect(sources).toContain(required);
    }
    expect(sources).not.toContain("renders React.Node");
  });

  it("keeps client boundaries intentional and colocated", () => {
    const clientModules = filesUnder(path.join(SIMPLE_SNS, "app"))
      .filter((file) => fs.readFileSync(file, "utf8").startsWith('"use client";'))
      .map((file) => path.relative(SIMPLE_SNS, file).replaceAll(path.sep, "/"))
      .sort();

    expect(clientModules).toEqual([
      "app/auth-client.js",
      "app/messages/direct-messages-client.js",
      "app/settings/settings-client.js",
      "app/timeline-client.js",
    ]);
  });

  it("models each topic explicitly", () => {
    expect(TOPICS.map(topicLabel)).toEqual(["Release", "Runtime", "Design", "Community"]);
    expect(pageTitle("messages")).toBe("Direct messages");
  });

  it("filters by topic and free text together", () => {
    const posts = samplePosts();
    expect(visiblePosts(posts, "release", "").map((post) => post.id)).toEqual(["release"]);
    expect(visiblePosts(posts, "all", "runtime").map((post) => post.id)).toEqual(["runtime"]);
    expect(visiblePosts(posts, "design", "release")).toEqual([]);
  });

  it("summarizes feed activity without double-counting authors", () => {
    expect(statsFor(samplePosts())).toEqual({ posts: 3, authors: 2, likes: 9, replies: 6 });
  });

  it("creates bounded optimistic records for posts and messages", () => {
    const viewer = {
      id: "u-mika",
      name: "Mika Tan",
      handle: "mika",
      avatar: "MT",
      bio: "Builder",
    };
    const post = optimisticPost("  ".concat("hello ".repeat(80)), "release", viewer, new Date(0));
    const message = optimisticMessage("thread-ren", "  ".concat("yo ".repeat(160)), new Date(0));

    expect(post.body.length).toBe(MAX_POST_LENGTH);
    expect(post.author.handle).toBe("mika");
    expect(post.liked).toBe(true);
    expect(message.body.length).toBe(MAX_MESSAGE_LENGTH);
    expect(message.threadId).toBe("thread-ren");
    expect(
      messagePreview({ id: "t", name: "Ren", handle: "ren", unread: 2, lastMessage: "Ping" }),
    ).toBe("2 new - Ping");
  });
});

function samplePosts() {
  const mika = {
    id: "u-mika",
    name: "Mika Tan",
    handle: "mika",
    avatar: "MT",
    bio: "Builder",
  };
  const sora = {
    id: "u-sora",
    name: "Sora Lin",
    handle: "sora",
    avatar: "SL",
    bio: "Runtime",
  };
  return [
    {
      id: "release",
      author: mika,
      body: "Release train is ready",
      topic: "release",
      likes: 2,
      replies: 1,
      createdAt: "2026-09-10T00:00:00.000Z",
      liked: true,
    },
    {
      id: "runtime",
      author: sora,
      body: "Runtime graph changed",
      topic: "runtime",
      likes: 3,
      replies: 2,
      createdAt: "2026-09-10T01:00:00.000Z",
      liked: false,
    },
    {
      id: "design",
      author: mika,
      body: "Quiet settings surface",
      topic: "design",
      likes: 4,
      replies: 3,
      createdAt: "2026-09-10T02:00:00.000Z",
      liked: false,
    },
  ];
}

function filesUnder(root) {
  const found = [];
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const file = path.join(root, entry.name);
    if (entry.isDirectory()) {
      found.push(...filesUnder(file));
    } else {
      found.push(file);
    }
  }
  return found;
}
