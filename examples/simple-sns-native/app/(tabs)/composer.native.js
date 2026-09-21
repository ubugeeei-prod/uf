// @flow

import { startTransition, useActionState, useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native";
import { stylex } from "@uniflowed/stylex/native";

import type { FormState, Post, Topic, TopicFilter, User } from "../_shared/social.js";

import { useSocial } from "../_shared/client.js";
import { TOP_ALIGNED, styles } from "../_shared/commonplace.stylex.js";
import { IDLE, MAX_POST_LENGTH, TOPICS, defaultTopic, topicLabel } from "../_shared/social.js";
import { Avatar, Button, FormStatus } from "../_shared/ui.native.js";

type Draft = {| readonly body: string, readonly topic: Topic |};

/** One channel of the composer's picker. */

component ChannelChip(topic: Topic, selected: boolean, onPress: () => void) {
  return (
    <Pressable
      accessibilityRole="radio"
      accessibilityLabel={`Post to ${topicLabel(topic)}`}
      accessibilityState={{ selected }}
      onPress={onPress}
      {...stylex.props(local.chip, selected && local.chipSelected)}
    >
      <Text {...stylex.props(local.chipLabel, selected && local.chipLabelSelected)}>
        {topicLabel(topic)}
      </Text>
    </Pressable>
  );
}

component ChannelPicker(children: renders* ChannelChip) {
  return (
    <View
      accessibilityRole="radiogroup"
      accessibilityLabel="Post channel"
      {...stylex.props(local.channels)}
    >
      {children}
    </View>
  );
}

/**
 * Publishing is an Action. It puts the note in the list at once through `onPublishing`, waits for
 * the service, and on success refreshes every read and clears the draft in the same transition —
 * so the draft is still there if publishing failed, and the list never shows the note twice.
 *
 * A note goes to the channel the feed is showing unless the person picks another, which is what
 * the web composer's `<select>` defaults to.
 */

export component Composer(viewer: User, topic: TopicFilter, onPublishing: (Post) => void) {
  const { service, refresh } = useSocial();
  const [body, setBody] = useState("");
  const [picked, setPicked] = useState<Topic | null>(null);
  const channel = picked ?? defaultTopic(topic);
  const [state, submit, pending] = useActionState<FormState<Post>, Draft>(
    async (_previous: FormState<Post>, draft: Draft): Promise<FormState<Post>> => {
      onPublishing({
        id: "pending-note",
        author: viewer,
        body: draft.body.trim(),
        topic: draft.topic,
        createdAt: new Date().toISOString(),
        likes: 0,
        liked: false,
      });
      const result = await service.publish(draft.body, draft.topic);
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
    <View accessibilityLabel="Publish a note" {...stylex.props(styles.rule, local.composer)}>
      <View {...stylex.props(local.body)}>
        <Avatar user={viewer} />
        <TextInput
          accessibilityLabel="Post body"
          placeholder={`What are you working on, ${viewer.name.split(" ")[0]}?`}
          placeholderTextColor="#8a8a8a"
          multiline
          editable={!pending}
          maxLength={MAX_POST_LENGTH}
          value={body}
          onChangeText={setBody}
          style={[stylex.props(local.input).style, TOP_ALIGNED]}
        />
      </View>
      <ChannelPicker>
        {TOPICS.map((value) => (
          <ChannelChip
            key={value}
            topic={value}
            selected={value === channel}
            onPress={() => setPicked(value)}
          />
        ))}
      </ChannelPicker>
      <View {...stylex.props(styles.spread, local.footer)}>
        <Text {...stylex.props(local.counter)}>
          {body.length}/{MAX_POST_LENGTH}
        </Text>
        <Button
          disabled={body.trim() === ""}
          pending={pending ? "Publishing…" : null}
          label="Publish note"
          onPress={() => startTransition(() => submit({ body, topic: channel }))}
        >
          Publish note
        </Button>
      </View>
      <FormStatus state={state} quiet />
    </View>
  );
}

const local = stylex.create({
  composer: { paddingTop: 20, paddingBottom: 6 },
  body: { flexDirection: "row", alignItems: "flex-start", gap: 14 },
  input: { flex: 1, minWidth: 0, fontSize: 14, lineHeight: 22, minHeight: 66, color: "#242424" },
  channels: { flexDirection: "row", flexWrap: "wrap", gap: 6, paddingTop: 10 },
  chip: {
    paddingLeft: 10,
    paddingRight: 10,
    paddingTop: 6,
    paddingBottom: 6,
    borderRadius: 6,
    borderWidth: 1,
    borderColor: "#e3e3e3",
    backgroundColor: "#ffffff",
  },
  chipSelected: { borderColor: "#202020" },
  chipLabel: { fontSize: 11, color: "#707070" },
  chipLabelSelected: { color: "#242424", fontWeight: "600" },
  footer: { paddingTop: 12, paddingBottom: 12 },
  counter: { fontSize: 11, color: "#707070" },
});
