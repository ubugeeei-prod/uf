// @flow
//
// Component tests for the Simple SNS example. The architecture test beside
// this one checks that the example uses the right uf features; these assert the
// pieces a reader can touch still behave like a product UI.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it, uft } from "@uniflowed/test";
import { cleanup, render, screen, userEvent } from "@uniflowed/react-testing";

import typeof * as SocialActions from "../../examples/simple-sns/app/social-actions.js";
import {
  type FormState,
  type Message,
  type MessageThread,
  type Post,
  type Settings,
  type User,
} from "../../examples/simple-sns/app/social-model.js";

const ACTIONS = "../../examples/simple-sns/app/social-actions.js";

afterEach(() => {
  cleanup();
  uft.unmock(ACTIONS);
  uft.resetModules();
});

describe("the Simple SNS client components", () => {
  it("renders the signup form as a real account screen", async () => {
    const { AuthClient } = await components();

    render(<AuthClient mode="signup" />);

    expect(screen.getByRole("heading", { name: "Reserve a demo profile" })).toBeInTheDocument();
    expect(screen.getByLabelText("Name")).toBeInTheDocument();
    expect(screen.getByLabelText("Handle")).toBeInTheDocument();
    expect(screen.getByLabelText("Bio")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create account" })).toBeInTheDocument();
  });

  it("filters timeline posts by topic and search text", async () => {
    const { TimelineClient } = await components();

    render(<TimelineClient initialPosts={posts()} viewer={viewer()} />);

    expect(screen.getByText("Release train is ready")).toBeInTheDocument();
    expect(screen.getByText("Runtime graph changed")).toBeInTheDocument();

    await userEvent.selectOptions(html(screen.getByRole("combobox", { name: "Filter by topic" })), [
      "runtime",
    ]);
    expect(screen.queryByText("Release train is ready")).toBe(null);
    expect(screen.getByText("Runtime graph changed")).toBeInTheDocument();

    await userEvent.type(html(screen.getByRole("textbox", { name: "Search timeline" })), "nobody");
    expect(screen.queryByText("Runtime graph changed")).toBe(null);
  });

  it("switches direct-message threads without leaving the messages surface", async () => {
    const { DirectMessagesClient } = await components();

    render(<DirectMessagesClient threads={threads()} initialMessages={messages()} />);

    expect(screen.getByText("Can the settings surface use the same pattern?")).toBeInTheDocument();
    expect(screen.queryByText("SQLite fixture is ready.")).toBe(null);

    await userEvent.click(html(screen.getByRole("button", { name: /Sora Lin/ })));
    expect(screen.getByText("SQLite fixture is ready.")).toBeInTheDocument();
    expect(screen.queryByText("Can the settings surface use the same pattern?")).toBe(null);
  });

  it("renders settings with the persisted profile values", async () => {
    const { SettingsClient } = await components();

    render(<SettingsClient initial={settings()} />);

    expect(screen.getByRole("heading", { name: "Mika Tan" })).toBeInTheDocument();
    expect(screen.getByLabelText("Display name")).toHaveValue("Mika Tan");
    expect(screen.getByLabelText("Handle")).toHaveValue("mika");
    expect(screen.getByLabelText("Email")).toHaveValue("mika@example.test");
    expect(screen.getByLabelText("Daily digest")).toBeChecked();
    expect(screen.getByLabelText("Quiet mode")).not.toBeChecked();
  });
});

async function components() {
  await uft.mock<SocialActions>(ACTIONS, actionMocks);
  const [auth, messages, settingsModule, timeline] = await Promise.all([
    import("../../examples/simple-sns/app/auth-client.js"),
    import("../../examples/simple-sns/app/messages/direct-messages-client.js"),
    import("../../examples/simple-sns/app/settings/settings-client.js"),
    import("../../examples/simple-sns/app/timeline-client.js"),
  ]);
  return {
    AuthClient: auth.AuthClient,
    DirectMessagesClient: messages.DirectMessagesClient,
    SettingsClient: settingsModule.SettingsClient,
    TimelineClient: timeline.TimelineClient,
  };
}

function actionMocks(): Partial<SocialActions> {
  return {
    createPost: async (previous: FormState<Post>, _form: FormData): Promise<FormState<Post>> =>
      previous,
    likePost: async (id: string): Promise<{| readonly id: string, readonly likes: number |}> => ({
      id,
      likes: 1,
    }),
    sendMessage: async (
      previous: FormState<Message>,
      _form: FormData,
    ): Promise<FormState<Message>> => previous,
    signIn: async (previous: FormState<User>, _form: FormData): Promise<FormState<User>> =>
      previous,
    signUp: async (previous: FormState<User>, _form: FormData): Promise<FormState<User>> =>
      previous,
    updateSettings: async (
      previous: FormState<Settings>,
      _form: FormData,
    ): Promise<FormState<Settings>> => previous,
  };
}

function html(element: Element): HTMLElement {
  if (!(element instanceof HTMLElement)) {
    throw new Error("expected test query to return an HTMLElement");
  }
  return element;
}

function viewer(): User {
  return {
    id: "u-mika",
    name: "Mika Tan",
    handle: "mika",
    avatar: "MT",
    bio: "Builds product loops.",
  };
}

function posts(): Array<Post> {
  const user = viewer();
  return [
    {
      id: "release",
      author: user,
      body: "Release train is ready",
      topic: "release",
      likes: 2,
      replies: 1,
      createdAt: "2026-09-10T08:20:00.000Z",
      liked: true,
    },
    {
      id: "runtime",
      author: { ...user, id: "u-sora", name: "Sora Lin", handle: "sora", avatar: "SL" },
      body: "Runtime graph changed",
      topic: "runtime",
      likes: 3,
      replies: 2,
      createdAt: "2026-09-10T07:35:00.000Z",
      liked: false,
    },
  ];
}

function threads(): Array<MessageThread> {
  return [
    {
      id: "thread-ren",
      name: "Ren Ito",
      handle: "ren",
      unread: 2,
      lastMessage: "Can the settings surface use the same pattern?",
    },
    {
      id: "thread-sora",
      name: "Sora Lin",
      handle: "sora",
      unread: 0,
      lastMessage: "SQLite fixture is ready.",
    },
  ];
}

function messages(): Array<Message> {
  return [
    {
      id: "m-ren-1",
      threadId: "thread-ren",
      author: "them",
      body: "Can the settings surface use the same pattern?",
      sentAt: "2026-09-10T08:05:00.000Z",
      delivery: "read",
    },
    {
      id: "m-sora-1",
      threadId: "thread-sora",
      author: "them",
      body: "SQLite fixture is ready.",
      sentAt: "2026-09-10T07:15:00.000Z",
      delivery: "read",
    },
  ];
}

function settings(): Settings {
  return {
    displayName: "Mika Tan",
    handle: "mika",
    bio: "Builds product loops.",
    email: "mika@example.test",
    digest: true,
    quietMode: false,
  };
}
