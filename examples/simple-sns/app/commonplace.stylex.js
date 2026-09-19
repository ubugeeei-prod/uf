// @flow
import { props, stylex, type StyleArgument, type StyleProps } from "@uniflowed/stylex";

// Values come directly from Commonplace's original CSS. Resets, descendant
// selectors, responsive layout and media state remain in base.css.
export const styles = stylex.create({
  srOnly: {
    position: "absolute",
    width: "1px",
    height: "1px",
    padding: "0",
    overflow: "hidden",
    clipPath: "inset(50%)",
    whiteSpace: "nowrap",
  },
  appShell: {
    maxWidth: "1600px",
    margin: "auto",
    "--line": "#dcdcdc",
  },
  workspaceLabel: {
    display: "flex",
    alignItems: "center",
    gap: "6px",
    fontSize: "11px",
    color: "var(--muted)",
    margin: "14px 0 32px",
  },
  onlineDot: {
    width: "4px",
    height: "4px",
    borderRadius: "50%",
    background: "#989898",
  },
  discoveryFooter: {
    display: "grid",
    gap: "5px",
    lineHeight: "1.7",
    color: "var(--muted)",
    fontSize: "10px",
    paddingInline: "4px",
  },
  emptyIcon: {
    color: "#868686",
    width: "48px",
    height: "48px",
    borderRadius: "10px",
    background: "#efefef",
    display: "grid",
    placeItems: "center",
  },
  feedToolbar: {
    borderBottom: "1px solid var(--line)",
  },
  resultLabel: {
    fontSize: "12px",
    color: "var(--muted)",
    marginTop: "18px",
  },
  postContent: {
    minWidth: "0",
    flex: "1",
  },
  postFooter: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: "14px",
  },
  postError: {
    fontSize: "11px",
    color: "#b13749",
    marginTop: "8px",
  },
  conversationPrivate: {
    marginLeft: "auto",
    color: "#989898",
  },
  authIntro: {
    fontSize: "12px",
    color: "var(--muted)",
    lineHeight: "1.7",
    marginBottom: "26px",
  },
  authNote: {
    fontSize: "10px",
    lineHeight: "1.8",
    color: "var(--muted)",
    marginTop: "19px",
    textAlign: "center",
  },
  composeRegion: {
    scrollMarginTop: "85px",
  },
  skeletonInk: {
    background: "#2424240d",
    borderRadius: "2px",
  },
  skeletonAuthor: {
    width: "92px",
    height: "11px",
    marginBlock: "4px",
  },
  skeletonHandle: {
    width: "48px",
    height: "9px",
  },
  skeletonTime: {
    width: "40px",
    height: "9px",
    marginLeft: "auto",
  },
  skeletonTag: {
    height: "8px",
    width: "54px",
    marginBlock: "10px",
  },
  skeletonReaction: {
    height: "10px",
    width: "28px",
  },
  skeletonFieldLabel: {
    height: "8px",
    width: "80px",
    marginBlock: "6px",
  },
  skeletonSectionTitle: {
    width: "105px",
    height: "11px",
    marginBlock: "4px 18px",
  },
  skeletonSectionCopy: {
    width: "230px",
    maxWidth: "85%",
    height: "7px",
    marginBottom: "28px",
  },
  skeletonButton: {
    width: "107px",
    height: "36px",
    borderRadius: "6px",
  },
  avatarPhoto: {
    position: "absolute",
    inset: "0",
    width: "100%",
    height: "100%",
    objectFit: "cover",
    borderRadius: "inherit",
  },
  avatarFallback: {
    display: "grid",
    placeItems: "center",
    width: "100%",
    height: "100%",
  },
  clipsPlayer: {
    position: "relative",
    minWidth: "0",
  },
  clipSlide: {
    height: "100%",
    width: "100%",
    scrollSnapAlign: "start",
    scrollSnapStop: "always",
  },
});

/** Keep semantic classes used by existing state and responsive selectors. */
export function styled(className: string, ...styles: Array<StyleArgument>): StyleProps {
  const result = props(...styles);
  return { ...result, className: `${className} ${result.className ?? ""}` };
}
