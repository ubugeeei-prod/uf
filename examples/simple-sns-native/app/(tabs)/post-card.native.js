// @flow

import { Pressable, Text, View } from "react-native";
import { Link } from "@uniflowed/router/native-navigation";
import { stylex } from "@uniflowed/stylex/native";

import { route } from "../../router";

import type { Post } from "../_shared/social.js";

import { styles } from "../_shared/commonplace.stylex.js";
import { displayDate, useSocial } from "../_shared/social.js";
import { Avatar, ChannelBadge } from "../_shared/ui.native.js";

/** One note as the web renders it: a row under a hairline, not a card. */

export component PostCard(post: Post, linked: boolean = true) {
  const { viewer, appreciate } = useSocial();
  const count = String(post.likes);

  return (
    <View {...stylex.props(styles.rule, local.post)}>
      <Avatar user={post.author} />
      <View {...stylex.props(styles.grow)}>
        <View {...stylex.props(local.header)}>
          <Text {...stylex.props(local.name)}>{post.author.name}</Text>
          <Text {...stylex.props(local.meta)}>@{post.author.handle}</Text>
          <Text {...stylex.props(local.meta, local.date)}>{displayDate(post.createdAt)}</Text>
        </View>
        {linked ? (
          <Link
            href={route("/notes/:id", { id: post.id })}
            accessibilityLabel={`Open note by ${post.author.name}`}
          >
            <Text {...stylex.props(local.body)}>{post.body}</Text>
          </Link>
        ) : (
          <Text {...stylex.props(local.body)}>{post.body}</Text>
        )}
        <View {...stylex.props(styles.spread)}>
          <ChannelBadge topic={post.topic} />
          {viewer != null ? (
            <Pressable
              accessibilityRole="button"
              accessibilityLabel={`${post.liked ? "Remove appreciation" : "Appreciate"} · ${count}`}
              accessibilityState={{ selected: post.liked }}
              hitSlop={8}
              onPress={() => appreciate(post.id)}
              {...stylex.props(local.reaction)}
            >
              <Text {...stylex.props(local.heart, post.liked && local.heartOn)}>
                {post.liked ? "♥" : "♡"}
              </Text>
              <Text {...stylex.props(local.count, post.liked && local.heartOn)}>{count}</Text>
            </Pressable>
          ) : (
            <Link href={route("/join")} accessibilityLabel={`Sign in to appreciate · ${count}`}>
              <View {...stylex.props(local.reaction)}>
                <Text {...stylex.props(local.heart)}>♡</Text>
                <Text {...stylex.props(local.count)}>{count}</Text>
              </View>
            </Link>
          )}
        </View>
      </View>
    </View>
  );
}

const local = stylex.create({
  post: {
    flexDirection: "row",
    alignItems: "flex-start",
    gap: 14,
    paddingTop: 24,
    paddingBottom: 18,
  },
  header: { flexDirection: "row", alignItems: "center", flexWrap: "wrap", gap: 8 },
  name: { fontSize: 13, fontWeight: "600", color: "#242424" },
  meta: { fontSize: 11, color: "#707070" },
  date: { marginLeft: "auto" },
  body: { fontSize: 14, lineHeight: 24, color: "#505050", marginTop: 9, marginBottom: 12 },
  reaction: { flexDirection: "row", alignItems: "center", gap: 7, minHeight: 32, paddingLeft: 7 },
  heart: { fontSize: 15, color: "#818181" },
  heartOn: { color: "#252525" },
  count: { fontSize: 11, color: "#818181" },
});
