// @flow

import * as React from "react";
import { startTransition, useActionState, useOptimistic, useState } from "react";
import {
  KeyboardAvoidingView,
  Platform,
  Pressable,
  ScrollView,
  Text,
  TextInput,
  View,
} from "react-native";
import { useNativeRouter, useParams } from "@uniflowed/router/native-navigation";
import { stylex } from "@uniflowed/stylex/native";

import type { FormState, Message, Thread, User } from "../../_shared/social.js";

import { AsyncRegion } from "../../_shared/async-region.native.js";
import { useResource, useSocial } from "../../_shared/client.js";
import { styles } from "../../_shared/commonplace.stylex.js";
import { IDLE, MAX_MESSAGE_LENGTH, displayTime } from "../../_shared/social.js";
import {
  Avatar,
  Button,
  EmptyState,
  FormStatus,
  LoadingState,
  Screen,
  SignInPrompt,
} from "../../_shared/ui.native.js";

component MessageBubble(message: Message) {
  const mine = message.author === "me";
  const sending = message.id.startsWith("pending-");

  return (
    <View {...stylex.props(local.message, mine && local.mine, sending && local.sending)}>
      <View {...stylex.props(local.bubble, mine && local.bubbleMine)}>
        <Text {...stylex.props(local.text, mine && local.textMine)}>{message.body}</Text>
      </View>
      <Text {...stylex.props(local.time)}>
        {sending ? "Sending…" : displayTime(message.sentAt)}
      </Text>
    </View>
  );
}

component MessageLog(children: renders* MessageBubble) {
  return (
    <ScrollView
      accessibilityLabel="Messages"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={stylex.props(local.log).style}
    >
      {children}
    </ScrollView>
  );
}

component Heading(participant: User | null) {
  const router = useNativeRouter();

  return (
    <View {...stylex.props(styles.rule, local.heading)}>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel="Back to inbox"
        hitSlop={10}
        onPress={() => router.back()}
      >
        <Text {...stylex.props(local.back)}>←</Text>
      </Pressable>
      {participant != null ? (
        <>
          <Avatar user={participant} small />
          <Text {...stylex.props(styles.sectionTitle)}>{participant.name}</Text>
        </>
      ) : null}
    </View>
  );
}

/**
 * Sending is an Action with an optimistic entry: the bubble is in the log, marked as sending,
 * before the service has answered, and the draft is cleared with the refresh that replaces it.
 * A refusal leaves the draft where it was and takes the bubble away.
 */

component Conversation(thread: Thread, messages: $ReadOnlyArray<Message>) {
  const { service, refresh } = useSocial();
  const [body, setBody] = useState("");
  const [entries, addOptimistic] = useOptimistic<$ReadOnlyArray<Message>, Message>(
    messages,
    (current, sending) => [...current, sending],
  );
  const [state, submit, pending] = useActionState<FormState<Message>, string>(
    async (_previous: FormState<Message>, text: string): Promise<FormState<Message>> => {
      addOptimistic({
        id: "pending-message",
        author: "me",
        body: text.trim(),
        sentAt: new Date().toISOString(),
      });
      const result = await service.send(thread.id, text);
      match (result) {
        {status: "success", ...} => {
          startTransition(() => setBody(""));
          refresh();
        }
        {status: "error", ...} => {}
      }

      return result;
    },
    IDLE,
  );

  return (
    <>
      <MessageLog>
        {entries.map((message) => (
          <MessageBubble key={message.id} message={message} />
        ))}
      </MessageLog>
      <View {...stylex.props(local.composer)}>
        <TextInput
          accessibilityLabel="Message"
          placeholder={`Message ${thread.participant.name.split(" ")[0]}`}
          placeholderTextColor="#8a8a8a"
          multiline
          editable={!pending}
          maxLength={MAX_MESSAGE_LENGTH}
          value={body}
          onChangeText={setBody}
          {...stylex.props(local.input)}
        />
        <Button
          label="Send message"
          disabled={body.trim() === ""}
          pending={pending ? "Sending…" : null}
          onPress={() => startTransition(() => submit(body))}
        >
          Send
        </Button>
      </View>
      <View {...stylex.props(local.status)}>
        <FormStatus state={state} quiet />
      </View>
    </>
  );
}

/** One private conversation: the log, and a composer that stays above the keyboard. */

export component Page() {
  const { thread: id } = useParams();
  const wanted = typeof id === "string" ? id : "";
  const { resource, retry } = useResource(`conversation:${wanted}`, (service) =>
    service.conversation(wanted),
  );

  return (
    <Screen edges={["top", "bottom"]}>
      <KeyboardAvoidingView
        behavior={Platform.OS === "ios" ? "padding" : undefined}
        {...stylex.props(local.fill)}
      >
        <AsyncRegion
          resource={resource}
          label="messages"
          retry={retry}
          pending={<LoadingState kind="conversation" />}
        >
          {(conversation) =>
            match (conversation) {
              {kind: "unauthenticated"} =>
                <>
                  <Heading participant={null} />
                  <SignInPrompt title="Sign in to read your messages" />
                </>,
              {kind: "missing"} =>
                <>
                  <Heading participant={null} />
                  <EmptyState title="Conversation not found">
                    It may belong to another account.
                  </EmptyState>
                </>,
              {kind: "ready", thread: const thread, messages: const messages} =>
                <>
                  <Heading participant={thread.participant} />
                  <Conversation key={thread.id} thread={thread} messages={messages} />
                </>,
            }}
        </AsyncRegion>
      </KeyboardAvoidingView>
    </Screen>
  );
}

const local = stylex.create({
  fill: { flex: 1 },
  heading: {
    flexDirection: "row",
    alignItems: "center",
    gap: 12,
    paddingLeft: 20,
    paddingRight: 20,
    paddingTop: 12,
    paddingBottom: 12,
  },
  back: { fontSize: 18, color: "#242424", paddingRight: 4 },
  log: { paddingLeft: 20, paddingRight: 20, paddingTop: 18, paddingBottom: 18, gap: 14 },
  message: { alignItems: "flex-start", gap: 4 },
  mine: { alignItems: "flex-end" },
  sending: { opacity: 0.6 },
  bubble: {
    maxWidth: "82%",
    paddingLeft: 13,
    paddingRight: 13,
    paddingTop: 9,
    paddingBottom: 9,
    borderRadius: 12,
    borderBottomLeftRadius: 3,
    borderWidth: 1,
    borderColor: "#e2e2e2",
    backgroundColor: "#ffffff",
  },
  bubbleMine: {
    borderBottomLeftRadius: 12,
    borderBottomRightRadius: 3,
    borderColor: "#202020",
    backgroundColor: "#202020",
  },
  text: { fontSize: 13, lineHeight: 20, color: "#242424" },
  textMine: { color: "#ffffff" },
  time: { fontSize: 10, color: "#868686" },
  composer: {
    flexDirection: "row",
    alignItems: "flex-end",
    gap: 10,
    paddingLeft: 20,
    paddingRight: 20,
    paddingTop: 10,
    paddingBottom: 10,
    borderTopWidth: 1,
    borderTopColor: "#dcdcdc",
  },
  input: {
    flex: 1,
    minWidth: 0,
    maxHeight: 120,
    borderWidth: 1,
    borderColor: "#d7d7d7",
    borderRadius: 6,
    paddingLeft: 12,
    paddingRight: 12,
    paddingTop: 10,
    paddingBottom: 10,
    fontSize: 13,
    lineHeight: 19,
    color: "#242424",
    backgroundColor: "#ffffff",
  },
  status: { paddingLeft: 20, paddingRight: 20 },
});
