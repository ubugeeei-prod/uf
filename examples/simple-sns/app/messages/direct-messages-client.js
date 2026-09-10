"use client";
// @flow

import * as React from "@uniflowed/react";
import { useActionState, useMemo, useOptimistic, useState } from "@uniflowed/react";
import { useFormStatus } from "react-dom";
import { props, stylex } from "@uniflowed/stylex";

import { sendMessage } from "../social-actions.js";
import {
  MAX_MESSAGE_LENGTH,
  deliveryLabel,
  messagePreview,
  optimisticMessage,
  type FormState,
  type Message,
  type MessageThread,
} from "../social-model.js";

const EMPTY_MESSAGE_STATE: FormState<Message> = { status: "idle", message: "" };

component SendButton() {
  const { pending } = useFormStatus();
  return (
    <button type="submit" disabled={pending} {...props(styles.button)}>
      {pending ? "Sending" : "Send"}
    </button>
  );
}

component ThreadButton(thread: MessageThread, selected: boolean, onSelect: (string) => void) {
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={() => onSelect(thread.id)}
      {...props(styles.thread, selected && styles.threadSelected)}
    >
      <span {...props(styles.threadTop)}>
        <strong>{thread.name}</strong>
        {thread.unread > 0 ? <span {...props(styles.unread)}>{thread.unread}</span> : null}
      </span>
      <span {...props(styles.preview)}>{messagePreview(thread)}</span>
    </button>
  );
}

component Bubble(message: Message) {
  const mine = message.author === "me";
  return (
    <div {...props(styles.bubbleRow, mine && styles.bubbleRowMine)}>
      <article {...props(styles.bubble, mine && styles.bubbleMine)}>
        <p {...props(styles.messageBody)}>{message.body}</p>
        <footer {...props(styles.messageMeta)}>
          {message.sentAt.slice(11, 16)} · {deliveryLabel(message.delivery)}
        </footer>
      </article>
    </div>
  );
}

export component DirectMessagesClient(
  threads: $ReadOnlyArray<MessageThread>,
  initialMessages: $ReadOnlyArray<Message>,
) {
  const firstThread = threads[0]?.id ?? "";
  const [selectedThreadId, setSelectedThreadId] = useState<string>(firstThread);
  const [state, action] = useActionState<FormState<Message>, FormData>(
    sendMessage,
    EMPTY_MESSAGE_STATE,
  );
  const [committedMessages, setCommittedMessages] = useState<Array<Message>>(() =>
    Array.from(initialMessages),
  );
  const [messages, addOptimisticMessage] = useOptimistic<Array<Message>, Message>(
    committedMessages,
    (current, draft) => [...current.filter((message) => message.id !== draft.id), draft],
  );
  React.useEffect(() => {
    const message = state.value;
    if (state.status !== "success" || message == null) {
      return;
    }
    setCommittedMessages((current) =>
      current.some((item) => item.id === message.id) ? current : [...current, message],
    );
  }, [state, setCommittedMessages]);
  const selectedMessages = useMemo(
    () => messages.filter((message) => message.threadId === selectedThreadId),
    [messages, selectedThreadId],
  );

  return (
    <div {...props(styles.layout)}>
      <section {...props(styles.threads)} aria-label="Message threads">
        {threads.map((thread) => (
          <ThreadButton
            key={thread.id}
            thread={thread}
            selected={thread.id === selectedThreadId}
            onSelect={setSelectedThreadId}
          />
        ))}
      </section>
      <section {...props(styles.chat)} aria-label="Messages">
        <div {...props(styles.messages)}>
          {selectedMessages.map((message) => (
            <Bubble key={message.id} message={message} />
          ))}
        </div>
        <form
          suppressHydrationWarning
          action={(formData) => {
            const body = String(formData.get("body") ?? "");
            if (selectedThreadId.length > 0 && body.trim().length > 0) {
              addOptimisticMessage(optimisticMessage(selectedThreadId, body, new Date()));
            }
            formData.set("threadId", selectedThreadId);
            action(formData);
          }}
          {...props(styles.composer)}
        >
          <input type="hidden" name="threadId" value={selectedThreadId} />
          <textarea
            name="body"
            aria-label="Direct message body"
            rows={3}
            maxLength={MAX_MESSAGE_LENGTH}
            required
            placeholder="Write a direct message"
            {...props(styles.textarea)}
          />
          <div {...props(styles.formFooter)}>
            <span {...props(styles.formMessage, state.status === "error" && styles.error)}>
              {state.message}
            </span>
            <SendButton />
          </div>
        </form>
      </section>
    </div>
  );
}

const styles = stylex.create({
  layout: {
    display: "grid",
    gap: 14,
    gridTemplateColumns: {
      default: "1fr",
      "@media (min-width: 860px)": "280px minmax(0, 1fr)",
    },
  },
  threads: {
    display: {
      default: "flex",
      "@media (min-width: 860px)": "grid",
    },
    gap: 8,
    overflowX: {
      default: "auto",
      "@media (min-width: 860px)": "visible",
    },
  },
  thread: {
    backgroundColor: "#ffffff",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: "#344054",
    cursor: "pointer",
    display: "grid",
    gap: 6,
    minWidth: {
      default: 220,
      "@media (min-width: 860px)": "auto",
    },
    padding: 12,
    textAlign: "left",
  },
  threadSelected: {
    backgroundColor: "#111827",
    borderColor: "#111827",
    color: "#ffffff",
  },
  threadTop: {
    alignItems: "center",
    display: "flex",
    gap: 8,
    justifyContent: "space-between",
  },
  unread: {
    backgroundColor: "#f97316",
    borderRadius: 8,
    color: "#ffffff",
    fontSize: 12,
    fontWeight: 800,
    paddingBlock: 2,
    paddingInline: 7,
  },
  preview: {
    fontSize: 13,
    lineHeight: 1.35,
    opacity: 0.82,
  },
  chat: {
    backgroundColor: "#ffffff",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    display: "grid",
    gap: 12,
    minHeight: {
      default: 500,
      "@media (min-width: 860px)": 560,
    },
    padding: {
      default: 12,
      "@media (min-width: 760px)": 16,
    },
  },
  messages: {
    alignContent: "start",
    display: "grid",
    gap: 10,
  },
  bubbleRow: {
    display: "flex",
    justifyContent: "flex-start",
  },
  bubbleRowMine: {
    justifyContent: "flex-end",
  },
  bubble: {
    backgroundColor: "#f1f5f9",
    borderRadius: 8,
    color: "#0f172a",
    maxWidth: 560,
    paddingBlock: 10,
    paddingInline: 12,
  },
  bubbleMine: {
    backgroundColor: "#dbeafe",
    borderBottomRightRadius: 2,
  },
  messageBody: {
    color: "#111827",
    lineHeight: 1.45,
    marginBlock: 0,
  },
  messageMeta: {
    color: "#475467",
    fontSize: 12,
    marginTop: 6,
  },
  composer: {
    alignSelf: "end",
    borderTopColor: "#eaecf0",
    borderTopStyle: "solid",
    borderTopWidth: 1,
    display: "grid",
    gap: 10,
    paddingTop: 12,
  },
  textarea: {
    backgroundColor: "#f8fafc",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: { default: "#0f172a", "::placeholder": "#475467" },
    font: "inherit",
    minHeight: 92,
    padding: 12,
    resize: "vertical",
  },
  formFooter: {
    alignItems: {
      default: "stretch",
      "@media (min-width: 560px)": "center",
    },
    display: {
      default: "grid",
      "@media (min-width: 560px)": "flex",
    },
    gap: 12,
    justifyContent: "space-between",
  },
  formMessage: {
    color: "#475467",
    fontSize: 14,
  },
  error: {
    color: "#b42318",
  },
  button: {
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
    paddingInline: 16,
  },
});
