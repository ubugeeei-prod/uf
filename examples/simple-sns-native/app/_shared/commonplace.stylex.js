// @flow
// The web Commonplace examples' visual language, in React Native's terms: the
// same ink, muted and rule colours, 6 px corners, hairline rules between rows
// rather than cards, and a 32 px medium heading.

import { stylex } from "@uniflowed/stylex/native";

/** For the few values React Navigation takes as options rather than as styles. */

export const INK = "#242424";
export const FAINT = "#868686";

/**
 * Two properties outside StyleX native's compiled subset, given to React Native as they are,
 * beside the compiled style: a link's underline, and a multiline input that starts at its top
 * on Android, where it would otherwise be centred.
 */

export const UNDERLINED: {| readonly textDecorationLine: "underline" |} = {
  textDecorationLine: "underline",
};
export const TOP_ALIGNED: {| readonly textAlignVertical: "top" |} = { textAlignVertical: "top" };

export const styles = stylex.create({
  page: { flex: 1, backgroundColor: "#f8f8f8" },
  content: { paddingLeft: 20, paddingRight: 20, paddingTop: 18, paddingBottom: 40 },

  topbar: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    paddingBottom: 18,
  },
  brand: { fontSize: 15, fontWeight: "600", letterSpacing: -0.3, color: "#242424" },
  topbarAction: { fontSize: 12, color: "#707070" },

  heading: {
    fontSize: 32,
    lineHeight: 40,
    fontWeight: "500",
    letterSpacing: -1.1,
    color: "#242424",
  },
  lede: { fontSize: 13, lineHeight: 21, color: "#707070", marginTop: 6 },
  sectionTitle: { fontSize: 15, fontWeight: "600", color: "#242424" },
  hint: { fontSize: 12, lineHeight: 19, color: "#707070" },

  rule: { borderBottomWidth: 1, borderBottomColor: "#dcdcdc" },
  row: { flexDirection: "row", alignItems: "center" },
  spread: { flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 18 },
  grow: { flex: 1, minWidth: 0 },

  button: {
    minHeight: 38,
    paddingLeft: 15,
    paddingRight: 15,
    borderRadius: 6,
    borderWidth: 1,
    borderColor: "#e3e3e3",
    backgroundColor: "#ffffff",
    alignItems: "center",
    justifyContent: "center",
  },
  buttonPrimary: { backgroundColor: "#202020", borderColor: "#202020" },
  buttonDisabled: { opacity: 0.55 },
  buttonLabel: { fontSize: 12, fontWeight: "600", color: "#242424" },
  buttonLabelPrimary: { color: "#ffffff" },

  textLink: { fontSize: 12, fontWeight: "600", color: "#242424" },

  field: { gap: 8, marginBottom: 19 },
  fieldLabel: { fontSize: 12, fontWeight: "600", color: "#5f5f5f" },
  alert: { fontSize: 12, lineHeight: 18, color: "#b13749", marginTop: 4 },
  status: { fontSize: 12, lineHeight: 18, color: "#707070", marginTop: 4 },

  avatar: {
    width: 42,
    height: 42,
    borderRadius: 21,
    backgroundColor: "#ffffff",
    borderWidth: 1,
    borderColor: "#dcdcdc",
    alignItems: "center",
    justifyContent: "center",
    overflow: "hidden",
  },
  avatarSmall: { width: 34, height: 34, borderRadius: 17 },
  avatarInitials: { fontSize: 12, fontWeight: "600", letterSpacing: -0.2, color: "#3b3b3b" },
  avatarInitialsSmall: { fontSize: 11 },

  channelDot: { width: 6, height: 6, borderRadius: 3, backgroundColor: "#a0a0a0" },

  empty: {
    alignItems: "center",
    gap: 10,
    paddingTop: 48,
    paddingBottom: 48,
    paddingLeft: 22,
    paddingRight: 22,
  },
  emptyTitle: { fontSize: 16, fontWeight: "600", color: "#242424", textAlign: "center" },
  emptyBody: { fontSize: 13, lineHeight: 21, color: "#707070", textAlign: "center" },
});
