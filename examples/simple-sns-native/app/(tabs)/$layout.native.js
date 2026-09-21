// @flow

import { Tabs } from "@uniflowed/router/native-navigation";

import { FAINT, INK } from "../_shared/commonplace.stylex.js";
import { Glyph, type GlyphName } from "../_shared/glyphs.native.js";

type Tab = {|
  readonly title: string,
  readonly glyph: GlyphName,
  // Clips is edge to edge and dark, so the bar under it is too.
  readonly immersive: boolean,
|};

/** The web's mobile navigation: Feed, Clips, Inbox and Settings. */

function tabFor(path: string): Tab {
  return match (path) {
    "/clips" => { title: "Clips", glyph: "clips", immersive: true },
    "/messages" => { title: "Inbox", glyph: "inbox", immersive: false },
    "/settings" => { title: "Settings", glyph: "settings", immersive: false },
    _ => { title: "Feed", glyph: "feed", immersive: false },
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
          tabBarActiveTintColor: tab.immersive ? "#ffffff" : INK,
          tabBarInactiveTintColor: tab.immersive ? "#8f8f8f" : FAINT,
          tabBarStyle: tab.immersive
            ? { backgroundColor: "#000000", borderTopColor: "#1f1f1f" }
            : { backgroundColor: "#f8f8f8", borderTopColor: "#dcdcdc" },
          tabBarLabelStyle: { fontSize: 10, fontWeight: "500" },
        };
      }}
    />
  );
}
