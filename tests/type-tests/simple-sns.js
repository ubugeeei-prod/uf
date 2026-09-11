// @flow
import * as React from "@uniflowed/react";
import type { ActionResult, Session } from "../../examples/simple-sns/app/social-model.js";
import { PostList } from "../../examples/simple-sns/app/timeline-client.js";
import { FormField } from "../../examples/simple-sns/app/form-ui.client.js";
import { EmptyState, ActionLink } from "../../examples/simple-sns/app/ui.js";
// expect: value
export const missingValue: ActionResult<string> = { status: "success", message: "Saved" };
// expect: value
export const errorWithValue: ActionResult<string> = {
  status: "error",
  message: "No",
  fields: {},
  value: "wrong",
};
export component BadField() {
  const control = <input />;
  // expect: does not render
  return <FormField label="Name">{control}</FormField>;
}
export component BadList() {
  const child = <div />;
  // expect: does not render
  return <PostList>{child}</PostList>;
}
export component BadAction() {
  const action = <button>Wrong action</button>;
  return (
    // expect: does not render
    <EmptyState title="Empty" action={action}>
      No notes
    </EmptyState>
  );
}
export component GoodSlot() {
  return (
    <EmptyState title="Empty" action={<ActionLink to="/">Home</ActionLink>}>
      No notes
    </EmptyState>
  );
}
export function incomplete(session: Session): string {
  // expect: hasn't checked all possible cases
  return match (session) {
    {kind: "guest"} => "Guest",
  };
}
