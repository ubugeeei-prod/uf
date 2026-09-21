// @flow

import { Pressable, ScrollView, Text, View } from "react-native";
import { stylex } from "@uniflowed/stylex/native";

import type { TopicFilter } from "../_shared/social.js";

import { TOPICS, topicLabel } from "../_shared/social.js";

component ChannelTab(channel: TopicFilter, selected: boolean, onPress: () => void) {
  const label = match (channel) {
    "all" => "All notes",
    "design" | "release" | "runtime" | "community" as const topic => topicLabel(topic),
  };

  return (
    <Pressable
      accessibilityRole="tab"
      accessibilityLabel={label}
      accessibilityState={{ selected }}
      onPress={onPress}
      {...stylex.props(local.tab, selected && local.tabSelected)}
    >
      <Text {...stylex.props(local.label, selected && local.labelSelected)}>{label}</Text>
    </Pressable>
  );
}

/** The strip takes tabs and nothing else, so what scrolls sideways is always a channel. */

component TabStrip(children: renders* ChannelTab) {
  return (
    <View {...stylex.props(local.bar)}>
      <ScrollView horizontal showsHorizontalScrollIndicator={false} accessibilityRole="tablist">
        {children}
      </ScrollView>
    </View>
  );
}

/** The feed's channels, as the underlined tabs the web page scrolls sideways. */

export component ChannelTabs(topic: TopicFilter, onChange: (TopicFilter) => void) renders TabStrip {
  const channels: $ReadOnlyArray<TopicFilter> = ["all", ...TOPICS];

  return (
    <TabStrip>
      {channels.map((channel) => (
        <ChannelTab
          key={channel}
          channel={channel}
          selected={channel === topic}
          onPress={() => onChange(channel)}
        />
      ))}
    </TabStrip>
  );
}

const local = stylex.create({
  bar: { marginTop: 22, borderBottomWidth: 1, borderBottomColor: "#dcdcdc" },
  tab: {
    marginRight: 24,
    paddingTop: 14,
    paddingBottom: 12,
    borderBottomWidth: 2,
    borderBottomColor: "transparent",
  },
  tabSelected: { borderBottomColor: "#202020" },
  label: { fontSize: 12, color: "#707070" },
  labelSelected: { color: "#242424", fontWeight: "600" },
});
