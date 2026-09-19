// @flow
import { Text, View } from "react-native";
import { stylex } from "@uniflowed/stylex";
import { Link } from "@uniflowed/router/native-navigation";
import { route } from "../../router";
const styles = stylex.create({ root: { padding: 12, backgroundColor: "#f3f7fb" } });
export component Page() {
  return <View testID="home" {...stylex.props(styles.root)}><Text>Home</Text><Link href={route("/users/:id", { id: "42" })}><Text>Open user</Text></Link></View>;
}
