// @flow

import { Pressable, ScrollView, Text, View } from "react-native";
import { stylex } from "@uniflowed/stylex/native";

import type { TopicFilter } from "../_shared/social.js";

import { TOPICS, topicLabel } from "../_shared/social.js";

/** The feed's channels, as the underlined tabs the web page scrolls sideways. */

export component ChannelTabs(topic: TopicFilter, onChange: (TopicFilter) => void) {
  const channels: $ReadOnlyArray<TopicFilter> = ["all", ...TOPICS];

  return (
    <View {...stylex.props(local.bar)}>
      <ScrollView horizontal showsHorizontalScrollIndicator={false} accessibilityRole="tablist">
        {channels.map((channel) => {
          const selected = channel === topic;
          const label = channel === "all" ? "All notes" : topicLabel(channel);

          return (
            <Pressable
              key={channel}
              accessibilityRole="tab"
              accessibilityLabel={label}
              accessibilityState={{ selected }}
              onPress={() => onChange(channel)}
              {...stylex.props(local.tab, selected && local.tabSelected)}
            >
              <Text {...stylex.props(local.label, selected && local.labelSelected)}>{label}</Text>
            </Pressable>
          );
        })}
      </ScrollView>
    </View>
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
