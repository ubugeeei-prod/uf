// @flow

import { useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { stylex } from "@uniflowed/stylex/native";

import type { Settings, User } from "../../_shared/social.js";

import { styles } from "../../_shared/commonplace.stylex.js";
import { useSocial } from "../../_shared/social.js";
import {
  Avatar,
  Button,
  Field,
  PageHeading,
  Screen,
  SignInPrompt,
  Topbar,
} from "../../_shared/ui.native.js";

export component Page() {
  const { viewer, settings } = useSocial();

  return (
    <Screen>
      <ScrollView
        keyboardShouldPersistTaps="handled"
        contentContainerStyle={stylex.props(styles.content).style}
      >
        <Topbar section="Settings" />
        <PageHeading title="Settings">Your profile and account.</PageHeading>
        {viewer == null || settings == null ? (
          <SignInPrompt />
        ) : (
          // A different account is a different form, with that account's values.
          <SettingsForm key={viewer.id} viewer={viewer} settings={settings} />
        )}
      </ScrollView>
    </Screen>
  );
}

/** The account to update is the signed-in one; no account id is part of the form. */

component SettingsForm(viewer: User, settings: Settings) {
  const { updateSettings, signOut } = useSocial();
  const [displayName, setDisplayName] = useState(settings.displayName);
  const [bio, setBio] = useState(settings.bio);
  const [email, setEmail] = useState(settings.email);
  const [feedback, setFeedback] = useState<{|
    readonly ok: boolean,
    readonly message: string,
  |} | null>(null);

  return (
    <View {...stylex.props(local.form)}>
      <View {...stylex.props(styles.rule, local.identity)}>
        <Avatar user={viewer} />
        <View {...stylex.props(styles.grow)}>
          <Text {...stylex.props(local.name)}>{viewer.name}</Text>
          <Text {...stylex.props(styles.hint)}>@{viewer.handle}</Text>
        </View>
      </View>
      <Field
        label="Display name"
        value={displayName}
        onChangeText={setDisplayName}
        maxLength={80}
      />
      <Field label="Bio" value={bio} onChangeText={setBio} maxLength={240} multiline />
      <Field label="Email" value={email} onChangeText={setEmail} email />
      <View {...stylex.props(styles.spread, local.footer)}>
        <Button primary={false} onPress={signOut}>
          Sign out
        </Button>
        <Button
          onPress={() => {
            const outcome = updateSettings({ displayName, bio, email });
            setFeedback(
              outcome.ok
                ? { ok: true, message: "Your changes are saved." }
                : { ok: false, message: outcome.message },
            );
          }}
        >
          Save changes
        </Button>
      </View>
      {feedback != null ? (
        <Text
          accessibilityRole={feedback.ok ? "text" : "alert"}
          accessibilityLiveRegion="polite"
          {...stylex.props(feedback.ok ? styles.status : styles.alert)}
        >
          {feedback.message}
        </Text>
      ) : null}
    </View>
  );
}

const local = stylex.create({
  form: { marginTop: 8 },
  identity: {
    flexDirection: "row",
    alignItems: "center",
    gap: 12,
    paddingTop: 18,
    paddingBottom: 18,
    marginBottom: 22,
  },
  name: { fontSize: 14, fontWeight: "600", color: "#242424" },
  footer: { borderTopWidth: 1, borderTopColor: "#dcdcdc", paddingTop: 20, marginTop: 4 },
});
