// @flow
"use client";

import * as React from "@uniflowed/react";
import { Link, useRoute } from "@uniflowed/router";
import type { Metadata } from "@uniflowed/router";

import "./board.css";

export const metadata: Metadata = {
  title: "Taskboard",
  description: "A little room for today's tasks.",
};

export component Layout(children: React.Node) {
  const { pathname } = useRoute();
  return (
    <div className="shell">
      <a className="skip" href="#content">
        Skip to content
      </a>
      <header className="masthead">
        <Link className="brand" to="/">
          uf <span>Taskboard</span>
        </Link>
        <nav aria-label="Pages">
          <Link to="/" aria-current={pathname === "/" ? "page" : undefined}>
            Board
          </Link>
          <Link to="/progress" aria-current={pathname === "/progress" ? "page" : undefined}>
            Progress
          </Link>
        </nav>
      </header>
      <main id="content">{children}</main>
      <footer>Your tasks stay here while you move between pages. Reload to start fresh.</footer>
    </div>
  );
}
