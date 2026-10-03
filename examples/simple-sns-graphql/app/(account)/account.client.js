"use client";
// @flow

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { graphql, useMutation } from "@uniflowed/relay";
import { props } from "@uniflowed/stylex";

import { styles as uiStyles } from "../_shared/ui.js";

import type { SnsRegisterMutation } from "./__generated__/SnsRegisterMutation.graphql.js";
import type { SnsLoginMutation } from "./__generated__/SnsLoginMutation.graphql.js";

const registerMutation = graphql`
  mutation SnsRegisterMutation($input: RegisterInput!) {
    register(input: $input) {
      id
    }
  }
`;

const login = graphql`
  mutation SnsLoginMutation($handle: String!, $password: String!) {
    login(handle: $handle, password: $password) {
      id
    }
  }
`;

/** Credentials go to the upstream service; only its HttpOnly cookie returns. */

export component AccountForm(register: boolean) {
  const [signup, signingUp] = useMutation<
    SnsRegisterMutation["variables"],
    SnsRegisterMutation["response"],
  >(registerMutation);
  const [signin, signingIn] = useMutation<
    SnsLoginMutation["variables"],
    SnsLoginMutation["response"],
  >(login);
  const [error,  setError]  = useState("");
  const pending = signingUp || signingIn;

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        setError("");
        const values = new FormData(event.currentTarget);
        const handle = String(values.get("handle"));
        const password = String(values.get("password"));
        const callbacks = {
          onCompleted: () => window.location.assign("/"),
          onError    : (failure: Error) => setError(failure.message),
        };
        if (register)
          signup({
            ...callbacks,
            variables: {
              input: {
                name : String(values.get("name")),
                email: String(values.get("email")),
                handle,
                password,
              },
            },
          });
        else signin({ ...callbacks, variables: { handle, password } });
      }}
    >
      {
        match (register) {
          true  =>
            <label {...props(uiStyles.field)}>
              <span {...props(uiStyles.fieldLabel)}>Display name</span>
              <input {...props(uiStyles.fieldControl)} name="name" autoComplete="name" required maxLength={80} />
            </label>,
          false => null,
        }
      }
      <label {...props(uiStyles.field)}>
        <span {...props(uiStyles.fieldLabel)}>Handle</span>
        <input
          {...props(uiStyles.fieldControl)}
          name="handle"
          autoComplete="username"
          required
          pattern="[a-z][a-z0-9_]{2,23}"
        />
      </label>
      {
        match (register) {
          true  =>
            <label {...props(uiStyles.field)}>
              <span {...props(uiStyles.fieldLabel)}>Email</span>
              <input {...props(uiStyles.fieldControl)} type="email" name="email" autoComplete="email" required />
            </label>,
          false => null,
        }
      }
      <label {...props(uiStyles.field)}>
        <span {...props(uiStyles.fieldLabel)}>Password</span>
        <input
          {...props(uiStyles.fieldControl)}
          type="password"
          name="password"
          autoComplete={
            match (register) {
              true  => "new-password",
              false => "current-password",
            }
          }
          required
          minLength={
            match (register) {
              true  => 12,
              false => undefined,
            }
          }
          maxLength={256}
        />
      </label>
      {
        match (error) {
          ""            => null,
          const message =>
            <p role="alert" {...props(uiStyles.postError)}>
              {message}
            </p>,
        }
      }
      <button type="submit" {...props(uiStyles.button, uiStyles.primary)} disabled={pending}>
        {
          match (pending) {
            true  => "Please wait…",
            false =>
              match (register) {
                true  => "Create account",
                false => "Sign in",
              },
          }
        }
      </button>
      <p>
        <Link
          to={
            match (register) {
              true  => "/login",
              false => "/signup",
            }
          }
        >
          {
            match (register) {
              true  => "Already have an account? Sign in",
              false => "New here? Create an account",
            }
          }
        </Link>
      </p>
    </form>
  );
}
