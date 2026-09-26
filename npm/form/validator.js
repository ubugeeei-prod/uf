// @flow
//
// `@uniflowed/form/validator`: `@uniflowed/validator` as a form's resolver.
//
//   const account = object({
//     email: pipe(string(), email()),
//     age: pipe(string(), transform(Number), min(18)),
//   });
//
//   const { register, handleSubmit } = useForm({
//     resolver: validatorResolver(account),
//     defaultValues: { email: "", age: "" },
//   });
//
//   // `values.age` is a number here, because the schema said so.
//   handleSubmit((values) => save(values));
//
// # Why this is a separate entry point
//
// So that a form that uses a different schema library — or none — never
// resolves `@uniflowed/validator` at all. The adapter is fifty lines and the
// schema library is not, and an application that writes its rules inline should
// not carry one because the form package happened to ship the other.
//
// # Why the two type parameters
//
// `Schema<TOut>` is a parser: it takes `mixed` and produces `TOut`. A form's
// values are what the *controls* produced — `{ age: "42" }` from a text box —
// and the schema's output is what the application wanted, `{ age: 42 }`. Those
// are two types, and keeping them apart is the point of the resolver contract:
// `handleSubmit` hands `onValid` the schema's output, so the coercion survives
// to the submit handler and across a Server Action boundary, rather than being
// redone by hand at each end.
//
// # How an issue becomes a field error
//
// `@uniflowed/validator` reports a path as segments — `["items", "2", "email"]`
// — and a form addresses fields with the dotted string `register` was given, so
// the segments are joined. That is the whole translation, and it works because
// both libraries describe the same tree the same way. An issue with no path is
// about the whole value, and the whole value's path is the empty string, so it
// arrives as `errors[""]`.
//
// The issue's `code` becomes the error's `type`, which is what lets a caller
// tell "this is not an email address" from "we need an email address" without
// matching on the message.

import type { Issue, Schema } from "@uniflowed/validator";
import { isAsync, safeParse, safeParseAsync } from "@uniflowed/validator";

import type { Resolver, ResolverErrors, ResolverResult } from "./resolver.js";
import { collectErrors } from "./resolver.js";
import type { FieldValues } from "./internal/field-path.js";

/**
 * Field errors for a list of validator issues.
 *
 * Exported because the same translation is what a Server Action needs when it
 * validates the same schema again on the server and sends the failures back:
 * feed the issues through this and the keys line up with the fields the form
 * already has, so `setError` can put each message where it belongs.
 */
export function errorsFromIssues(issues: $ReadOnlyArray<Issue>): ResolverErrors {
  return collectErrors(
    issues.map((issue) => ({
      path: issue.path == null ? "" : issue.path.join("."),
      type: issue.code,
      message: issue.message,
    })),
  );
}

/**
 * A resolver that validates the form against `schema`.
 *
 * Synchronous when the schema is, which is the case a form in `onChange` mode
 * runs on every keystroke: a promise there would cost a microtask and a pair of
 * `isValidating` renders for an answer that was already available. The
 * `Resolver` contract allows either, and `runResolver` returns a synchronous
 * answer synchronously, so nothing downstream pays for the choice.
 *
 * A schema with a `checkAsync` in it — "is this handle taken" — is
 * asynchronous from that step up, and `@uniflowed/validator` says so through
 * `isAsync` when the schema is built rather than when it is run. So the branch
 * is taken once, here, and not per keystroke; and `safeParse` never gets a
 * schema it would refuse. The form already handles an asynchronous resolver,
 * including discarding a slower answer that a newer one has overtaken.
 */
export function validatorResolver<TValues extends FieldValues, TOutput>(
  schema: Schema<TOutput>,
): Resolver<TValues, TOutput> {
  if (isAsync(schema)) {
    return async (values: TValues): Promise<ResolverResult<TOutput>> => {
      const result = await safeParseAsync(schema, values);
      if (result.ok) {
        return { values: result.value };
      }
      return { errors: errorsFromIssues(result.issues) };
    };
  }
  return (values: TValues): ResolverResult<TOutput> => {
    const result = safeParse(schema, values);
    if (result.ok) {
      return { values: result.value };
    }
    return { errors: errorsFromIssues(result.issues) };
  };
}
