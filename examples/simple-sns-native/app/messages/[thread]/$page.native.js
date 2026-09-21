// @flow

import { useState } from "react";
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

import { styles } from "../../_shared/commonplace.stylex.js";
import { MAX_MESSAGE_LENGTH, displayTime, useSocial } from "../../_shared/social.js";
import { Avatar, Button, EmptyState, Screen, SignInPrompt } from "../../_shared/ui.native.js";

/** One private conversation: the log, and a composer that stays above the keyboard. */

export component Page() {
  const { thread: id } = useParams();
  const { viewer, threads, send } = useSocial();
  const router = useNativeRouter();
  const [body, setBody] = useState("");
  const [error, setError] = useState("");
  const thread = threads.find((item) => item.id === id);

  return (
    <Screen edges={["top", "bottom"]}>
      <KeyboardAvoidingView
        behavior={Platform.OS === "ios" ? "padding" : undefined}
        {...stylex.props(local.fill)}
      >
        <View {...stylex.props(styles.rule, local.heading)}>
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="Back to inbox"
            hitSlop={10}
            onPress={() => router.back()}
          >
            <Text {...stylex.props(local.back)}>←</Text>
          </Pressable>
          {thread != null ? (
            <>
              <Avatar user={thread.participant} small />
              <Text {...stylex.props(styles.sectionTitle)}>{thread.participant.name}</Text>
            </>
          ) : null}
        </View>
        {viewer == null ? (
          <SignInPrompt title="Sign in to read your messages" />
        ) : thread == null ? (
          <EmptyState title="Conversation not found">It may belong to another account.</EmptyState>
        ) : (
          <>
            <ScrollView
              accessibilityLabel="Messages"
              keyboardShouldPersistTaps="handled"
              contentContainerStyle={stylex.props(local.log).style}
            >
              {thread.messages.map((message) => {
                const mine = message.author === "me";

                return (
                  <View key={message.id} {...stylex.props(local.message, mine && local.mine)}>
                    <View {...stylex.props(local.bubble, mine && local.bubbleMine)}>
                      <Text {...stylex.props(local.text, mine && local.textMine)}>
                        {message.body}
                      </Text>
                    </View>
                    <Text {...stylex.props(local.time)}>{displayTime(message.sentAt)}</Text>
                  </View>
                );
              })}
            </ScrollView>
            <View {...stylex.props(local.composer)}>
              <TextInput
                accessibilityLabel="Message"
                placeholder={`Message ${thread.participant.name.split(" ")[0]}`}
                placeholderTextColor="#8a8a8a"
                multiline
                maxLength={MAX_MESSAGE_LENGTH}
                value={body}
                onChangeText={(text) => {
                  setBody(text);
                  setError("");
                }}
                {...stylex.props(local.input)}
              />
              <Button
                label="Send message"
                disabled={body.trim() === ""}
                onPress={() => {
                  const outcome = send(thread.id, body);
                  if (outcome.ok) setBody("");
                  else setError(outcome.message);
                }}
              >
                Send
              </Button>
            </View>
            {error !== "" ? (
              <Text accessibilityRole="alert" {...stylex.props(styles.alert, local.error)}>
                {error}
              </Text>
            ) : null}
          </>
        )}
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
  error: { paddingLeft: 20, paddingRight: 20, paddingBottom: 8 },
});
