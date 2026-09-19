// @flow
import { Pressable, ScrollView, Text } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useNativeRouter, useParams } from "@uniflowed/router/native-navigation";
import { stylex } from "@uniflowed/stylex";
import { useNotes } from "../../_shared/notes.js";
import { styles } from "../../_shared/commonplace.stylex.js";
export component Page() {
  const { id } = useParams();
  const { notes } = useNotes();
  const router = useNativeRouter();
  const note = notes.find((item) => item.id === id);
  return (
    <SafeAreaView {...stylex.props(styles.page)}>
      <ScrollView contentContainerStyle={stylex.props(styles.content).style}>
        <Pressable accessibilityRole="button" onPress={() => router.back()}>
          <Text {...stylex.props(styles.link)}>← Back to feed</Text>
        </Pressable>
        {note ? (
          <>
            <Text {...stylex.props(styles.eyebrow)}>{note.topic}</Text>
            <Text {...stylex.props(styles.title)}>
              {note.handle === "you" ? "Your note" : `${note.author}'s note`}
            </Text>
            <Text {...stylex.props(styles.body)}>{note.body}</Text>
          </>
        ) : (
          <Text {...stylex.props(styles.title)}>This note is no longer here.</Text>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}
