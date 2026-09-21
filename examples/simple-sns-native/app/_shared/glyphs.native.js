// @flow
// The web examples draw their navigation icons as SVG paths. This app adds no
// SVG dependency, so the same four ideas are drawn from views: a feed of lines,
// a clip with its play mark, a message with its lines, and two sliders.

import { View } from "react-native";
import { stylex } from "@uniflowed/stylex/native";

export type GlyphName = "feed" | "clips" | "inbox" | "settings";

export component Glyph(name: GlyphName, color: string) {
  const ink = { backgroundColor: color };
  const outline = { borderColor: color };

  if (name === "feed") {
    return (
      <View {...stylex.props(glyph.box, glyph.stack)}>
        <View style={[stylex.props(glyph.line).style, ink]} />
        <View style={[stylex.props(glyph.line, glyph.lineShort).style, ink]} />
        <View style={[stylex.props(glyph.line).style, ink]} />
      </View>
    );
  }

  if (name === "clips") {
    return (
      <View {...stylex.props(glyph.box)}>
        <View style={[stylex.props(glyph.frame, glyph.portrait).style, outline]}>
          <View style={[stylex.props(glyph.play).style, { borderLeftColor: color }]} />
        </View>
      </View>
    );
  }

  if (name === "inbox") {
    return (
      <View {...stylex.props(glyph.box)}>
        <View style={[stylex.props(glyph.frame, glyph.bubble).style, outline]}>
          <View style={[stylex.props(glyph.line, glyph.lineInset).style, ink]} />
          <View style={[stylex.props(glyph.line, glyph.lineInset, glyph.lineShort).style, ink]} />
        </View>
      </View>
    );
  }

  return (
    <View {...stylex.props(glyph.box, glyph.stack, glyph.sliders)}>
      <View style={[stylex.props(glyph.line).style, ink]}>
        <View style={[stylex.props(glyph.knob, glyph.knobLeft).style, ink]} />
      </View>
      <View style={[stylex.props(glyph.line).style, ink]}>
        <View style={[stylex.props(glyph.knob, glyph.knobRight).style, ink]} />
      </View>
    </View>
  );
}

const glyph = stylex.create({
  box: { width: 22, height: 22, alignItems: "center", justifyContent: "center" },
  stack: { gap: 4, alignItems: "flex-start", paddingLeft: 2, paddingRight: 2 },
  sliders: { gap: 8 },
  line: { width: 18, height: 2, borderRadius: 1 },
  lineShort: { width: 11 },
  lineInset: { width: 10, height: 1.5 },
  frame: { borderWidth: 1.6, alignItems: "center", justifyContent: "center" },
  portrait: { width: 15, height: 20, borderRadius: 3 },
  bubble: {
    width: 20,
    height: 16,
    borderRadius: 5,
    borderBottomLeftRadius: 1,
    gap: 3,
    alignItems: "flex-start",
    paddingLeft: 4,
  },
  play: {
    width: 0,
    height: 0,
    marginLeft: 2,
    borderTopWidth: 3.5,
    borderBottomWidth: 3.5,
    borderLeftWidth: 6,
    borderTopColor: "transparent",
    borderBottomColor: "transparent",
  },
  knob: { position: "absolute", top: -3, width: 2, height: 8, borderRadius: 1 },
  knobLeft: { left: 4 },
  knobRight: { right: 4 },
});
