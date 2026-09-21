// @flow

import { Tabs } from "@uniflowed/router/native-navigation";

import { FAINT, INK } from "../_shared/commonplace.stylex.js";
import { Glyph, type GlyphName } from "../_shared/glyphs.native.js";

/** The web's mobile navigation: Feed, Clips, Inbox and Settings. */

const TABS: { readonly [path: string]: {| readonly title: string, readonly glyph: GlyphName |} } = {
  "/": { title: "Feed", glyph: "feed" },
  "/clips": { title: "Clips", glyph: "clips" },
  "/messages": { title: "Inbox", glyph: "inbox" },
  "/settings": { title: "Settings", glyph: "settings" },
};

export component Layout() {
  return (
    <Tabs
      screenOptions={({ route }: { route: { name: string, ... }, ... }) => {
        const tab = TABS[route.name] ?? TABS["/"];

        return {
          headerShown: false,
          title: tab.title,
          tabBarIcon: ({ color }: { color: string, ... }) => (
            <Glyph name={tab.glyph} color={color} />
          ),
          tabBarActiveTintColor: INK,
          tabBarInactiveTintColor: FAINT,
          tabBarStyle: { backgroundColor: "#f8f8f8", borderTopColor: "#dcdcdc" },
          tabBarLabelStyle: { fontSize: 10, fontWeight: "500" },
        };
      }}
    />
  );
}
