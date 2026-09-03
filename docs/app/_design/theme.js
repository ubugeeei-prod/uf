// @flow
//
// Light and dark.
//
// Three states, not two: "system" is the default and follows the operating
// system, and the two explicit choices are stored so a reader who prefers
// dark on a light machine keeps it. The stored choice is written to
// `data-theme` on the root element, which every rule in `seam.css` keys off.

import { useEffect, useState } from "@uniflowed/react";

export type Theme = "system" | "light" | "dark";

const STORAGE_KEY = "uf-docs-theme";

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

/**
 * The current theme and a way to change it.
 *
 * Starts at "system" on the server and on the first client render, because
 * the server cannot know the stored value and hydrating with a different one
 * would be a mismatch. The stored value is adopted in an effect, one frame
 * later — by which time the bootstrap script has already painted it.
 */
export hook useTheme(): [Theme, (next: Theme) => void] {
  const [theme, setTheme] = useState<Theme>("system");

  useEffect(() => {
    setTheme(stored());
  }, []);

  const choose = (next: Theme) => {
    setTheme(next);
    apply(next);
    try {
      if (next === "system") {
        localStorage.removeItem(STORAGE_KEY);
      } else {
        localStorage.setItem(STORAGE_KEY, next);
      }
    } catch {
      // A browser with storage blocked still gets the theme for this page.
    }
  };

  return [theme, choose];
}

/** The theme after this one, cycling system → light → dark → system. */
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

function stored(): Theme {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    if (value === "dark" || value === "light") {
      return value;
    }
  } catch {
    // Fall through to the system preference.
  }
  return "system";
}

function apply(theme: Theme): void {
  const root = document.documentElement;
  if (theme === "system") {
    delete root.dataset.theme;
  } else {
    root.dataset.theme = theme;
  }
}
