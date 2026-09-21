// @flow

import * as React from "react";
import { Image, Pressable, Text, TextInput, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { Link } from "@uniflowed/router/native-navigation";
import { stylex } from "@uniflowed/stylex/native";

import { route } from "../../router";

import type { Topic, User } from "./social.js";

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

/** A licensed portrait for a fixture member, or the account's initials. */

export component Avatar(user: User, small: boolean = false) {
  const photo = portrait(user.id);

  return (
    <View
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      {...stylex.props(styles.avatar, small && styles.avatarSmall)}
    >
      {photo == null ? null : small ? (
        <Image source={photo} {...stylex.props(local.photoSmall)} />
      ) : (
        <Image source={photo} {...stylex.props(local.photo)} />
      )}
      {photo != null ? null : (
        <Text {...stylex.props(styles.avatarInitials, small && styles.avatarInitialsSmall)}>
          {user.avatar}
        </Text>
      )}
    </View>
  );
}

export component Button(
  children: string,
  onPress: () => void,
  primary: boolean = true,
  disabled: boolean = false,
  label?: string,
) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label ?? children}
      accessibilityState={{ disabled }}
      disabled={disabled}
      onPress={onPress}
      {...stylex.props(
        styles.button,
        primary && styles.buttonPrimary,
        disabled && styles.buttonDisabled,
      )}
    >
      <Text {...stylex.props(styles.buttonLabel, primary && styles.buttonLabelPrimary)}>
        {children}
      </Text>
    </Pressable>
  );
}

/** Navigation with the primary button's treatment, as the web's `ActionLink`. */

export component ActionLink(href: string, children: string, primary: boolean = true) {
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

export component ChannelBadge(topic: Topic) {
  return (
    <View {...stylex.props(local.badge)}>
      <View {...stylex.props(styles.channelDot)} />
      <Text {...stylex.props(local.badgeLabel)}>{topicLabel(topic)}</Text>
    </View>
  );
}

export component EmptyState(title: string, children: string, action: React.Node = null) {
  return (
    <View {...stylex.props(styles.empty)}>
      <Text {...stylex.props(styles.emptyTitle)}>{title}</Text>
      <Text {...stylex.props(styles.emptyBody)}>{children}</Text>
      {action}
    </View>
  );
}

/** What a private screen shows a guest, as the web's `SignInPrompt`. */

export component SignInPrompt(title: string = "Sign in to continue") {
  return (
    <EmptyState
      title={title}
      action={<ActionLink href={route("/join")}>Create account</ActionLink>}
    >
      Your conversations and settings stay with your account.
    </EmptyState>
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
});
