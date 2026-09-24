// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import * as Field from "./field.js";

const styles = stylex.create({
  form: {
    display: "grid",
    gap: ufTokens.space4,
    maxWidth: "24rem",
  },
});

/** A field with help, one that is invalid and says why, and a text area. */
export component Example() {
  return (
    <form {...props(styles.form)}>
      <Field.Root required>
        <Field.Label>Name</Field.Label>
        <Field.Input autoComplete="name" defaultValue="Ada Lovelace" name="name" />
        <Field.Description>As it should appear on receipts.</Field.Description>
      </Field.Root>
      <Field.Root invalid required>
        <Field.Label>Email</Field.Label>
        <Field.Input autoComplete="email" defaultValue="ada@example" name="email" type="email" />
        <Field.Error>Enter an address with a domain, like ada@example.com.</Field.Error>
      </Field.Root>
      <Field.Root>
        <Field.Label>Note</Field.Label>
        <Field.Textarea name="note" placeholder="Anything we should know" />
      </Field.Root>
    </form>
  );
}
