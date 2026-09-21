// @flow

import { startTransition, useActionState, useOptimistic } from "react";
import { Pressable, Text, View } from "react-native";
import { Link } from "@uniflowed/router/native-navigation";
import { stylex } from "@uniflowed/stylex/native";

import { route } from "../../router";

import type { FormState, Post } from "../_shared/social.js";

import { useSocial } from "../_shared/client.js";
import { styles } from "../_shared/commonplace.stylex.js";
import { IDLE, displayDate } from "../_shared/social.js";
import { Avatar, ChannelBadge, FormStatus } from "../_shared/ui.native.js";

component Heart(liked: boolean, likes: number) {
  return (
    <View {...stylex.props(local.reaction)}>
      <Text {...stylex.props(local.heart, liked && local.heartOn)}>{liked ? "♥" : "♡"}</Text>
      <Text {...stylex.props(local.count, liked && local.heartOn)}>{String(likes)}</Text>
    </View>
  );
}

/**
 * Set a reaction at once and let the service confirm it. The heart shows the optimistic value
 * for as long as the Action runs; a success refreshes every screen that shows this note in the
 * same transition, and a failure leaves the committed value, so the heart goes back by itself.
 */

component Appreciation(post: Post, signedIn: boolean) {
  const { service, refresh } = useSocial();
  const [optimistic, change] = useOptimistic<Post, boolean>(post, (value, liked) => ({
    ...value,
    liked,
    likes: value.likes + (liked === value.liked ? 0 : liked ? 1 : -1),
  }));
  const [state, submit, pending] = useActionState<FormState<Post>, boolean>(
    async (_previous: FormState<Post>, liked: boolean): Promise<FormState<Post>> => {
      change(liked);
      const result = await service.appreciate(post.id, liked);
      match (result) {
        {status: "success", ...} => {
          refresh();
        }
        {status: "error", ...} => {}
      }

      return result;
    },
    IDLE,
  );

  return (
    <View {...stylex.props(local.appreciation)}>
      {signedIn ? (
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={`${optimistic.liked ? "Remove appreciation" : "Appreciate"} · ${optimistic.likes}`}
          accessibilityState={{ selected: optimistic.liked, busy: pending }}
          disabled={pending}
          hitSlop={8}
          onPress={() => startTransition(() => submit(!optimistic.liked))}
        >
          <Heart liked={optimistic.liked} likes={optimistic.likes} />
        </Pressable>
      ) : (
        <Link href={route("/join")} accessibilityLabel={`Sign in to appreciate · ${post.likes}`}>
          <Heart liked={false} likes={post.likes} />
        </Link>
      )}
      <FormStatus state={state} quiet />
    </View>
  );
}

/** One note as the web renders it: a row under a hairline, not a card. */

export component PostCard(post: Post, signedIn: boolean, linked: boolean = true) {
  const publishing = post.id.startsWith("pending-");

  return (
    <View
      accessibilityState={{ busy: publishing }}
      {...stylex.props(styles.rule, local.post, publishing && local.publishing)}
    >
      <Avatar user={post.author} />
      <View {...stylex.props(styles.grow)}>
        <View {...stylex.props(local.header)}>
          <Text {...stylex.props(local.name)}>{post.author.name}</Text>
          <Text {...stylex.props(local.meta)}>@{post.author.handle}</Text>
          <Text {...stylex.props(local.meta, local.date)}>
            {publishing ? "Publishing…" : displayDate(post.createdAt)}
          </Text>
        </View>
        {linked && !publishing ? (
          <Link
            href={route("/notes/:id", { id: post.id })}
            accessibilityLabel={`Open note by ${post.author.name}`}
          >
            <Text {...stylex.props(local.body)}>{post.body}</Text>
          </Link>
        ) : (
          <Text {...stylex.props(local.body)}>{post.body}</Text>
        )}
        <View {...stylex.props(local.footer)}>
          <ChannelBadge topic={post.topic} />
          {publishing ? null : <Appreciation post={post} signedIn={signedIn} />}
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
  publishing: { opacity: 0.6 },
  header: { flexDirection: "row", alignItems: "center", flexWrap: "wrap", gap: 8 },
  name: { fontSize: 13, fontWeight: "600", color: "#242424" },
  meta: { fontSize: 11, color: "#707070" },
  date: { marginLeft: "auto" },
  body: { fontSize: 14, lineHeight: 24, color: "#505050", marginTop: 9, marginBottom: 12 },
  footer: {
    flexDirection: "row",
    alignItems: "flex-start",
    justifyContent: "space-between",
    gap: 18,
  },
  appreciation: { alignItems: "flex-end" },
  reaction: { flexDirection: "row", alignItems: "center", gap: 7, minHeight: 32, paddingLeft: 7 },
  heart: { fontSize: 15, color: "#818181" },
  heartOn: { color: "#252525" },
  count: { fontSize: 11, color: "#818181" },
});
