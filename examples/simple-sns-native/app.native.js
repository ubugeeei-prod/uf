// @flow

import { useState } from "react";
import { createNativeStackNavigator } from "@react-navigation/native-stack";
import { createBottomTabNavigator } from "@react-navigation/bottom-tabs";
import { createNativeNavigation } from "@uniflowed/router/native-navigation";

import type { Service } from "./app/_shared/service.js";

import { SocialProvider } from "./app/_shared/client.js";
import { createService } from "./app/_shared/service.js";
import { routeTable as table, layouts } from "./router.native.js";

const navigation = createNativeNavigation({
  table,
  layouts,
  stack: createNativeStackNavigator(),
  tabs: createBottomTabNavigator(),
});

/**
 * The whole app around the service it talks to. A test hands in one that answers at once, or one
 * that delays or refuses a particular call.
 */

export component Commonplace(service: Service) {
  const Root = navigation.Root;

  return (
    <SocialProvider service={service}>
      <Root />
    </SocialProvider>
  );
}

/** What Expo registers: Commonplace on the in-memory service, with its usual latency. */

export component App() {
  const [service] = useState(() => createService());

  return <Commonplace service={service} />;
}
