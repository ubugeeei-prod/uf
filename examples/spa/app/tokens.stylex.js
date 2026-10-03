// @flow

import { stylex, type CompiledStyle } from "@uniflowed/stylex";

export const tokens: {|
  readonly paper : string,
  readonly ink   : string,
  readonly muted : string,
  readonly rule  : string,
  readonly accent: string,
|} = stylex.defineVars({
  paper : "#fbfaf8",
  ink   : "#16171a",
  muted : "#5b6068",
  rule  : "#e0ddd6",
  accent: "#1a56d6",
});

/** Follows the reader's color scheme. Light mode keeps the declared tokens. */

export const dark: CompiledStyle = stylex.createTheme(tokens, {
  paper : { "@media (prefers-color-scheme: dark)": "#0c0d0f" },
  ink   : { "@media (prefers-color-scheme: dark)": "#e8e6e1" },
  muted : { "@media (prefers-color-scheme: dark)": "#9aa0a8" },
  rule  : { "@media (prefers-color-scheme: dark)": "#24272c" },
  accent: { "@media (prefers-color-scheme: dark)": "#7fb0ff" },
});

export const focus: {| readonly ring: CompiledStyle |} = stylex.create({
  ring: {
    ":focus-visible": {
      outlineWidth : "2px",
      outlineStyle : "solid",
      outlineColor : tokens.accent,
      outlineOffset: "4px",
    },
  },
});
