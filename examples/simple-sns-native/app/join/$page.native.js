// @flow

import { startTransition, useActionState, useState } from "react";
import { Pressable, ScrollView, Text, View } from "react-native";
import { useNativeRouter } from "@uniflowed/router/native-navigation";
import { stylex } from "@uniflowed/stylex/native";

import type { Account, FormState, User } from "../_shared/social.js";

import { useSocial } from "../_shared/client.js";
import { styles } from "../_shared/commonplace.stylex.js";
import { IDLE } from "../_shared/social.js";
import { Button, Field, FormStatus, PageHeading, Screen } from "../_shared/ui.native.js";

/**
 * The web examples' signup, without its password: there is no service here to check one
 * against, and a field that protects nothing would only teach the wrong thing.
 *
 * Creating the account is an Action. When it succeeds every read is refreshed — this person is
 * now someone — and the screen underneath, which asked them to join, is where they go back to.
 */

export component Page() {
  const { service, refresh } = useSocial();
  const router = useNativeRouter();
  const [name, setName] = useState("");
  const [handle, setHandle] = useState("");
  const [email, setEmail] = useState("");
  const [state, submit, pending] = useActionState<FormState<User>, Account>(
    async (_previous: FormState<User>, account: Account): Promise<FormState<User>> => {
      const result = await service.join(account);
      match (result) {
        {status: "success", ...} => {
          refresh();
          router.back();
        }
        {status: "error", ...} => {}
      }

      return result;
    },
    IDLE,
  );

  return (
    <Screen edges={["top", "bottom"]}>
      <ScrollView
        keyboardShouldPersistTaps="handled"
        contentContainerStyle={stylex.props(styles.content).style}
      >
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="Back"
          hitSlop={10}
          onPress={() => router.back()}
        >
          <Text {...stylex.props(local.back)}>← Back</Text>
        </Pressable>
        <PageHeading title="Join Commonplace">
          A place to share what you are working on.
        </PageHeading>
        <View {...stylex.props(local.form)}>
          <Field label="Display name" value={name} onChangeText={setName} maxLength={80} />
          <Field label="Handle" value={handle} onChangeText={setHandle} maxLength={24} plain />
          <Field label="Email" value={email} onChangeText={setEmail} email />
          <View {...stylex.props(local.status)}>
            <FormStatus state={state} quiet />
          </View>
          <Button
            pending={pending ? "Please wait…" : null}
            label="Create account"
            onPress={() => startTransition(() => submit({ name, handle, email }))}
          >
            Create account
          </Button>
          <Text {...stylex.props(styles.hint, local.note)}>
            Accounts live in memory and are gone when the app restarts.
          </Text>
        </View>
      </ScrollView>
    </Screen>
  );
}

const local = stylex.create({
  back: { fontSize: 12, fontWeight: "600", color: "#242424", paddingBottom: 22 },
  form: { marginTop: 28 },
  status: { marginBottom: 14 },
  note: { marginTop: 16, textAlign: "center" },
});
