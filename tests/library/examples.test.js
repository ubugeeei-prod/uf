// @flow
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "@uniflowed/test";
import { contextFor, runWithContext } from "../../packages/server/internal/context.js";
import {
  database,
  closeDatabase,
  transaction,
} from "../../examples/simple-sns/app/server/database.server.js";
import {
  authenticate,
  issueSession,
  revokeSession,
  viewerFor,
  SESSION_COOKIE,
} from "../../examples/simple-sns/app/server/session.server.js";
import {
  listThreads,
  listMessages,
  insertMessage,
  insertPost,
  settingsFor,
} from "../../examples/simple-sns/app/server/repository.server.js";
import {
  createPost,
  likePost,
  sendMessage,
  updateSettings,
} from "../../examples/simple-sns/app/social-actions.js";
import {
  sessionData,
  timelineData,
  messagesData,
  settingsData,
} from "../../examples/simple-sns/app/social-queries.js";
import { POST } from "../../examples/simple-sns/app/auth/session/$route.js";
import {
  IDLE,
  feedFilter,
  type User,
  type Post,
  type FormState,
} from "../../examples/simple-sns/app/social-model.js";
import { layerMerge, layerSucceed, provide, runPromiseExit } from "@uniflowed/effect";
import {
  IdentityService,
  SocialStore,
  publishNote,
  type MutationProblem,
  type Identity,
  type Store,
} from "../../examples/simple-sns/app/server/programs.server.js";

const originalDb = process.env.UF_SIMPLE_SNS_DB;
let directory = "";
beforeEach(() => {
  closeDatabase();
  directory = fs.mkdtempSync(path.join(os.tmpdir(), "commonplace-test-"));
  process.env.UF_SIMPLE_SNS_DB = path.join(directory, "test.sqlite");
});
afterEach(() => {
  closeDatabase();
  fs.rmSync(directory, { recursive: true, force: true });
  if (originalDb == null) delete process.env.UF_SIMPLE_SNS_DB;
  else process.env.UF_SIMPLE_SNS_DB = originalDb;
});
function form(values: { readonly [string]: string }): FormData {
  const data = new FormData();
  for (const [key, value] of Object.entries(values)) data.set(key, value);
  return data;
}
async function account(handle: string): Promise<{| user: User, cookie: string |}> {
  const user = await authenticate(
    form({
      handle,
      password: "a sample password 123",
      name: handle,
      email: `${handle}@example.test`,
    }),
    "signup",
  );
  return { user, cookie: issueSession(user, null, false).split(";")[0] };
}
function as<T>(cookie: string, action: () => T): T {
  return runWithContext(
    contextFor(new Request("http://localhost/", { headers: { cookie } })),
    action,
  );
}
function token(cookie: string): string {
  return cookie.slice(cookie.indexOf("=") + 1);
}
function request(values: { [string]: string }, headers: { [string]: string } = {}): Request {
  const requestHeaders = new Headers({
    origin: "http://localhost",
    "content-type": "application/x-www-form-urlencoded",
  });
  for (const [key, value] of Object.entries(headers)) requestHeaders.set(key, value);
  return new Request("http://localhost/auth/session", {
    method: "POST",
    headers: requestHeaders,
    body: new URLSearchParams(values).toString(),
  });
}

describe("Commonplace server contracts", () => {
  it("normalizes and bounds feed filters without interpreting search as SQL", async () => {
    expect(feedFilter("unknown", "  hello  ", "-8")).toEqual({
      topic: "all",
      query: "hello",
      page: 1,
    });
    expect(feedFilter("design", "x".repeat(200), "99999").query.length).toBe(100);
    expect((await as("", () => timelineData("all", "' OR 1=1 --"))).posts).toEqual([]);
  });
  it("never accepts the previous action state as an identity", async () => {
    const alice = await account("alice");
    const forged: FormState<Post> = {
      status: "success",
      value: { ...insertPost(alice.user, "Original", "design", "original") },
      message: "OK",
    };
    const denied = await as("", () =>
      createPost(forged, form({ body: "Forged", topic: "design", requestId: "forged" })),
    );
    expect(denied.status).toBe("error");
    expect((await as("", () => timelineData("all", "Forged"))).posts).toEqual([]);
  });
  it("stores only password and session hashes, rotates tokens, and expires sessions", async () => {
    const alice = await account("alice");
    const row = database()
      .prepare("SELECT password_hash FROM members WHERE id=?")
      .get(alice.user.id);
    expect(String(row?.password_hash)).not.toContain("sample password");
    const stored = database()
      .prepare("SELECT token_hash FROM sessions WHERE member_id=?")
      .get(alice.user.id);
    expect(stored?.token_hash).not.toBe(token(alice.cookie));
    expect(viewerFor(token(alice.cookie))?.id).toBe(alice.user.id);
    const rotated = issueSession(alice.user, token(alice.cookie), true);
    expect(rotated).toContain("HttpOnly; SameSite=Lax");
    expect(rotated).toContain("Secure");
    expect(viewerFor(token(alice.cookie))).toBe(null);
    const next = token(rotated.split(";")[0]);
    expect(viewerFor(next)?.id).toBe(alice.user.id);
    database().prepare("UPDATE sessions SET expires_at=0").run();
    expect(viewerFor(next)).toBe(null);
    const latest = token(issueSession(alice.user, null, false).split(";")[0]);
    revokeSession(latest, false);
    expect(viewerFor(latest)).toBe(null);
  });
  it("keeps request identity isolated across interleaved asynchronous work", async () => {
    const alice = await account("alice"),
      bob = await account("bobby");
    const results = await Promise.all([
      as(alice.cookie, async () => {
        await Promise.resolve();
        return sessionData();
      }),
      as(bob.cookie, async () => {
        await Promise.resolve();
        return sessionData();
      }),
      as("", sessionData),
    ]);
    expect(results).toEqual([
      { kind: "authenticated", user: alice.user },
      { kind: "authenticated", user: bob.user },
      { kind: "guest" },
    ]);
    expect(JSON.stringify(results)).not.toContain("password_hash");
    expect(JSON.stringify(results)).not.toContain("@example.test");
  });
  it("makes publication retries idempotent and rejects a reused submission with different content", async () => {
    const alice = await account("alice");
    const input = form({ body: "A new note", topic: "design", requestId: "same-request" });
    const first = await as(alice.cookie, () => createPost(IDLE, input));
    expect(first.status).toBe("success");
    expect(await as(alice.cookie, () => createPost(IDLE, input))).toEqual(first);
    expect((await as(alice.cookie, () => timelineData("design", "A new note"))).posts.length).toBe(
      1,
    );
    input.set("body", "Different content");
    expect((await as(alice.cookie, () => createPost(IDLE, input))).status).toBe("error");
    input.set("requestId", "new-request");
    input.set("body", "x".repeat(501));
    expect((await as(alice.cookie, () => createPost(IDLE, input))).status).toBe("error");
  });
  it("sets intended appreciation per member without double counting retries", async () => {
    const alice = await account("alice"),
      bob = await account("bobby");
    const post = insertPost(alice.user, "Appreciation", "community", "reaction-post");
    await as(alice.cookie, () => likePost(post.id, true));
    await as(alice.cookie, () => likePost(post.id, true));
    await as(bob.cookie, () => likePost(post.id, true));
    const feed = await as(alice.cookie, () => timelineData("all", "Appreciation"));
    expect(feed.posts[0].likes).toBe(2);
    expect(feed.posts[0].liked).toBe(true);
    await as(alice.cookie, () => likePost(post.id, false));
    await as(alice.cookie, () => likePost(post.id, false));
    expect((await as(bob.cookie, () => timelineData("all", "Appreciation"))).posts[0].likes).toBe(
      1,
    );
  });
  it("authorizes every conversation read and write and preserves messages across restart", async () => {
    const alice = await account("alice"),
      bob = await account("bobby");
    const thread = listThreads(alice.user)[0];
    expect(listThreads(bob.user).some((value) => value.id === thread.id)).toBe(false);
    expect(await as(bob.cookie, () => messagesData(thread.id))).toEqual({ kind: "missing" });
    expect(() => listMessages(bob.user, thread.id)).toThrow("not available");
    const input = form({ body: "Private note", threadId: thread.id, requestId: "message-retry" });
    expect((await as(bob.cookie, () => sendMessage(IDLE, input))).status).toBe("error");
    const first = await as(alice.cookie, () => sendMessage(IDLE, input));
    expect(first.status).toBe("success");
    expect(await as(alice.cookie, () => sendMessage(IDLE, input))).toEqual(first);
    closeDatabase();
    expect(
      listMessages(alice.user, thread.id).filter((message) => message.body === "Private note")
        .length,
    ).toBe(1);
    expect(viewerFor(token(alice.cookie))?.id).toBe(alice.user.id);
  });
  it("updates only the request member and rolls back conflicting handles", async () => {
    const alice = await account("alice"),
      bob = await account("bobby");
    const input = form({
      displayName: "Changed",
      handle: "bobby",
      bio: "Bio",
      email: "new@example.test",
    });
    expect((await as(alice.cookie, () => updateSettings(IDLE, input))).status).toBe("error");
    expect(settingsFor(alice.user).displayName).toBe("alice");
    input.set("handle", "alice_new");
    expect((await as(alice.cookie, () => updateSettings(IDLE, input))).status).toBe("success");
    expect(settingsFor(bob.user).displayName).toBe("bobby");
    expect(await as("", settingsData)).toEqual({ kind: "unauthenticated" });
  });
  it("rolls back a partially executed transaction", () => {
    expect(() =>
      transaction((db) => {
        db.prepare("INSERT INTO conversations VALUES (?)").run("rolled-back");
        throw new Error("abort");
      }),
    ).toThrow("abort");
    expect(database().prepare("SELECT id FROM conversations WHERE id=?").get("rolled-back")).toBe(
      undefined,
    );
  });
  it("rejects cross-site, wrong content-type, and oversized authentication bodies", async () => {
    expect(
      (await as("", () => POST(request({ mode: "login" }, { origin: "https://elsewhere.test" }))))
        .status,
    ).toBe(403);
    expect(
      (await as("", () => POST(request({ mode: "login" }, { "sec-fetch-site": "cross-site" }))))
        .status,
    ).toBe(403);
    expect(
      (await as("", () => POST(request({ mode: "login" }, { "content-type": "application/json" }))))
        .status,
    ).toBe(415);
    expect(
      (await as("", () => POST(request({ mode: "login", handle: "a".repeat(5000) })))).status,
    ).toBe(413);
  });
  it("issues the session only in HTTP headers and invalidates it on logout", async () => {
    const signup = request({
      mode: "signup",
      handle: "alice",
      password: "a sample password 123",
      name: "Alice",
      email: "alice@example.test",
    });
    const response = await as("", () => POST(signup));
    expect(response.status).toBe(200);
    expect(response.headers.get("cache-control")).toBe("no-store");
    const cookie = String(response.headers.get("set-cookie")).split(";")[0];
    expect(cookie).toContain(SESSION_COOKIE);
    expect(JSON.stringify(await response.json())).not.toContain(token(cookie));
    const logout = await as(cookie, () => POST(request({ mode: "logout" })));
    expect(logout.headers.get("set-cookie")).toContain("Max-Age=0");
    expect(viewerFor(token(cookie))).toBe(null);
  });
  it("injects Effect dependencies and keeps expected failures separate from defects", async () => {
    const alice = await account("alice");
    let calls = 0;
    const store = {
      insertPost: () => {
        calls++;
        throw new Error("storage is down");
      },
      setReaction: () => {
        throw new Error("unused");
      },
      insertMessage: () => {
        throw new Error("unused");
      },
      saveSettings: () => {
        throw new Error("unused");
      },
    };
    const live = layerMerge(
      layerSucceed(IdentityService, { current: () => alice.user }),
      layerSucceed(SocialStore, store),
    );
    const invalid = await runPromiseExit(
      provide<Post, MutationProblem, empty, Identity | Store, empty, empty>(
        publishNote(form({ body: "", topic: "design", requestId: "request" })),
        live,
      ),
    );
    expect(invalid.kind).toBe("failure");
    if (invalid.kind === "failure") expect(invalid.cause.kind).toBe("fail");
    expect(calls).toBe(0);
    const defect = await runPromiseExit(
      provide<Post, MutationProblem, empty, Identity | Store, empty, empty>(
        publishNote(form({ body: "Valid", topic: "design", requestId: "request" })),
        live,
      ),
    );
    if (defect.kind !== "failure") throw new Error("expected defect");
    expect(defect.cause.kind).toBe("die");
    expect(calls).toBe(1);
  });
});
