// @flow
import { Tabs } from "@uniflowed/router/native-navigation";
export component Layout() {
  return (
    <Tabs
      screenOptions={({ route }) => ({
        headerShown: false,
        title: route.name === "/" ? "Feed" : "You",
        tabBarIcon: () => null,
        tabBarActiveTintColor: "#315744",
        tabBarInactiveTintColor: "#78786d",
        tabBarStyle: { backgroundColor: "#fffdf8", borderTopColor: "#e1dfd5" },
        tabBarLabelStyle: { fontSize: 13, fontWeight: "600" },
      })}
    />
  );
}
