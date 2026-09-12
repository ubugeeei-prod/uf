// @flow
//
// Theme constants shared by the document and the client toggle.

export type Theme = "system" | "light" | "dark";

export const STORAGE_KEY: string = "uf-docs-theme";

/**
 * The script that runs before first paint.
 *
 * It has to be inline and synchronous: a stored dark preference applied after
 * the first frame is a white flash, and there is no CSS that can express
 * "read localStorage". Kept to one statement so it can be inlined safely.
 */
export const themeBootstrap: string =
  `try{var t=localStorage.getItem(${JSON.stringify(STORAGE_KEY)});` +
  `if(t==="dark"||t==="light")document.documentElement.dataset.theme=t}catch(e){}`;

/** The theme after this one, cycling system -> light -> dark -> system. */
export function nextTheme(theme: Theme): Theme {
  return match (theme) {
    "system" => "light",
    "light" => "dark",
    "dark" => "system",
  };
}

/** What the toggle should say it will do. */
export function themeLabel(theme: Theme): string {
  return match (theme) {
    "system" => "Theme: system",
    "light" => "Theme: light",
    "dark" => "Theme: dark",
  };
}
