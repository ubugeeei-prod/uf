// @flow
import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it, uft } from "@uniflowed/test";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  userEvent,
  waitFor,
  within,
} from "@uniflowed/react-testing";
import typeof * as SocialActions from "../../examples/simple-sns/app/social-actions.js";
import typeof * as SocialQueries from "../../examples/simple-sns/app/social-queries.js";
import {
  failed,
  succeeded,
  type FeedData,
  type FeedFilter,
  type User,
  type Post,
  type Message,
  type InboxData,
  type ConversationData,
} from "../../examples/simple-sns/app/social-model.js";
const ACTIONS = "../../examples/simple-sns/app/social-actions.js";
const QUERIES = "../../examples/simple-sns/app/social-queries.js";
const ROUTER = "@uniflowed/router";
afterEach(() => {
  cleanup();
  uft.unmock(ACTIONS);
  uft.unmock(QUERIES);
  uft.unmock(ROUTER);
  uft.resetModules();
});
component TestLink(to: string, ...{ children }: React.ElementConfig<"a">) {
  return <a href={to}>{children}</a>;
}
function unconfigured(): empty {
  throw new Error("Unexpected request");
}
async function components(
  actions: Partial<SocialActions> = {},
  queries: Partial<SocialQueries> = {},
) {
  await uft.mock<SocialActions>(ACTIONS, () => ({
    createPost: async () => unconfigured(),
    likePost: async () => unconfigured(),
    sendMessage: async () => unconfigured(),
    updateSettings: async () => unconfigured(),
    ...actions,
  }));
  await uft.mock<SocialQueries>(QUERIES, () => ({
    sessionData: async () => unconfigured(),
    timelineData: async () => unconfigured(),
    threadsData: async () => unconfigured(),
    messagesData: async () => unconfigured(),
    settingsData: async () => unconfigured(),
    ...queries,
  }));
  await uft.mock(ROUTER, () => ({ Link: TestLink }));
  const [auth, timeline, inbox, settings, messages] = await Promise.all([
    import("../../examples/simple-sns/app/auth-client.js"),
    import("../../examples/simple-sns/app/timeline-client.js"),
    import("../../examples/simple-sns/app/messages/inbox.client.js"),
    import("../../examples/simple-sns/app/settings/settings-client.js"),
    import("../../examples/simple-sns/app/messages/direct-messages-client.js"),
  ]);
  return { ...auth, ...timeline, ...inbox, ...settings, ...messages };
}
async function renderAsync(element: React.MixedElement): Promise<void> {
  await act(async () => {
    render(element);
  });
}

const USER: User = { id: "viewer", name: "Alice", handle: "alice", avatar: "A", bio: "" };
const POST: Post = {
  id: "existing",
  author: USER,
  body: "An existing note",
  topic: "design",
  likes: 0,
  liked: false,
  createdAt: "2026-09-11T00:00:00Z",
};
const FILTER: FeedFilter = { topic: "all", query: "", page: 1 };
function feed(): FeedData {
  return { ...FILTER, posts: [POST], hasNext: false };
}
function deferred<T>(): {| promise: Promise<T>, resolve: (T) => void, reject: (Error) => void |} {
  let resolve: (T) => void = () => {
    throw new Error("not initialized");
  };
  let reject: (Error) => void = () => {
    throw new Error("not initialized");
  };
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function html(value: Element): HTMLElement {
  if (!(value instanceof HTMLElement)) throw new Error("expected HTMLElement");
  return value;
}

describe("Commonplace React interactions", () => {
  it("shows a ready paused clip without leaving its loading notice and releases inactive media", async () => {
    await components();
    const { ClipPlayer } = await import("../../examples/simple-sns/app/clips/clips-client.js");
    const clip = {
      id: "test",
      title: "Test clip",
      description: "A test scene.",
      src: "/test.mp4",
      poster: "/test.jpg",
      credit: "Test",
      source: "https://example.test",
    };
    const view = render(<ClipPlayer clip={clip} active={false} muted={true} onMute={() => {}} />);
    const video = screen.getByLabelText("Test clip. A test scene. Silent stock footage.");
    expect(video.getAttribute("src")).toBe(null);
    await act(async () =>
      view.rerender(<ClipPlayer clip={clip} active={true} muted={true} onMute={() => {}} />),
    );
    Reflect.defineProperty(video, "paused", { value: true, configurable: true });
    await act(async () => {
      fireEvent(video, "waiting");
    });
    expect(screen.getByText("Loading video…")).toBeInTheDocument();
    await act(async () => {
      fireEvent(video, "loadeddata");
    });
    expect(screen.queryByText("Loading video…")).toBe(null);
    expect(screen.getByRole("button", { name: "Play video", exact: true })).toBeInTheDocument();
    await act(async () =>
      view.rerender(<ClipPlayer clip={clip} active={false} muted={true} onMute={() => {}} />),
    );
    expect(video.getAttribute("src")).toBe(null);
  });
  it("exposes correctly associated account fields", async () => {
    const { AuthClient } = await components();
    await renderAsync(<AuthClient mode="signup" />);
    expect(screen.getByRole("heading", { name: "Create an account" })).toBeInTheDocument();
    for (const label of ["Name", "Email address", "Handle", "Password"])
      expect(screen.getByLabelText(label)).toBeInTheDocument();
    expect(screen.getByLabelText("Password").getAttribute("autocomplete")).toBe("new-password");
  });
  it("keeps the composer usable while the feed suspends and preserves its draft in Activity", async () => {
    const { TimelineClient } = await components();
    const pending = deferred<FeedData>();
    await renderAsync(
      <TimelineClient
        initial={pending.promise}
        filter={FILTER}
        session={{ kind: "authenticated", user: USER }}
      />,
    );
    expect(screen.getByRole("status", { name: "Loading notes" })).toBeInTheDocument();
    await act(async () =>
      userEvent.type(html(screen.getByLabelText("Post body")), "Draft while loading"),
    );
    await act(async () =>
      userEvent.click(html(screen.getByRole("button", { name: "Write a note" }))),
    );
    expect(screen.getByRole("button", { name: "Write a note" }).getAttribute("aria-expanded")).toBe(
      "false",
    );
    expect(screen.getByLabelText("Post body")).not.toBeVisible();
    await act(async () =>
      userEvent.click(html(screen.getByRole("button", { name: "Write a note" }))),
    );
    expect(screen.getByLabelText("Post body")).toHaveValue("Draft while loading");
    await act(async () => pending.resolve(feed()));
    expect(screen.getByText("An existing note")).toBeInTheDocument();
    expect(screen.getByLabelText("Post body")).toHaveValue("Draft while loading");
  });
  it("adopts a refreshed loader resource without losing the composer draft", async () => {
    const { TimelineClient } = await components();
    const session = { kind: "authenticated", user: USER } as const;
    const initial = Promise.resolve(feed());
    const view = render(<TimelineClient initial={initial} filter={FILTER} session={session} />);
    await act(async () => {
      await initial;
    });
    await act(async () =>
      userEvent.type(html(screen.getByLabelText("Post body")), "Keep my draft"),
    );
    const refreshed = Promise.resolve<FeedData>({
      ...feed(),
      posts: [{ ...POST, body: "Fresh server response" }],
    });
    await act(async () =>
      view.rerender(<TimelineClient initial={refreshed} filter={FILTER} session={session} />),
    );
    expect(screen.getByText("Fresh server response")).toBeInTheDocument();
    expect(screen.queryByText("An existing note")).toBe(null);
    expect(screen.getByLabelText("Post body")).toHaveValue("Keep my draft");
  });
  it("reveals the inbox list independently of the conversation", async () => {
    const { InboxRegions } = await components();
    const threads = deferred<InboxData>(),
      conversation = deferred<ConversationData>();
    await renderAsync(
      <InboxRegions
        threads={threads.promise}
        conversation={conversation.promise}
        threadId="thread"
      />,
    );
    expect(screen.getByRole("status", { name: "Loading conversations" })).toBeInTheDocument();
    await act(async () =>
      threads.resolve({
        kind: "ready",
        value: [{ id: "thread", name: "Ren", handle: "ren", avatar: "R", lastMessage: "Hello" }],
      }),
    );
    expect(screen.getByRole("link", { name: /Ren/ })).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "Loading messages" })).toBeInTheDocument();
    await act(async () => conversation.resolve({ kind: "empty" }));
    expect(screen.getByRole("heading", { name: "No conversations yet" })).toBeInTheDocument();
  });
  it("rolls back a failed optimistic note, retains the draft and reuses its request ID", async () => {
    const requests: Array<string> = [];
    const response = deferred<ReturnType<typeof failed>>();
    const { TimelineClient } = await components({
      createPost: async (_old, form) => {
        requests.push(String(form.get("requestId")));
        return response.promise;
      },
    });
    await renderAsync(
      <TimelineClient
        initial={Promise.resolve(feed())}
        filter={FILTER}
        session={{ kind: "authenticated", user: USER }}
      />,
    );
    await screen.findByText("An existing note");
    await act(async () =>
      userEvent.type(html(screen.getByLabelText("Post body")), "An optimistic note"),
    );
    await act(async () =>
      userEvent.click(html(screen.getByRole("button", { name: "Publish note" }))),
    );
    expect(screen.getByRole("button", { name: "Publishing…" })).toBeDisabled();
    expect(
      within(screen.getByRole("region", { name: "Timeline posts" })).getByText(
        "An optimistic note",
      ),
    ).toBeInTheDocument();
    await act(async () => response.resolve(failed("Not saved. Please retry.")));
    expect(
      within(screen.getByRole("region", { name: "Timeline posts" })).queryByText(
        "An optimistic note",
      ),
    ).toBe(null);
    expect(screen.getByLabelText("Post body")).toHaveValue("An optimistic note");
    await act(async () =>
      userEvent.click(html(screen.getByRole("button", { name: "Publish note" }))),
    );
    await waitFor(() => expect(requests.length).toBe(2));
    expect(requests[0]).toBe(requests[1]);
  });
  it("commits a published note once and clears the successful draft", async () => {
    const { TimelineClient } = await components({
      createPost: async () =>
        succeeded({ ...POST, id: "saved", body: "Published note" }, "Note published."),
    });
    await renderAsync(
      <TimelineClient
        initial={Promise.resolve(feed())}
        filter={FILTER}
        session={{ kind: "authenticated", user: USER }}
      />,
    );
    await screen.findByText("An existing note");
    await act(async () =>
      userEvent.type(html(screen.getByLabelText("Post body")), "Published note"),
    );
    await act(async () =>
      userEvent.click(html(screen.getByRole("button", { name: "Publish note" }))),
    );
    await screen.findByText("Note published.");
    expect(
      within(screen.getByRole("region", { name: "Timeline posts" })).getAllByText("Published note")
        .length,
    ).toBe(1);
    expect(screen.getByLabelText("Post body")).toHaveValue("");
  });
  it("retries only the failed feed while retaining the composer draft", async () => {
    let reads = 0;
    const first = deferred<FeedData>();
    const { TimelineClient } = await components(
      {},
      {
        timelineData: async () => {
          reads++;
          return feed();
        },
      },
    );
    await renderAsync(
      <TimelineClient
        initial={first.promise}
        filter={FILTER}
        session={{ kind: "authenticated", user: USER }}
      />,
    );
    await act(async () =>
      userEvent.type(html(screen.getByLabelText("Post body")), "Keep this draft"),
    );
    await act(async () => first.reject(new Error("read failed")));
    expect(screen.getByRole("heading", { name: "Could not load notes" })).toBeInTheDocument();
    await act(async () => userEvent.click(html(screen.getByRole("button", { name: "Try again" }))));
    await screen.findByText("An existing note");
    expect(reads).toBe(1);
    expect(screen.getByLabelText("Post body")).toHaveValue("Keep this draft");
  });
  it("preserves failed profile edits and announces the field error", async () => {
    const { SettingsClient } = await components({
      updateSettings: async () => failed("Choose another handle.", { handle: "Already taken." }),
    });
    await renderAsync(
      <SettingsClient
        initial={{ displayName: "Alice", handle: "alice", email: "alice@example.test", bio: "" }}
      />,
    );
    await act(async () => userEvent.type(html(screen.getByLabelText("Handle")), "_new"));
    await act(async () =>
      userEvent.click(html(screen.getByRole("button", { name: "Save changes" }))),
    );
    await screen.findByText("Already taken.");
    expect(screen.getByLabelText("Handle")).toHaveValue("alice_new");
    expect(screen.getByLabelText("Handle").getAttribute("aria-invalid")).toBe("true");
  });
  it("preserves a failed message draft and rolls back its optimistic bubble", async () => {
    const pending = deferred<ReturnType<typeof failed>>();
    const { DirectMessagesClient } = await components({ sendMessage: async () => pending.promise });
    await renderAsync(
      <DirectMessagesClient
        thread={{ id: "thread", name: "Ren", handle: "ren", avatar: "R", lastMessage: "" }}
        initialMessages={[]}
      />,
    );
    await act(async () => userEvent.type(html(screen.getByLabelText("Message body")), "A message"));
    await act(async () => userEvent.click(html(screen.getByRole("button", { name: "Send" }))));
    expect(
      within(screen.getByRole("log", { name: "Messages" })).getByText("A message"),
    ).toBeInTheDocument();
    await act(async () => pending.resolve(failed("Not sent.")));
    expect(within(screen.getByRole("log", { name: "Messages" })).queryByText("A message")).toBe(
      null,
    );
    expect(screen.getByLabelText("Message body")).toHaveValue("A message");
  });
});
