// @flow
import { Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { stylex } from "@uniflowed/stylex";
import { useNotes } from "../../_shared/notes.js";
import { styles } from "../../_shared/commonplace.stylex.js";
export component Page() {
  const { notes } = useNotes();
  const count = notes.filter((note) => note.handle === "you").length;
  return (
    <SafeAreaView edges={["top"]} {...stylex.props(styles.page)}>
      <View {...stylex.props(styles.content)}>
        <Text {...stylex.props(styles.eyebrow)}>YOUR CORNER</Text>
        <Text {...stylex.props(styles.title)}>Make yourself at home.</Text>
        <View {...stylex.props(styles.card)}>
          <Text {...stylex.props(styles.author)}>You · @you</Text>
          <Text {...stylex.props(styles.body)}>{count} notes shared this visit</Text>
          <Text {...stylex.props(styles.subtitle)}>A quiet place for everyday things.</Text>
        </View>
      </View>
    </SafeAreaView>
  );
}
