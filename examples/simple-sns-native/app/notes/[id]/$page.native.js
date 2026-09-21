// @flow

import { Pressable, ScrollView, Text } from "react-native";
import { useNativeRouter, useParams } from "@uniflowed/router/native-navigation";
import { stylex } from "@uniflowed/stylex/native";

import { styles } from "../../_shared/commonplace.stylex.js";
import { useSocial } from "../../_shared/social.js";
import { EmptyState, Screen } from "../../_shared/ui.native.js";
import { PostCard } from "../../(tabs)/post-card.native.js";

/** One note on its own screen, pushed over the tabs; its reaction is the feed's. */

export component Page() {
  const { id } = useParams();
  const { post } = useSocial();
  const router = useNativeRouter();
  const note = typeof id === "string" ? post(id) : null;

  return (
    <Screen edges={["top", "bottom"]}>
      <ScrollView contentContainerStyle={stylex.props(styles.content).style}>
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="Back to feed"
          hitSlop={10}
          onPress={() => router.back()}
        >
          <Text {...stylex.props(local.back)}>← Back to feed</Text>
        </Pressable>
        {note != null ? (
          <PostCard post={note} linked={false} />
        ) : (
          <EmptyState title="This note is no longer here">It may have been removed.</EmptyState>
        )}
      </ScrollView>
    </Screen>
  );
}

const local = stylex.create({
  back: { fontSize: 12, fontWeight: "600", color: "#242424" },
});
