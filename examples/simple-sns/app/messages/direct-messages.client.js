"use client";
// @flow

import * as React from "@uniflowed/react";

import { callAction } from "../_shared/action-result.client.js";

import { useActionState, useOptimistic, useState, useEffect, useRef } from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";

import { sendMessage } from "../_server/social-actions.js";
import { FieldError, FormStatus, SubmitButton } from "../_shared/form-ui.client.js";
import { Avatar, Icon, styles as uiStyles } from "../_shared/ui.js";
import {
  IDLE,
  fieldError,
  MAX_MESSAGE_LENGTH,
  displayTime,
  type FormState,
  type Message,
  type MessageThread,
} from "../_shared/social-model.js";

type LocalMessage = {| readonly requestId: string, readonly message: Message |};

/**
 * Preserve a conversation draft while pending and reconcile each submitted message once.
 * Failed sends roll back the optimistic bubble and retain the request ID for an idempotent retry.
 */

export component DirectMessagesClient(
  thread         : MessageThread,
  initialMessages: $ReadOnlyArray<Message>,
) {
  const [committed, setCommitted]  = useState<Array<LocalMessage>>(() =>
    initialMessages.map((message) => ({ requestId: message.id, message })),
  );
  const [entries,   addOptimistic] = useOptimistic<Array<LocalMessage>, LocalMessage>(
    committed,
    (current, draft) =>
      current.some((entry) => entry.requestId === draft.requestId) ? current : [...current, draft],
  );
  const messages = entries.map((entry) => entry.message);
  const [body,      setBody]      = useState("");
  const [requestId, setRequestId] = useState("");
  const [state, submit, pending]  = useActionState<FormState<Message>, FormData>(
    async (_previous: FormState<Message>, form: FormData): Promise<FormState<Message>> => {
      const text = String(form.get("body") ?? "").trim();
      const submissionId = requestId || crypto.randomUUID();
      setRequestId(submissionId);
      const submitted = new FormData();
      submitted.set("body", text);
      submitted.set("threadId", thread.id);
      submitted.set("requestId", submissionId);
      if (text.length > 0 && text.length <= MAX_MESSAGE_LENGTH)
        addOptimistic({
          requestId: submissionId,
          message: {
            id      : `pending-${submissionId}`,
            threadId: thread.id,
            author  : "me",
            body    : text,
            sentAt  : new Date().toISOString(),
          },
        });
      const result = await callAction(
        () => sendMessage(IDLE, submitted),
        "Your message was not sent. Your draft is still here; try again.",
      );
      match (result) {
        {status: "success", value: const saved, ...} => {
          setCommitted((current) => [
            ...current.filter((entry) => entry.message.id !== saved.id),
            { requestId: submissionId, message: saved },
          ]);
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
    <section {...props(uiStyles.conversation)} aria-label={`Conversation with ${thread.name}`}>
      <header {...props(uiStyles.conversationHeader)}>
        <Avatar
          user={{
            id    : thread.id,
            name  : thread.name,
            handle: thread.handle,
            avatar: thread.avatar,
            photo : thread.photo,
            bio   : "",
          }}
          small
        />
        <div>
          <h2 {...props(styles.conversationTitle)}>{thread.name}</h2>
          <p {...props(styles.conversationMeta)}>@{thread.handle}</p>
        </div>
        <span {...props(styles.conversationPrivate)} title="Private conversation">
          <Icon name="lock" size={16} />
        </span>
      </header>
      <MessageLog lastId={messages.at(-1)?.id ?? ""}>
        {messages.map((message) => (
          <MessageBubble key={message.id} message={message} />
        ))}
      </MessageLog>
      <form action={submit} {...props(uiStyles.messageComposer)} aria-label="Send a message">
        <input type="hidden" name="threadId" value={thread.id} />
        <textarea
          {...props(styles.messageTextarea)}
          name="body"
          aria-label="Message body"
          aria-describedby="body-error"
          aria-invalid={fieldError(state, "body") != null}
          value={body}
          onChange={(event) => {
            setBody(event.currentTarget.value);
            setRequestId((current) => current || crypto.randomUUID());
          }}
          placeholder={`Write to ${thread.name.split(" ")[0]}…`}
          required
          maxLength={MAX_MESSAGE_LENGTH}
          disabled={pending}
          rows={2}
        />
        <FieldError state={state} name="body" />
        <div {...props(uiStyles.messageComposerFooter)}>
          <small {...props(styles.messageNote)}>Only the people in this conversation can read it.</small>
          <SubmitButton
            pendingLabel="Sending…"
            disabled={body.trim().length === 0}
            xstyle={styles.messageSubmit}
          >
            Send
          </SubmitButton>
        </div>
        <FormStatus state={state} />
      </form>
    </section>
  );
}

/** Render one message with direction relative to the authenticated participant. */

export component MessageBubble(message: Message) {
  const pending = message.id.startsWith("pending-");

  return (
    <article
      {...props(
        uiStyles.messageBubble,
        match (message.author) {
          "me" => uiStyles.messageMine,
          _    => null,
        },
        pending && styles.optimistic,
      )}
    >
      <p
        {...props(
          styles.messageText,
          match (message.author) {
            "me" => styles.messageTextMine,
            _    => null,
          },
        )}
      >
        {message.body}
      </p>
      <time
        {...props(
          styles.messageTime,
          match (message.author) {
            "me" => styles.messageTimeMine,
            _    => null,
          },
        )}
        dateTime={message.sentAt}
      >
        {
          match (pending) {
            true  => "Sending…",
            false => `${displayTime(message.sentAt)} UTC`,
          }
        }
      </time>
    </article>
  );
}

/** Accept typed message children and synchronize scroll position with the latest message. */

export component MessageLog(lastId: string, children: renders* MessageBubble) {
  const viewport  = useRef<HTMLDivElement | null>(null);
  const following = useRef(true);
  // Scroll is a DOM side effect. Reading older messages opts out until the reader returns below.

  useEffect(() => {
    const node = viewport.current;
    if (node != null && following.current) node.scrollTop = node.scrollHeight;
  }, [lastId]);

  return (
    <div
      ref={viewport}
      onScroll={(event) => {
        const node = event.currentTarget;
        following.current = node.scrollHeight - node.clientHeight - node.scrollTop < 48;
      }}
      {...props(uiStyles.messageList)}
      role="log"
      aria-label="Messages"
      aria-live="polite"
      aria-relevant="additions text"
    >
      {children}
    </div>
  );
}

const styles = stylex.create({
  conversationTitle: {
    fontSize  : "14px",
    fontWeight: "600",
  },
  conversationMeta: {
    fontSize : "11px",
    color    : "var(--muted)",
    marginTop: "4px",
  },
  conversationPrivate: {
    marginLeft: "auto",
    color     : "#989898",
  },
  optimistic: {
    opacity: "0.6",
  },
  messageText: {
    borderRadius : "0 10px 10px 10px",
    fontSize     : "13px",
    lineHeight   : "1.7",
    whiteSpace   : "pre-wrap",
    overflowWrap : "anywhere",
    color        : "#4e4e4e",
    background   : "#ebebeba6",
    paddingTop   : "13px",
    paddingRight : "16px",
    paddingBottom: "13px",
    paddingLeft  : "16px",
  },
  messageTextMine: {
    borderRadius: "10px 0 10px 10px",
    background  : "#e7e7e7a6",
  },
  messageTime: {
    fontSize : "10px",
    color    : "var(--muted)",
    display  : "block",
    marginTop: "7px",
  },
  messageTimeMine: {
    textAlign: "right",
  },
  messageTextarea: {
    width        : "100%",
    border       : "0",
    background   : "transparent",
    fontSize     : { default: "13px", "@media (max-width: 760px)": "16px" },
    lineHeight   : "1.7",
    minHeight    : "52px",
    maxHeight    : "180px",
    paddingTop   : "6px",
    paddingRight : "0",
    paddingBottom: "6px",
    paddingLeft  : "0",
  },
  messageNote: {
    fontSize  : "10px",
    color     : "var(--muted)",
    lineHeight: "1.5",
  },
  messageSubmit: {
    fontSize : "12px",
    minHeight: "33px",
  },
});
