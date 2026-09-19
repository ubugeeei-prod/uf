// @flow
import { Text, View } from "react-native";
import { Link } from "@uniflowed/router/native-navigation";
import { stylex } from "@uniflowed/stylex";
import { route } from "../../router";
import type { Note } from "../_shared/notes.js";
import { styles } from "../_shared/commonplace.stylex.js";
export component NoteCard(note: Note) {
  return (
    <View {...stylex.props(styles.card)}>
      <Text {...stylex.props(styles.eyebrow)}>{note.topic}</Text>
      <Text {...stylex.props(styles.author)}>
        {note.author} · @{note.handle}
      </Text>
      <Text {...stylex.props(styles.body)}>{note.body}</Text>
      <Link
        href={route("/notes/:id", { id: note.id })}
        accessibilityLabel={"Open note by " + note.author}
      >
        <Text {...stylex.props(styles.link)}>Read note →</Text>
      </Link>
    </View>
  );
}
