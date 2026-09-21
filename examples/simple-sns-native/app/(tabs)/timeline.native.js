// @flow

import { useOptimistic } from "react";
import { Text, View } from "react-native";
import { stylex } from "@uniflowed/stylex/native";

import { route } from "../../router";

import type { Post, Session, TopicFilter } from "../_shared/social.js";

import { styles } from "../_shared/commonplace.stylex.js";
import { ActionLink, EmptyState } from "../_shared/ui.native.js";
import { Composer } from "./composer.native.js";
import { PostCard } from "./post-card.native.js";

/** The list accepts cards — a fragment or an array of them — and not arbitrary children. */

component PostList(children: renders* PostCard) {
  return <View accessibilityLabel="Timeline posts">{children}</View>;
}

/** What a guest sees where the composer would be. */

component Invitation() {
  return (
    <View {...stylex.props(styles.spread, styles.rule, local.invitation)}>
      <View {...stylex.props(styles.grow)}>
        <Text {...stylex.props(styles.sectionTitle)}>What are you working on?</Text>
        <Text {...stylex.props(styles.hint)}>Sign in to post an update or ask a question.</Text>
      </View>
      <ActionLink href={route("/join")}>Create account</ActionLink>
    </View>
  );
}

/**
 * The notes the service returned, and in front of them the one being published: the composer adds
 * it optimistically, inside its Action, and React drops it again when that Action has settled —
 * replaced by the refreshed list if it succeeded, gone if it did not.
 */

export component Timeline(
  session: Session,
  posts: $ReadOnlyArray<Post>,
  topic: TopicFilter,
  stale: boolean,
) {
  const [entries, addOptimistic] = useOptimistic<$ReadOnlyArray<Post>, Post>(
    posts,
    (current, publishing) => [publishing, ...current],
  );

  return (
    <View {...stylex.props(stale && local.stale)}>
      {
        match (session) {
          {kind: "guest"} => <Invitation />,
          {kind: "authenticated", user: const user} =>
            <Composer viewer={user} topic={topic} onPublishing={addOptimistic} />,
        }
      }
      {entries.length > 0 ? (
        <PostList>
          {entries.map((post) => (
            <PostCard key={post.id} post={post} signedIn={session.kind === "authenticated"} />
          ))}
        </PostList>
      ) : (
        <EmptyState title="No notes here yet">Try another channel or search.</EmptyState>
      )}
    </View>
  );
}

const local = stylex.create({
  invitation: { paddingTop: 22, paddingBottom: 22 },
  stale: { opacity: 0.55 },
});
