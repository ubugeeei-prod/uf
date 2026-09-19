// @flow
import { createNativeStackNavigator } from "@react-navigation/native-stack";
import { createBottomTabNavigator } from "@react-navigation/bottom-tabs";
import { createNativeNavigation } from "@uniflowed/router/native-navigation";
import { NotesProvider } from "./app/_shared/notes.js";
import { routeTable as table, layouts } from "./router.native.js";
const navigation = createNativeNavigation({
  table,
  layouts,
  stack: createNativeStackNavigator(),
  tabs: createBottomTabNavigator(),
});
export component App() {
  const Root = navigation.Root;
  return (
    <NotesProvider>
      <Root />
    </NotesProvider>
  );
}
