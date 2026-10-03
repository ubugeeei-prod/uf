"use client";
// @flow

import * as React from "@uniflowed/react";
import { startTransition } from "@uniflowed/react";
import { Link, useRouter } from "@uniflowed/router";
import { promise, runPromiseExit } from "@uniflowed/effect";

import { props, stylex } from "@uniflowed/stylex";

import { styles as controlStyles } from "../_shared/form-ui.client.js";
import { Icon } from "../_shared/ui.js";
import { feedHref, type FeedFilter } from "../_shared/social-model.js";

/**
 * Enhance a native GET search with router navigation; failed navigation falls back to the same URL.
 */

export component SearchNotes(filter: FeedFilter) {
  const router = useRouter();

  return (
    <form
      {...props(styles.feedSearch)}
      role="search"
      method="get"
      action="/"
      onSubmit={(event) => {
        event.preventDefault();
        const query = String(new FormData(event.currentTarget).get("q") ?? "").trim();
        const href = feedHref(filter.topic, query);
        startTransition(async () => {
          const result = await runPromiseExit(promise(() => router.push(href)));

          if (result.kind === "failure") {
            window.location.assign(href);
          }
        });
      }}
    >
      <Icon name="search" size={15} />
      <input
        {...props(styles.feedSearchInput)}
        name="q"
        defaultValue={filter.query}
        placeholder="Search notes and people"
        aria-label="Search notes"
        maxLength={100}
      />
      {
        match (filter.topic) {
          "all"       => null,
          const topic => <input type="hidden" name="topic" value={topic} />,
        }
      }
      <button type="submit" {...props(styles.feedSearchButton)}>
        Search
      </button>
      {
        match (filter.query) {
          "" => null,
          _  =>
            <Link to={feedHref(filter.topic)} {...props(controlStyles.textLink)}>
              Clear
            </Link>,
        }
      }
    </form>
  );
}

const styles = stylex.create({
  feedSearch: {
    display      : "flex",
    alignItems   : "center",
    gap          : "6px",
    borderBottom : "1px solid var(--line)",
    color        : "#8a8a8a",
    paddingTop   : { default: "8px", "@media (max-width: 760px)": "0" },
    paddingRight : "0",
    paddingBottom: { default: "8px", "@media (max-width: 760px)": "0" },
    paddingLeft  : "0",
    minHeight    : { "@media (max-width: 760px)": "44px" },
  },
  feedSearchInput: {
    fontSize     : { default: "12px", "@media (max-width: 760px)": "16px" },
    flex         : "1",
    border       : "0",
    background   : "transparent",
    outlineOffset: "0",
    color        : "var(--ink)",
    paddingTop   : { default: "8px", "@media (max-width: 760px)": "10px" },
    paddingRight : "8px",
    paddingBottom: { default: "8px", "@media (max-width: 760px)": "10px" },
    paddingLeft  : "8px",
    "::placeholder": {
      fontSize: "12px",
    },
  },
  feedSearchButton: {
    border       : "0",
    background   : "transparent",
    fontSize     : "11px",
    color        : "var(--muted)",
    paddingTop   : "8px",
    paddingRight : "8px",
    paddingBottom: "8px",
    paddingLeft  : "8px",
    minHeight    : { "@media (max-width: 760px)": "44px" },
    ":hover": {
      color: "var(--ink)",
    },
  },
});
