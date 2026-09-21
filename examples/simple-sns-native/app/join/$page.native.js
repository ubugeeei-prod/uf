// @flow

import { useState } from "react";
import { Pressable, ScrollView, Text, View } from "react-native";
import { useNativeRouter } from "@uniflowed/router/native-navigation";
import { stylex } from "@uniflowed/stylex/native";

import { styles } from "../_shared/commonplace.stylex.js";
import { useSocial } from "../_shared/social.js";
import { Button, Field, PageHeading, Screen } from "../_shared/ui.native.js";

/**
 * The web examples' signup, without its password: there is no service here to check one
 * against, and a field that protects nothing would only teach the wrong thing.
 */

export component Page() {
  const { join } = useSocial();
  const router = useNativeRouter();
  const [name, setName] = useState("");
  const [handle, setHandle] = useState("");
  const [email, setEmail] = useState("");
  const [error, setError] = useState("");

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
          {error !== "" ? (
            <Text accessibilityRole="alert" {...stylex.props(styles.alert, local.error)}>
              {error}
            </Text>
          ) : null}
          <Button
            onPress={() => {
              const outcome = join({ name, handle, email });
              if (outcome.ok) router.back();
              else setError(outcome.message);
            }}
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
  error: { marginTop: 0, marginBottom: 14 },
  note: { marginTop: 16, textAlign: "center" },
});
