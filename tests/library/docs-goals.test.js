// @flow
//
// The goals map (#1421), checked against the manual it points into.
//
// Each step is a link a reader follows without knowing the manual's layout,
// so every one has to land on a page the manual lists; a page that moves
// breaks this test rather than a reader's path. And a status is a claim, so
// the two that are not "it works" have to say why.

import { describe, expect, it } from "@uniflowed/test";

import { goals } from "../../docs/app/_design/goals.js";
import { entryFor } from "../../docs/app/_design/nav.js";

describe("the goals map", () => {
  it("links every step to a page the manual lists", () => {
    const unlisted = [];
    for (const goal of goals) {
      for (const step of goal.steps) {
        const route = step.href.split("#")[0];
        if (entryFor(route) == null) {
          unlisted.push(`${goal.title}: ${step.href}`);
        }
      }
    }
    expect(unlisted).toEqual([]);
  });

  it("says why a goal is not implemented, and only then", () => {
    for (const goal of goals) {
      expect({ goal: goal.title, caveat: goal.caveat != null }).toEqual({
        goal: goal.title,
        caveat: goal.status !== "Implemented",
      });
    }
  });

  it("names the issue that tracks a planned goal", () => {
    for (const goal of goals.filter((candidate) => candidate.status === "Planned")) {
      expect(/#\d+/.test(goal.caveat ?? "")).toBe(true);
    }
  });

  it("gives every goal a path, and each goal once", () => {
    expect(goals.every((goal) => goal.steps.length > 0)).toBe(true);
    expect(new Set(goals.map((goal) => goal.title)).size).toBe(goals.length);
    expect(goals.some((goal) => goal.onHome)).toBe(true);
  });
});
