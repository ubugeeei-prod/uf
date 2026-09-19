// @flow
import { Text, View, Pressable } from "react-native";
import { useParams, useNativeRouter } from "@uniflowed/router/native-navigation";
export component Page() {
  const { id } = useParams();
  const router = useNativeRouter();
  return <View><Text>User {id}</Text><Pressable accessibilityRole="button" onPress={() => router.replace("/users/43")}><Text>Replace user</Text></Pressable><Pressable accessibilityRole="button" onPress={() => router.back()}><Text>Back</Text></Pressable></View>;
}
