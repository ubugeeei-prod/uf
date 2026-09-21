// @flow

import { startTransition, useActionState, useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { stylex } from "@uniflowed/stylex/native";

import type { FormState, Session, Settings } from "../../_shared/social.js";

import { AsyncRegion } from "../../_shared/async-region.native.js";
import { useResource, useSocial } from "../../_shared/client.js";
import { styles } from "../../_shared/commonplace.stylex.js";
import { IDLE } from "../../_shared/social.js";
import {
  Avatar,
  Button,
  Field,
  FormStatus,
  LoadingState,
  PageHeading,
  Screen,
  SignInPrompt,
  Topbar,
} from "../../_shared/ui.native.js";

/** Who the form belongs to; the fields beside it are the form's own, until they are saved. */

component Identity(session: Session) {
  return match (session) {
    {kind: "guest"} => null,
    {kind: "authenticated", user: const user} =>
      <View {...stylex.props(styles.rule, local.identity)}>
        <Avatar user={user} />
        <View {...stylex.props(styles.grow)}>
          <Text {...stylex.props(local.name)}>{user.name}</Text>
          <Text {...stylex.props(styles.hint)}>@{user.handle}</Text>
        </View>
      </View>,
  };
}

/**
 * Saving and signing out are both Actions: each button says what it is doing while it does it,
 * and a success refreshes every read, so the name above the form, the feed and the inbox follow.
 * The account to update is the signed-in one; no account id is part of the form.
 */

component SettingsForm(session: Session, settings: Settings) {
  const { service, refresh } = useSocial();
  const [displayName, setDisplayName] = useState(settings.displayName);
  const [bio, setBio] = useState(settings.bio);
  const [email, setEmail] = useState(settings.email);
  const [saved, save, saving] = useActionState<FormState<Settings>, Settings>(
    async (_previous: FormState<Settings>, next: Settings): Promise<FormState<Settings>> => {
      const result = await service.updateSettings(next);
      if (result.status === "success") refresh();

      return result;
    },
    IDLE,
  );
  const [left, leave, leaving] = useActionState<FormState<null>, void>(async () => {
    const result = await service.signOut();
    if (result.status === "success") refresh();

    return result;
  }, IDLE);

  return (
    <View {...stylex.props(local.form)}>
      <Identity session={session} />
      <Field
        label="Display name"
        value={displayName}
        onChangeText={setDisplayName}
        maxLength={80}
      />
      <Field label="Bio" value={bio} onChangeText={setBio} maxLength={240} multiline />
      <Field label="Email" value={email} onChangeText={setEmail} email />
      <View {...stylex.props(styles.spread, local.footer)}>
        <Button
          primary={false}
          pending={leaving ? "Signing out…" : null}
          label="Sign out"
          onPress={() => startTransition(() => leave())}
        >
          Sign out
        </Button>
        <Button
          pending={saving ? "Saving…" : null}
          label="Save changes"
          onPress={() => startTransition(() => save({ displayName, bio, email }))}
        >
          Save changes
        </Button>
      </View>
      <FormStatus state={saved} />
      <FormStatus state={left} quiet />
    </View>
  );
}

export component Page() {
  const { resource, retry } = useResource("settings", (service) =>
    Promise.all([service.session(), service.settings()]).then(([session, settings]) => ({
      session,
      settings,
    })),
  );

  return (
    <Screen>
      <ScrollView
        keyboardShouldPersistTaps="handled"
        contentContainerStyle={stylex.props(styles.content).style}
      >
        <Topbar section="Settings" />
        <PageHeading title="Settings">Your profile and account.</PageHeading>
        <AsyncRegion
          resource={resource}
          label="your profile"
          retry={retry}
          pending={<LoadingState kind="profile" />}
        >
          {({ session, settings }) =>
            match (settings) {
              {kind: "unauthenticated"} => <SignInPrompt />,
              // A different account is a different form, with that account's values.
              {kind: "ready", value: const value} =>
                <SettingsForm
                  key={session.kind === "authenticated" ? session.user.id : "guest"}
                  session={session}
                  settings={value}
                />,
            }}
        </AsyncRegion>
      </ScrollView>
    </Screen>
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
