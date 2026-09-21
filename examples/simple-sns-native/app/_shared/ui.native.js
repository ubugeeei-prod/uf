// @flow

import * as React from "react";
import { Image, Pressable, Text, TextInput, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Link } from "@uniflowed/router/native-navigation";
import { stylex } from "@uniflowed/stylex/native";

import { route } from "../../router";

import type { FormState, Topic, User } from "./social.js";

import { TOP_ALIGNED, styles } from "./commonplace.stylex.js";
import { portrait } from "./media.js";
import { topicLabel } from "./social.js";

/** A screen's frame: the safe area, the brand row the web examples open with, and its content. */

export component Screen(children: React.Node, edges: $ReadOnlyArray<"top" | "bottom"> = ["top"]) {
  return (
    <SafeAreaView edges={edges} {...stylex.props(styles.page)}>
      {children}
    </SafeAreaView>
  );
}

export component Topbar(section: string) {
  return (
    <View {...stylex.props(styles.topbar)}>
      <Text {...stylex.props(styles.brand)}>Commonplace</Text>
      <Text {...stylex.props(styles.topbarAction)}>{section}</Text>
    </View>
  );
}

export component PageHeading(title: string, children: string) {
  return (
    <View accessibilityRole="header">
      <Text {...stylex.props(styles.heading)}>{title}</Text>
      <Text {...stylex.props(styles.lede)}>{children}</Text>
    </View>
  );
}

/** The circle every avatar is drawn in; decorative, because a name is always beside it. */

component AvatarFrame(small: boolean, children: React.Node) {
  return (
    <View
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      {...stylex.props(styles.avatar, small && styles.avatarSmall)}
    >
      {children}
    </View>
  );
}

/** A licensed portrait for a fixture member, or the account's initials. */

export component Avatar(user: User, small: boolean = false) renders AvatarFrame {
  const photo = portrait(user.id);

  return (
    <AvatarFrame small={small}>
      {photo == null ? (
        <Text {...stylex.props(styles.avatarInitials, small && styles.avatarInitialsSmall)}>
          {user.avatar}
        </Text>
      ) : small ? (
        <Image source={photo} {...stylex.props(local.photoSmall)} />
      ) : (
        <Image source={photo} {...stylex.props(local.photo)} />
      )}
    </AvatarFrame>
  );
}

/**
 * `pending` is an Action's: the button says what is happening and cannot start it twice, which is
 * what the web examples' submit buttons do with `useActionState`'s third value.
 */

export component Button(
  children: string,
  onPress: () => void,
  primary: boolean = true,
  disabled: boolean = false,
  pending: string | null = null,
  label?: string,
) {
  const busy = pending != null;

  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label ?? children}
      accessibilityState={{ disabled: disabled || busy, busy }}
      disabled={disabled || busy}
      onPress={onPress}
      {...stylex.props(
        styles.button,
        primary && styles.buttonPrimary,
        (disabled || busy) && styles.buttonDisabled,
      )}
    >
      <Text {...stylex.props(styles.buttonLabel, primary && styles.buttonLabelPrimary)}>
        {pending ?? children}
      </Text>
    </Pressable>
  );
}

/** Navigation with the primary button's treatment, as the web's `ActionLink`. */

export component ActionLink(href: string, children: string, primary: boolean = true) renders Link {
  return (
    <Link href={href} accessibilityLabel={children}>
      <View {...stylex.props(styles.button, primary && styles.buttonPrimary)}>
        <Text {...stylex.props(styles.buttonLabel, primary && styles.buttonLabelPrimary)}>
          {children}
        </Text>
      </View>
    </Link>
  );
}

export component Field(
  label: string,
  value: string,
  onChangeText: (string) => void,
  multiline: boolean = false,
  maxLength?: number,
  email: boolean = false,
  plain: boolean = false,
) {
  return (
    <View {...stylex.props(styles.field)}>
      <Text {...stylex.props(styles.fieldLabel)}>{label}</Text>
      {multiline ? (
        <TextInput
          accessibilityLabel={label}
          value={value}
          onChangeText={onChangeText}
          multiline
          maxLength={maxLength}
          style={[stylex.props(local.textarea).style, TOP_ALIGNED]}
        />
      ) : (
        <TextInput
          accessibilityLabel={label}
          value={value}
          onChangeText={onChangeText}
          maxLength={maxLength}
          autoCapitalize={email || plain ? "none" : "sentences"}
          autoCorrect={!email && !plain}
          keyboardType={email ? "email-address" : "default"}
          {...stylex.props(local.input)}
        />
      )}
    </View>
  );
}

/** One line under a form: an alert a screen reader interrupts for, or a quiet confirmation. */

component Feedback(tone: "alert" | "status", children: string) {
  return match (tone) {
    "alert" =>
      <Text accessibilityRole="alert" {...stylex.props(styles.alert)}>
        {children}
      </Text>,
    "status" =>
      <Text accessibilityLiveRegion="polite" {...stylex.props(styles.status)}>
        {children}
      </Text>,
  };
}

/**
 * What an Action left behind. A failure is always said; a success only where the screen does not
 * already show it, and `quiet` is how a caller says that it does.
 */

export component FormStatus(state: FormState<mixed>, quiet: boolean = false) renders? Feedback {
  return match (state) {
    {status: "idle"} => null,
    {status: "error", message: const message} => <Feedback tone="alert">{message}</Feedback>,
    {status: "success", message: const message, ...} =>
      quiet || message === "" ? null : <Feedback tone="status">{message}</Feedback>,
  };
}

export component ChannelBadge(topic: Topic) {
  return (
    <View {...stylex.props(local.badge)}>
      <View {...stylex.props(styles.channelDot)} />
      <Text {...stylex.props(local.badgeLabel)}>{topicLabel(topic)}</Text>
    </View>
  );
}

/** A regional explanation with an optional recovery: navigation, or a retry. */

export component EmptyState(
  title: string,
  children: string,
  action: renders? (ActionLink | Button) = null,
) {
  return (
    <View {...stylex.props(styles.empty)}>
      <Text {...stylex.props(styles.emptyTitle)}>{title}</Text>
      <Text {...stylex.props(styles.emptyBody)}>{children}</Text>
      {action}
    </View>
  );
}

/** What a private screen shows a guest, as the web's `SignInPrompt`. */

export component SignInPrompt(title: string = "Sign in to continue") renders EmptyState {
  return (
    <EmptyState
      title={title}
      action={<ActionLink href={route("/join")}>Create account</ActionLink>}
    >
      Your conversations and settings stay with your account.
    </EmptyState>
  );
}

component Bone(width: number | "100%", height: number = 10, round: boolean = false) {
  return (
    <View style={[stylex.props(local.bone, round && local.boneRound).style, { width, height }]} />
  );
}

export type Loading = "feed" | "threads" | "conversation" | "profile";

/**
 * Hold each region's real geometry while its own boundary waits, as the web's `LoadingState`.
 * The bones are decorative; one label tells assistive technology what is loading.
 */

export component LoadingState(kind: Loading) {
  const label = match (kind) {
    "feed" => "Loading notes",
    "threads" => "Loading conversations",
    "conversation" => "Loading messages",
    "profile" => "Loading profile",
  };

  return (
    <View
      accessibilityRole="progressbar"
      accessibilityLabel={label}
      accessibilityState={{ busy: true }}
      {...stylex.props(local.loading)}
    >
      {
        match (kind) {
          "feed" =>
            [0, 1, 2].map((row) => (
              <View key={row} {...stylex.props(styles.rule, local.skeletonPost)}>
                <Bone width={42} height={42} round />
                <View {...stylex.props(styles.grow, local.skeletonLines)}>
                  <Bone width={120} />
                  <Bone width="100%" />
                  <Bone width="100%" />
                  <Bone width={row === 1 ? 140 : 210} />
                </View>
              </View>
            )),
          "threads" =>
            [0, 1].map((row) => (
              <View key={row} {...stylex.props(styles.rule, local.skeletonThread)}>
                <Bone width={34} height={34} round />
                <View {...stylex.props(styles.grow, local.skeletonLines)}>
                  <Bone width={96} />
                  <Bone width={220} />
                </View>
              </View>
            )),
          "conversation" =>
            <View {...stylex.props(local.skeletonLog)}>
              <Bone width={230} height={42} />
              <View {...stylex.props(local.skeletonMine)}>
                <Bone width={180} height={42} />
              </View>
            </View>,
          "profile" =>
            [0, 1, 2].map((row) => (
              <View key={row} {...stylex.props(local.skeletonField)}>
                <Bone width={84} />
                <Bone width="100%" height={row === 1 ? 76 : 40} />
              </View>
            )),
        }
      }
    </View>
  );
}

// Image and TextInput take their styles from this module: handed one that was
// imported from another, Flow's check of those two components runs out of
// recursion before it reaches an answer.
const local = stylex.create({
  photo: { width: 42, height: 42, borderRadius: 21 },
  photoSmall: { width: 34, height: 34, borderRadius: 17 },
  input: {
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
  textarea: {
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
    minHeight: 76,
  },
  badge: { flexDirection: "row", alignItems: "center", gap: 6, paddingTop: 4, paddingBottom: 4 },
  badgeLabel: { fontSize: 10, color: "#6b6b6b" },
  loading: { paddingTop: 4 },
  bone: { borderRadius: 4, backgroundColor: "#e6e6e6" },
  boneRound: { borderRadius: 21 },
  skeletonPost: { flexDirection: "row", gap: 14, paddingTop: 24, paddingBottom: 24 },
  skeletonThread: {
    flexDirection: "row",
    alignItems: "center",
    gap: 12,
    paddingTop: 16,
    paddingBottom: 16,
  },
  skeletonLines: { gap: 10, paddingTop: 4 },
  skeletonLog: { gap: 14, paddingLeft: 20, paddingRight: 20, paddingTop: 18 },
  skeletonMine: { alignItems: "flex-end" },
  skeletonField: { gap: 8, marginBottom: 19 },
});
