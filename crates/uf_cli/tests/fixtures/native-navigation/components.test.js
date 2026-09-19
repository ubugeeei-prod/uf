// @flow
import * as React from "react";
import { View, Text, Pressable, TextInput, ScrollView } from "react-native";
import { render, screen, press, changeText, scroll } from "@uniflowed/react-native-testing/native";
import { test, expect } from "@uniflowed/test";

component Screen() {
  const [name, setName] = React.useState("");
  const [count, setCount] = React.useState(0);
  const [offset, setOffset] = React.useState(0);
  return <View>
    <Pressable accessibilityRole="button" onPress={() => setCount(count + 1)}><Text>Count {count}</Text></Pressable>
    <TextInput testID="name" value={name} onChangeText={setName} />
    <Text>{name}</Text>
    <ScrollView testID="scroller" onScroll={(event) => setOffset(event.nativeEvent.contentOffset.y)}><Text>Offset {offset}</Text></ScrollView>
  </View>;
}

test("real RN components render and receive press, text and scroll events", async () => {
  await render(<Screen />);
  await press(screen.getByRole("button"));
  expect(screen.getByText("Count 1")).toBeTruthy();
  await changeText(screen.getByTestId("name"), "Ada");
  expect(screen.getByText("Ada")).toBeTruthy();
  await scroll(screen.getByTestId("scroller"), { nativeEvent: { contentOffset: { x: 0, y: 80 } } });
  expect(screen.getByText("Offset 80")).toBeTruthy();
});
