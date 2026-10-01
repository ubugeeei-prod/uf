// @flow
"use client";

import { Link } from "@uniflowed/router";
import { useAtomValue } from "@uniflowed/state";
import { completed, tasks } from "../model.js";

export component Page() {
  const total = useAtomValue(tasks).length;
  const done  = useAtomValue(completed);
  return (
    <>
      <p className="eyebrow">A little at a time</p>
      <h1>Your progress.</h1>
      <p className="progress-count">
        {done}
        <span> / {total}</span>
      </p>
      <p>
        {
          match (total === 0) {
            true  => "A clear board. Start whenever you are ready.",
            false => `${total - done} tasks left to make a little space.`,
          }
        }
      </p>
      <progress value={done} max={Math.max(total, 1)} aria-label="Completed tasks" />
      <p>
        <Link to="/">Back to the board</Link>
      </p>
    </>
  );
}
