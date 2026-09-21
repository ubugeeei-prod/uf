// @flow

import { Tabs } from "@uniflowed/router/native-navigation";

import { FAINT, INK } from "../_shared/commonplace.stylex.js";
import { Glyph, type GlyphName } from "../_shared/glyphs.native.js";

type Tab = {| readonly title: string, readonly glyph: GlyphName |};

/** The web's mobile navigation: Feed, Clips, Inbox and Settings. */

function tabFor(path: string): Tab {
  return match (path) {
    "/clips" => { title: "Clips", glyph: "clips" },
    "/messages" => { title: "Inbox", glyph: "inbox" },
    "/settings" => { title: "Settings", glyph: "settings" },
    _ => { title: "Feed", glyph: "feed" },
  };
}

/** React Navigation asks for an icon with the tint it chose; the answer is always a glyph. */

component TabIcon(glyph: GlyphName, color: string) renders Glyph {
  return <Glyph name={glyph} color={color} />;
}

export component Layout() {
  return (
    <Tabs
      screenOptions={({ route }: { route: { name: string, ... }, ... }) => {
        const tab = tabFor(route.name);

        return {
          headerShown: false,
          title: tab.title,
          tabBarIcon: ({ color }: { color: string, ... }) => (
            <TabIcon glyph={tab.glyph} color={color} />
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
