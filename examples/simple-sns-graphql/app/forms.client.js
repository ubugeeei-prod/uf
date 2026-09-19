"use client";
// @flow
import { styled, styles as sharedStyles } from "./commonplace.stylex.js";
import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { useMutation } from "@uniflowed/relay";
import { register as registerMutation, login, sendMessage, updateSettings } from "./operations.js";
import { Avatar, SignInPrompt } from "./ui.js";
import { displayTime } from "./social-model.js";
import type { SnsScreenQuery$data } from "./__generated__/SnsScreenQuery.graphql.js";
import type { SnsRegisterMutation } from "./__generated__/SnsRegisterMutation.graphql.js";
import type { SnsLoginMutation } from "./__generated__/SnsLoginMutation.graphql.js";
import type { SnsSendMessageMutation } from "./__generated__/SnsSendMessageMutation.graphql.js";
import type { SnsUpdateSettingsMutation } from "./__generated__/SnsUpdateSettingsMutation.graphql.js";

/** Credentials go to the upstream service; only its HttpOnly cookie returns. */
export component AccountForm(register: boolean) {
  const [signup, signingUp] = useMutation<
    SnsRegisterMutation["variables"],
    SnsRegisterMutation["response"],
  >(registerMutation);
  const [signin, signingIn] = useMutation<
    SnsLoginMutation["variables"],
    SnsLoginMutation["response"],
  >(login);
  const [error, setError] = useState("");
  const pending = signingUp || signingIn;
  return (
    <section className="auth-panel">
      <header className="page-heading">
        <div>
          <h1>{register ? "Join Commonplace" : "Welcome back"}</h1>
          <p>
            {register ? "A place to share what you are working on." : "Sign in to your community."}
          </p>
        </div>
      </header>
      <form
        className="auth-form"
        onSubmit={(event) => {
          event.preventDefault();
          setError("");
          const values = new FormData(event.currentTarget);
          const handle = String(values.get("handle"));
          const password = String(values.get("password"));
          const callbacks = {
            onCompleted: () => window.location.assign("/"),
            onError: (failure: Error) => setError(failure.message),
          };
          if (register)
            signup({
              ...callbacks,
              variables: {
                input: {
                  name: String(values.get("name")),
                  email: String(values.get("email")),
                  handle,
                  password,
                },
              },
            });
          else signin({ ...callbacks, variables: { handle, password } });
        }}
      >
        {register ? (
          <label className="field">
            <span>Display name</span>
            <input name="name" autoComplete="name" required maxLength={80} />
          </label>
        ) : null}
        <label className="field">
          <span>Handle</span>
          <input name="handle" autoComplete="username" required pattern="[a-z][a-z0-9_]{2,23}" />
        </label>
        {register ? (
          <label className="field">
            <span>Email</span>
            <input type="email" name="email" autoComplete="email" required />
          </label>
        ) : null}
        <label className="field">
          <span>Password</span>
          <input
            type="password"
            name="password"
            autoComplete={register ? "new-password" : "current-password"}
            required
            minLength={register ? 12 : undefined}
            maxLength={256}
          />
        </label>
        {error ? (
          <p role="alert" {...styled("post-error", sharedStyles.postError)}>
            {error}
          </p>
        ) : null}
        <button className="button primary" disabled={pending}>
          {pending ? "Please wait…" : register ? "Create account" : "Sign in"}
        </button>
        <p className="auth-switch">
          <Link to={register ? "/login" : "/signup"}>
            {register ? "Already have an account? Sign in" : "New here? Create an account"}
          </Link>
        </p>
      </form>
    </section>
  );
}

/** The service derives the account to update; no account ID enters the input. */
export component SettingsForm(settings: SnsScreenQuery$data["settings"]) {
  const [commit, pending] = useMutation<
    SnsUpdateSettingsMutation["variables"],
    SnsUpdateSettingsMutation["response"],
  >(updateSettings);
  const [feedback, setFeedback] = useState("");
  if (settings == null) return <SignInPrompt />;
  return (
    <section className="settings-panel">
      <header className="page-heading">
        <div>
          <h1>Settings</h1>
          <p>Your profile and account.</p>
        </div>
      </header>
      <form
        className="settings-form"
        onSubmit={(event) => {
          event.preventDefault();
          setFeedback("");
          const values = new FormData(event.currentTarget);
          commit({
            variables: {
              input: {
                displayName: String(values.get("displayName")),
                bio: String(values.get("bio")),
                email: String(values.get("email")),
              },
            },
            onCompleted: () => setFeedback("Your changes are saved."),
            onError: (failure: Error) => setFeedback(failure.message),
          });
        }}
      >
        <label className="field">
          <span>Display name</span>
          <input name="displayName" defaultValue={settings.displayName} required maxLength={80} />
        </label>
        <label className="field">
          <span>Bio</span>
          <textarea name="bio" defaultValue={settings.bio} maxLength={240} />
        </label>
        <label className="field">
          <span>Email</span>
          <input type="email" name="email" defaultValue={settings.email} required />
        </label>
        <button className="button primary" disabled={pending}>
          {pending ? "Saving…" : "Save changes"}
        </button>
        {feedback ? <p role="status">{feedback}</p> : null}
      </form>
    </section>
  );
}

/** Private reads and idempotent writes stay behind the GraphQL service. */
export component Inbox(data: SnsScreenQuery$data) {
  const [commit, pending] = useMutation<
    SnsSendMessageMutation["variables"],
    SnsSendMessageMutation["response"],
  >(sendMessage);
  const [body, setBody] = useState("");
  const [requestId, setRequestId] = useState("");
  const [error, setError] = useState("");
  if (data.viewer == null) return <SignInPrompt title="Sign in to read your messages" />;
  const conversation = data.conversation;
  return (
    <>
      <header className="page-heading">
        <div>
          <h1>Inbox</h1>
          <p>Your conversations, one at a time.</p>
        </div>
      </header>
      <div className="conversation-layout">
        <nav className="thread-list" aria-label="Conversations">
          {data.threads?.map((thread) => (
            <Link
              key={thread.id}
              className="thread"
              to={`/messages?thread=${encodeURIComponent(thread.id)}`}
              aria-current={conversation?.thread.id === thread.id ? "page" : undefined}
            >
              <Avatar
                user={{
                  id: thread.id,
                  name: thread.name,
                  handle: thread.handle,
                  avatar: thread.avatar,
                  photo: thread.photo,
                  bio: "",
                }}
                small
              />
              <span>
                <strong>{thread.name}</strong>
                <small>{thread.lastMessage}</small>
              </span>
            </Link>
          ))}
        </nav>
        {conversation ? (
          <section className="conversation">
            <header className="conversation-heading">
              <strong>{conversation.thread.name}</strong>
            </header>
            <div className="message-log" role="log" aria-label="Messages">
              {conversation.messages.map((message) => (
                <div
                  key={message.id}
                  className={`message ${message.author === "me" ? "mine" : ""}`}
                >
                  <p className="message-content">{message.body}</p>
                  <time dateTime={message.sentAt}>{displayTime(message.sentAt)}</time>
                </div>
              ))}
            </div>
            <form
              className="message-composer"
              onSubmit={(event) => {
                event.preventDefault();
                const id = requestId || crypto.randomUUID();
                setRequestId(id);
                setError("");
                commit({
                  variables: { input: { threadId: conversation.thread.id, body, requestId: id } },
                  updater: (store) => {
                    const current = store
                      .getRoot()
                      .getLinkedRecord("conversation", { id: conversation.thread.id });
                    const message = store.getRootField("sendMessage");
                    if (current && message) {
                      const previous = current.getLinkedRecords("messages") ?? [];
                      if (!previous.some((item) => item?.getDataID() === message.getDataID()))
                        current.setLinkedRecords([...previous, message], "messages");
                    }
                  },
                  onCompleted: () => {
                    setBody("");
                    setRequestId("");
                  },
                  onError: (failure: Error) => setError(failure.message),
                });
              }}
            >
              <textarea
                aria-label="Message"
                value={body}
                maxLength={2000}
                required
                onChange={(event) => {
                  setBody(event.currentTarget.value);
                  setRequestId("");
                }}
              />
              <button className="button primary" disabled={pending}>
                {pending ? "Sending…" : "Send message"}
              </button>
              {error ? <p role="alert">{error}</p> : null}
            </form>
          </section>
        ) : (
          <p>Conversation not found.</p>
        )}
      </div>
    </>
  );
}
