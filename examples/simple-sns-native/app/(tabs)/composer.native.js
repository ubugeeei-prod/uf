// @flow

import { useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native";
import { stylex } from "@uniflowed/stylex/native";

import type { Topic, TopicFilter, User } from "../_shared/social.js";

import { TOP_ALIGNED, styles } from "../_shared/commonplace.stylex.js";
import { MAX_POST_LENGTH, TOPICS, topicLabel, useSocial } from "../_shared/social.js";
import { Avatar, Button } from "../_shared/ui.native.js";

/**
 * A note goes to the channel the feed is showing unless the person picks another, which is what
 * the web composer's `<select>` defaults to. The draft survives a change of channel.
 */

export component Composer(viewer: User, topic: TopicFilter) {
  const { publish } = useSocial();
  const [body, setBody] = useState("");
  const [picked, setPicked] = useState<Topic | null>(null);
  const [error, setError] = useState("");
  const channel = picked ?? (topic === "all" ? "community" : topic);

  return (
    <View accessibilityLabel="Publish a note" {...stylex.props(styles.rule, local.composer)}>
      <View {...stylex.props(local.body)}>
        <Avatar user={viewer} />
        <TextInput
          accessibilityLabel="Post body"
          placeholder={`What are you working on, ${viewer.name.split(" ")[0]}?`}
          placeholderTextColor="#8a8a8a"
          multiline
          maxLength={MAX_POST_LENGTH}
          value={body}
          onChangeText={(text) => {
            setBody(text);
            setError("");
          }}
          style={[stylex.props(local.input).style, TOP_ALIGNED]}
        />
      </View>
      <View
        accessibilityRole="radiogroup"
        accessibilityLabel="Post channel"
        {...stylex.props(local.channels)}
      >
        {TOPICS.map((value) => {
          const selected = value === channel;

          return (
            <Pressable
              key={value}
              accessibilityRole="radio"
              accessibilityLabel={`Post to ${topicLabel(value)}`}
              accessibilityState={{ selected }}
              onPress={() => setPicked(value)}
              {...stylex.props(local.chip, selected && local.chipSelected)}
            >
              <Text {...stylex.props(local.chipLabel, selected && local.chipLabelSelected)}>
                {topicLabel(value)}
              </Text>
            </Pressable>
          );
        })}
      </View>
      <View {...stylex.props(styles.spread, local.footer)}>
        <Text {...stylex.props(local.counter)}>
          {body.length}/{MAX_POST_LENGTH}
        </Text>
        <Button
          disabled={body.trim() === ""}
          onPress={() => {
            const outcome = publish(body, channel);
            if (outcome.ok) setBody("");
            else setError(outcome.message);
          }}
        >
          Publish note
        </Button>
      </View>
      {error !== "" ? (
        <Text accessibilityRole="alert" {...stylex.props(styles.alert)}>
          {error}
        </Text>
      ) : null}
    </View>
  );
}

const local = stylex.create({
  composer: { paddingTop: 20, paddingBottom: 6 },
  body: { flexDirection: "row", alignItems: "flex-start", gap: 14 },
  input: {
    flex: 1,
    minWidth: 0,
    fontSize: 14,
    lineHeight: 22,
    minHeight: 66,
    color: "#242424",
  },
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
