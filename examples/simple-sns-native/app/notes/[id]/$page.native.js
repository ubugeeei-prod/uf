// @flow

import { Pressable, ScrollView, Text } from "react-native";
import { useNativeRouter, useParams } from "@uniflowed/router/native-navigation";
import { stylex } from "@uniflowed/stylex/native";

import { AsyncRegion } from "../../_shared/async-region.native.js";
import { useResource } from "../../_shared/client.js";
import { styles } from "../../_shared/commonplace.stylex.js";
import { EmptyState, LoadingState, Screen } from "../../_shared/ui.native.js";
import { PostCard } from "../../(tabs)/post-card.native.js";

/**
 * One note on its own screen, pushed over the tabs. It reads the note itself rather than being
 * handed the feed's copy, so a reaction made here refreshes both and neither can disagree.
 */

export component Page() {
  const { id } = useParams();
  const wanted = typeof id === "string" ? id : "";
  const router = useNativeRouter();
  const { resource, retry } = useResource(`note:${wanted}`, (service) =>
    Promise.all([service.session(), service.post(wanted)]).then(([session, post]) => ({
      session,
      post,
    })),
  );

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
        <AsyncRegion
          resource={resource}
          label="this note"
          retry={retry}
          pending={<LoadingState kind="feed" />}
        >
          {({ session, post }) =>
            post == null ? (
              <EmptyState title="This note is no longer here">It may have been removed.</EmptyState>
            ) : (
              <PostCard post={post} signedIn={session.kind === "authenticated"} linked={false} />
            )}
        </AsyncRegion>
      </ScrollView>
    </Screen>
  );
}

const local = stylex.create({
  back: { fontSize: 12, fontWeight: "600", color: "#242424" },
});
