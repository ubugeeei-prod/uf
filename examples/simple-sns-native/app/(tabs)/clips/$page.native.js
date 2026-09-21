// @flow

import { Image, Linking, Pressable, ScrollView, Text, View } from "react-native";
import { stylex } from "@uniflowed/stylex/native";

import type { ImageSourcePropType } from "react-native";

import { UNDERLINED, styles } from "../../_shared/commonplace.stylex.js";
import { POSTERS } from "../../_shared/media.js";
import { PageHeading, Screen, Topbar } from "../../_shared/ui.native.js";

type Clip = {|
  readonly id: string,
  readonly title: string,
  readonly description: string,
  readonly poster: ImageSourcePropType,
  readonly credit: string,
  readonly source: string,
|};

/**
 * The web examples' curated clips, with the same titles, credits and sources. The web plays the
 * MP4s; this app adds no video dependency, so each clip is its poster and a link to its source.
 */

const CLIPS: $ReadOnlyArray<Clip> = [
  {
    id: "after-dark",
    title: "After dark",
    description: "A different rhythm above the city.",
    poster: POSTERS.city,
    credit: "Rubén Velasco",
    source: "https://mixkit.co/free-stock-video/big-city-at-night-from-an-aerial-shot-49878/",
  },
  {
    id: "last-train",
    title: "The last light",
    description: "Watching the evening settle over the tracks.",
    poster: POSTERS.rail,
    credit: "Rubén Velasco",
    source: "https://mixkit.co/free-stock-video/sunset-over-train-tracks-1594/",
  },
  {
    id: "long-way",
    title: "The long way home",
    description: "Twelve seconds away from the desk.",
    poster: POSTERS.park,
    credit: "Edgar Fernandez",
    source: "https://mixkit.co/free-stock-video/slowly-walking-down-a-path-in-a-park-40656/",
  },
];

export component Page() {
  return (
    <Screen>
      <ScrollView contentContainerStyle={stylex.props(styles.content).style}>
        <Topbar section="Clips" />
        <PageHeading title="Clips">Short moments away from the desk.</PageHeading>
        <View {...stylex.props(local.list)}>
          {CLIPS.map((clip) => (
            <View key={clip.id} {...stylex.props(local.clip)}>
              <Image
                source={clip.poster}
                accessibilityLabel={clip.title}
                resizeMode="cover"
                {...stylex.props(local.poster)}
              />
              <View {...stylex.props(local.caption)}>
                <Text {...stylex.props(local.title)}>{clip.title}</Text>
                <Text {...stylex.props(local.description)}>{clip.description}</Text>
                <Pressable
                  accessibilityRole="link"
                  accessibilityLabel={`Source of ${clip.title}, by ${clip.credit}`}
                  onPress={() => {
                    void Linking.openURL(clip.source);
                  }}
                >
                  <Text style={[stylex.props(local.credit).style, UNDERLINED]}>
                    Video by {clip.credit} · Mixkit
                  </Text>
                </Pressable>
              </View>
            </View>
          ))}
        </View>
      </ScrollView>
    </Screen>
  );
}

const local = stylex.create({
  list: { gap: 18, marginTop: 24 },
  clip: { borderRadius: 14, overflow: "hidden", backgroundColor: "#151515" },
  poster: { width: "100%", height: 420 },
  caption: { position: "absolute", left: 18, right: 18, bottom: 18, gap: 4 },
  title: { fontSize: 20, fontWeight: "600", letterSpacing: -0.4, color: "#ffffff" },
  description: { fontSize: 13, lineHeight: 20, color: "#ffffffd9" },
  credit: { fontSize: 11, color: "#ffffffb3", marginTop: 6 },
});
