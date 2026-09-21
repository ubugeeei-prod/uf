// @flow

import * as React from "react";
import { useRef, useState } from "react";
import {
  Image,
  Linking,
  Pressable,
  ScrollView,
  StatusBar,
  Text,
  View,
  useWindowDimensions,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { stylex } from "@uniflowed/stylex/native";

import type { ImageSourcePropType } from "react-native";

import { UNDERLINED } from "../../_shared/commonplace.stylex.js";
import { POSTERS } from "../../_shared/media.js";

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

const two = (value: number): string => String(value).padStart(2, "0");

/**
 * One clip is one screen: its poster edge to edge, and its caption over the bottom of it, on a
 * band dark enough to read against a bright sky. A band rather than a gradient, which would be
 * another dependency.
 */

component ClipSlide(clip: Clip, index: number, count: number, height: number) {
  return (
    <View
      accessibilityLabel={`${index + 1} of ${count}: ${clip.title}`}
      style={[stylex.props(local.slide).style, { height }]}
    >
      <Image source={clip.poster} resizeMode="cover" {...stylex.props(local.poster)} />
      <View {...stylex.props(local.caption)}>
        <Text {...stylex.props(local.title)}>{clip.title}</Text>
        <Text {...stylex.props(local.description)}>{clip.description}</Text>
        <Pressable
          accessibilityRole="link"
          accessibilityLabel={`Source of ${clip.title}, by ${clip.credit}`}
          hitSlop={8}
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
  );
}

/** The pager takes slides and nothing else, so every page of it is a whole clip. */

component ClipPager(children: renders* ClipSlide) {
  return <>{children}</>;
}

/** Previous and next, for a thumb that would rather press than swipe, and for a screen reader. */

component Step(label: string, glyph: string, disabled: boolean, onPress: () => void) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ disabled }}
      disabled={disabled}
      hitSlop={6}
      onPress={onPress}
      {...stylex.props(local.step, disabled && local.stepDisabled)}
    >
      <Text {...stylex.props(local.stepGlyph)}>{glyph}</Text>
    </Pressable>
  );
}

/**
 * Clips the way a phone shows short video: one to a screen, edge to edge and under the status
 * bar, paged vertically. The scroll view pages by its own height, so a slide is exactly as tall
 * as the space above the tab bar — measured once it is laid out, and the window's height until
 * then. The heading, the counter, the steps and the rail of marks sit over the clip and follow
 * the page the swipe settled on, as the web's counter and buttons follow its scroll snapping.
 */

export component Page() {
  const insets = useSafeAreaInsets();
  const window = useWindowDimensions();
  const pager = useRef<React.ElementRef<typeof ScrollView> | null>(null);
  const [measured, setMeasured] = useState<number | null>(null);
  const [selected, setSelected] = useState(0);
  const height = measured ?? window.height;

  const move = (index: number) => {
    if (index < 0 || index >= CLIPS.length) return;
    setSelected(index);
    pager.current?.scrollTo({ y: index * height, animated: true });
  };

  return (
    <View {...stylex.props(local.screen)}>
      <StatusBar barStyle="light-content" />
      <ScrollView
        ref={pager}
        pagingEnabled
        showsVerticalScrollIndicator={false}
        accessibilityLabel="Clips. Swipe up or down to change clip."
        onLayout={(event) => setMeasured(event.nativeEvent.layout.height)}
        onMomentumScrollEnd={(event) =>
          setSelected(
            Math.max(
              0,
              Math.min(CLIPS.length - 1, Math.round(event.nativeEvent.contentOffset.y / height)),
            ),
          )
        }
      >
        <ClipPager>
          {CLIPS.map((clip, index) => (
            <ClipSlide
              key={clip.id}
              clip={clip}
              index={index}
              count={CLIPS.length}
              height={height}
            />
          ))}
        </ClipPager>
      </ScrollView>
      <View
        pointerEvents="box-none"
        style={[stylex.props(local.bar).style, { paddingTop: insets.top + 12 }]}
      >
        <Text {...stylex.props(local.heading)}>Clips</Text>
        <Text accessibilityLiveRegion="polite" {...stylex.props(local.counter)}>
          {two(selected + 1)}
          <Text {...stylex.props(local.counterTotal)}> / {two(CLIPS.length)}</Text>
        </Text>
      </View>
      <View pointerEvents="box-none" {...stylex.props(local.rail)}>
        <Step
          label="Previous clip"
          glyph="↑"
          disabled={selected === 0}
          onPress={() => move(selected - 1)}
        />
        <View {...stylex.props(local.marks)}>
          {CLIPS.map((clip, index) => (
            <View
              key={clip.id}
              {...stylex.props(local.mark, index === selected && local.markSelected)}
            />
          ))}
        </View>
        <Step
          label="Next clip"
          glyph="↓"
          disabled={selected === CLIPS.length - 1}
          onPress={() => move(selected + 1)}
        />
      </View>
    </View>
  );
}

const local = stylex.create({
  screen: { flex: 1, backgroundColor: "#000000" },
  slide: { width: "100%", backgroundColor: "#0b0b0b", overflow: "hidden" },
  poster: { position: "absolute", top: 0, left: 0, width: "100%", height: "100%" },
  caption: {
    position: "absolute",
    left: 0,
    right: 0,
    bottom: 0,
    gap: 5,
    paddingLeft: 20,
    paddingRight: 76,
    paddingTop: 18,
    paddingBottom: 24,
    backgroundColor: "#00000073",
  },
  title: { fontSize: 22, fontWeight: "600", letterSpacing: -0.5, color: "#ffffff" },
  description: { fontSize: 14, lineHeight: 21, color: "#ffffffe0" },
  credit: { fontSize: 11, color: "#ffffffb8", marginTop: 6 },
  // Dark behind the status bar as well: its text is light, and a poster can be a bright sky.
  bar: {
    position: "absolute",
    top: 0,
    left: 0,
    right: 0,
    flexDirection: "row",
    alignItems: "baseline",
    justifyContent: "space-between",
    paddingLeft: 20,
    paddingRight: 20,
    paddingBottom: 12,
    backgroundColor: "#00000059",
  },
  heading: { fontSize: 24, fontWeight: "600", letterSpacing: -0.7, color: "#ffffff" },
  counter: { fontSize: 13, fontWeight: "600", color: "#ffffff" },
  counterTotal: { fontWeight: "400", color: "#ffffffa6" },
  rail: {
    position: "absolute",
    right: 14,
    top: 0,
    bottom: 0,
    alignItems: "center",
    justifyContent: "center",
    gap: 14,
  },
  step: {
    width: 42,
    height: 42,
    borderRadius: 21,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: "#00000066",
    borderWidth: 1,
    borderColor: "#ffffff33",
  },
  stepDisabled: { opacity: 0.35 },
  stepGlyph: { fontSize: 17, color: "#ffffff" },
  marks: { gap: 6, alignItems: "center" },
  mark: { width: 3, height: 14, borderRadius: 2, backgroundColor: "#ffffff59" },
  markSelected: { height: 26, backgroundColor: "#ffffff" },
});
