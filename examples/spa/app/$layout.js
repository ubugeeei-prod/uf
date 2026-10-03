// @flow
"use client";

import * as React from "@uniflowed/react";
import { Link, useRoute } from "@uniflowed/router";
import type { Metadata } from "@uniflowed/router";
import { props, stylex } from "@uniflowed/stylex";

import { dark, focus, tokens } from "./tokens.stylex.js";

import "./board.css";

export const metadata: Metadata = {
  title      : "Taskboard",
  description: "A little room for today's tasks.",
};

export component Layout(children: React.Node) {
  const { pathname } = useRoute();
  return (
    <div {...props(styles.viewport, dark)}>
      <a {...props(styles.skip, focus.ring)} href="#content">
        Skip to content
      </a>
      <div {...props(styles.shell)}>
        <header {...props(styles.masthead)}>
          <Link {...props(styles.brand, focus.ring)} to="/">
            uf <span {...props(styles.brandMark)}>Taskboard</span>
          </Link>
          <nav {...props(styles.nav)} aria-label="Pages">
            <Link
              {...props(styles.navLink, focus.ring)}
              to="/"
              aria-current={
                match (pathname) {
                  "/" => "page",
                  _   => undefined,
                }
              }
            >
              Board
            </Link>
            <Link
              {...props(styles.navLink, focus.ring)}
              to="/progress"
              aria-current={
                match (pathname) {
                  "/progress" => "page",
                  _           => undefined,
                }
              }
            >
              Progress
            </Link>
          </nav>
        </header>
        <main {...props(styles.main)} id="content">
          {children}
        </main>
        <footer {...props(styles.footer)}>
          Your tasks stay here while you move between pages. Reload to start fresh.
        </footer>
      </div>
    </div>
  );
}

const styles = stylex.create({
  viewport: {
    minHeight      : "100vh",
    backgroundColor: tokens.paper,
    color          : tokens.ink,
    colorScheme    : "light dark",
    fontFamily     : "Inter, ui-sans-serif, system-ui, sans-serif",
    fontSize       : "16px",
    lineHeight     : "1.6",
  },
  shell: {
    maxWidth: "820px",
    margin  : "0 auto",
    padding : "0 clamp(1.25rem, 4vw, 3rem)",
  },
  skip: {
    position           : "absolute",
    top                : "-5rem",
    backgroundColor    : tokens.paper,
    color              : tokens.accent,
    textUnderlineOffset: "0.2em",
    padding            : "0.5rem",
    zIndex             : "2",
    ":focus"           : { top: "0.5rem" },
  },
  masthead: {
    minHeight        : "88px",
    display          : "flex",
    alignItems       : "center",
    justifyContent   : "space-between",
    gap              : "1rem",
    borderBottomWidth: "1px",
    borderBottomStyle: "solid",
    borderBottomColor: tokens.rule,
  },
  brand: {
    color         : tokens.ink,
    fontSize      : "24px",
    fontWeight    : "700",
    textDecoration: "none",
  },
  brandMark: {
    marginLeft: "0.5rem",
    color     : tokens.muted,
    fontSize  : "14px",
    fontWeight: "400",
  },
  nav: {
    display: "flex",
    gap    : "1.25rem",
  },
  navLink: {
    color              : tokens.muted,
    textDecoration     : "none",
    fontSize           : "14px",
    textUnderlineOffset: "0.2em",
    ":is([aria-current=page])": {
      color         : tokens.ink,
      textDecoration: "underline",
    },
  },
  main: {
    position       : "relative",
    minHeight      : "65vh",
    margin         : "3rem 0",
    paddingLeft    : "clamp(1rem, 4vw, 2rem)",
    borderLeftWidth: "1px",
    borderLeftStyle: "solid",
    borderLeftColor: tokens.rule,
    "::before": {
      content   : "''",
      position  : "absolute",
      top       : "0",
      left      : "-1px",
      width     : "2px",
      height    : "96px",
      background: "linear-gradient(#35d6f6, #2677ff, #8f4bff, transparent)",
    },
  },
  footer: {
    padding       : "1.5rem 0",
    borderTopWidth: "1px",
    borderTopStyle: "solid",
    borderTopColor: tokens.rule,
    color         : tokens.muted,
    fontSize      : "12px",
  },
});
