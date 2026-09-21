// @flow

import { useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native";
import { stylex } from "@uniflowed/stylex/native";

import { UNDERLINED, styles } from "../_shared/commonplace.stylex.js";

/**
 * The draft is this component's; the feed hears about it on submit, like the web's GET form.
 * The feed keys this by the committed search, so clearing it also resets the draft.
 */

export component SearchNotes(search: string, onSearch: (string) => void) {
  const [draft, setDraft] = useState(search);
  const submit = () => onSearch(draft.trim().slice(0, 100));

  return (
    <View accessibilityRole="search" {...stylex.props(styles.rule, local.row)}>
      <TextInput
        accessibilityLabel="Search notes"
        placeholder="Search notes and people"
        placeholderTextColor="#8a8a8a"
        returnKeyType="search"
        autoCapitalize="none"
        autoCorrect={false}
        maxLength={100}
        value={draft}
        onChangeText={setDraft}
        onSubmitEditing={submit}
        {...stylex.props(local.input)}
      />
      <Pressable accessibilityRole="button" accessibilityLabel="Search" onPress={submit}>
        <Text {...stylex.props(local.action)}>Search</Text>
      </Pressable>
      {search !== "" ? (
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="Clear search"
          onPress={() => onSearch("")}
        >
          <Text style={[stylex.props(styles.textLink).style, UNDERLINED]}>Clear</Text>
        </Pressable>
      ) : null}
    </View>
  );
}

const local = stylex.create({
  row: { flexDirection: "row", alignItems: "center", gap: 12, paddingTop: 4, paddingBottom: 4 },
  input: { flex: 1, minWidth: 0, fontSize: 13, color: "#242424", paddingTop: 8, paddingBottom: 8 },
  action: { fontSize: 12, fontWeight: "600", color: "#242424" },
});
