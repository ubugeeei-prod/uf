// @flow
import { useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native";
import { stylex } from "@uniflowed/stylex";
import { useNotes } from "../_shared/notes.js";
import { styles } from "../_shared/commonplace.stylex.js";
const local = stylex.create({
  input: { fontSize: 16, lineHeight: 24, minHeight: 68, color: "#3e473e" },
  button: {
    alignSelf: "flex-end",
    paddingLeft: 18,
    paddingRight: 18,
    paddingTop: 10,
    paddingBottom: 10,
    borderRadius: 20,
    backgroundColor: "#315744",
  },
  disabled: { opacity: 0.4 },
  label: { color: "#fffdf8", fontSize: 13, fontWeight: "600" },
});
export component Composer() {
  const [body, setBody] = useState("");
  const { publish } = useNotes();
  const disabled = body.trim().length === 0;
  return (
    <View {...stylex.props(styles.card)}>
      <Text {...stylex.props(styles.author)}>Leave a little note</Text>
      <TextInput
        accessibilityLabel="Your note"
        placeholder="What made you pause today?"
        multiline
        maxLength={500}
        value={body}
        onChangeText={setBody}
        {...stylex.props(local.input)}
      />
      <Pressable
        accessibilityRole="button"
        accessibilityLabel="Share note"
        disabled={disabled}
        onPress={() => {
          publish(body);
          setBody("");
        }}
        {...stylex.props(local.button, disabled && local.disabled)}
      >
        <Text {...stylex.props(local.label)}>Share note</Text>
      </Pressable>
    </View>
  );
}
