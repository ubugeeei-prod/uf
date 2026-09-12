"use client";
// @flow
//
// Light and dark.
//
// Three states, not two: "system" is the default and follows the operating
// system, and the two explicit choices are stored so a reader who prefers
// dark on a light machine keeps it. The stored choice is written to
// `data-theme` on the root element, which every rule in `seam.css` keys off.
//
// This is the first `"use client"` in this repository, and it is here because
// the module is honest about what it needs: `useEffect`, `localStorage` and
// `document.documentElement` are the browser's, and a module that reaches for
// them is a client module whether or not it says so. `uf build` used to count
// the five resulting contract violations and print the number; now it reports
// them and fails, so the boundary has to be declared rather than tolerated.
// See ubugeeei-prod/uf#281.

import { useEffect, useState } from "@uniflowed/react";

import { nextTheme, STORAGE_KEY, themeLabel } from "./theme-static.js";
import type { Theme } from "./theme-static.js";

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

export component ThemeToggle() {
  const [theme, setTheme] = useTheme();

  return (
    <button className="theme-toggle" type="button" onClick={() => setTheme(nextTheme(theme))}>
      {themeLabel(theme)}
    </button>
  );
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
  if (root == null) {
    return;
  }
  if (theme === "system") {
    delete root.dataset.theme;
  } else {
    root.dataset.theme = theme;
  }
}
