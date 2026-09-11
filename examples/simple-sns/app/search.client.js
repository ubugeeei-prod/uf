"use client";
// @flow

import * as React from "@uniflowed/react";
import { startTransition } from "@uniflowed/react";
import { Link, useRouter } from "@uniflowed/router";
import { promise, runPromiseExit } from "@uniflowed/effect";
import { Icon } from "./ui.js";
import { feedHref, type FeedFilter } from "./social-model.js";

/**
 * Enhance a native GET search with router navigation; failed navigation falls back to the same URL.
 */
export component SearchNotes(filter: FeedFilter) {
  const router = useRouter();

  return (
    <form
      className="feed-search"
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
        name="q"
        defaultValue={filter.query}
        placeholder="Search notes and people"
        aria-label="Search notes"
        maxLength={100}
      />
      {filter.topic !== "all" ? <input type="hidden" name="topic" value={filter.topic} /> : null}
      <button type="submit">Search</button>
      {filter.query ? (
        <Link to={feedHref(filter.topic)} className="text-link">
          Clear
        </Link>
      ) : null}
    </form>
  );
}
