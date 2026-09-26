// @flow
"use client";

import { useState } from "@uniflowed/react";
import { useAtomValue, useSetAtom } from "@uniflowed/state";
import { addTask, clearCompleted, completed, tasks, toggleTask } from "./model.js";

export component Page() {
  const items = useAtomValue(tasks);
  const done = useAtomValue(completed);
  const add = useSetAtom(addTask);
  const toggle = useSetAtom(toggleTask);
  const clear = useSetAtom(clearCompleted);
  const [draft, setDraft] = useState("");

  return (
    <>
      <p className="eyebrow">A little room for today</p>
      <h1>Make room for what matters.</h1>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          add(draft);
          setDraft("");
        }}
      >
        <label htmlFor="task">Add a task</label>
        <div className="compose">
          <input
            id="task"
            name="task"
            value={draft}
            maxLength={160}
            autoComplete="off"
            onChange={(event) => setDraft(event.currentTarget.value)}
            placeholder="What is next?"
          />
          <button type="submit" disabled={draft.trim().length === 0}>
            Add task
          </button>
        </div>
      </form>
      <ul className="tasks" aria-label="Tasks">
        {items.map((task) => (
          <li key={task.id}>
            <label>
              <input type="checkbox" checked={task.done} onChange={() => toggle(task.id)} />
              <span className={task.done ? "done" : undefined}>{task.title}</span>
            </label>
          </li>
        ))}
      </ul>
      {items.length === 0 && (
        <p className="quiet">Nothing waiting. Add something when you are ready.</p>
      )}
      <div className="summary">
        <p role="status">
          {done} of {items.length} complete
        </p>
        <button type="button" className="subtle" disabled={done === 0} onClick={() => clear()}>
          Clear completed
        </button>
      </div>
    </>
  );
}
