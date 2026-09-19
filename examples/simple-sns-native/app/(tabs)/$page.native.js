// @flow
import { ScrollView, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { stylex } from "@uniflowed/stylex";
import { useNotes } from "../_shared/notes.js";
import { styles } from "../_shared/commonplace.stylex.js";
import { NoteCard } from "./note-card.native.js";
import { Composer } from "./composer.native.js";
export component Page() {
  const { notes } = useNotes();
  return (
    <SafeAreaView edges={["top"]} {...stylex.props(styles.page)}>
      <ScrollView contentContainerStyle={stylex.props(styles.content).style}>
        <View>
          <Text {...stylex.props(styles.eyebrow)}>COMMONPLACE</Text>
          <Text {...stylex.props(styles.title)}>Good to see you.</Text>
          <Text {...stylex.props(styles.subtitle)}>Small notes from your corner of the world.</Text>
        </View>
        <Composer />
        {notes.map((note) => (
          <NoteCard key={note.id} note={note} />
        ))}
      </ScrollView>
    </SafeAreaView>
  );
}
