// @flow

import { createHash, randomBytes, scrypt, timingSafeEqual } from "node:crypto";
import { cookies } from "@uniflowed/server";

import { database, transaction } from "./database.server.js";
import { member, publicUser, welcomeConversation } from "./repository.server.js";
import {
  attemptCount,
  deleteAttempts,
  deleteExpiredAttempts,
  deleteExpiredSessions,
  deleteSession,
  insertMember,
  insertSession,
  memberByHandle,
  recordAttempt,
  sessionMember,
} from "./db/query.sql.js";
import { InputError, field, handleField, emailField } from "./validation.server.js";

import type { User } from "../_shared/social-model.js";

export const SESSION_COOKIE = "commonplace.session";

const TTL = 60 * 60 * 24 * 7;

function digest(token: string): string {
  return createHash("sha256").update(token).digest("hex");
}

/**
 * Resolve a well-formed session token against its hash and expiry; invalid tokens are guests.
 */

export function viewerFor(token: string | null): User | null {
  if (token == null || !/^[a-f0-9]{64}$/.test(token)) {
    return null;
  }
  const memberId = sessionMember(database(), {
    tokenHash: digest(token),
    expiresAt: Date.now(),
  });

  return memberId == null ? null : member(memberId);
}

/** Read identity from this request’s cookie context, never from process-global user state. */

export function viewer(): User | null {
  return viewerFor(cookies().get(SESSION_COOKIE));
}

/** Require a current session for repository callers that need a concrete account. */

export function requireViewer(): User {
  const current = viewer();
  if (current == null) {
    throw new InputError("Please sign in to continue.");
  }

  return current;
}

function derive(password: string, salt: string): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    scrypt(password, salt, 64, { N: 32768, r: 8, p: 1, maxmem: 64 * 1024 * 1024 }, (error, key) =>
      error != null ? reject(error) : resolve(key),
    );
  });
}

function passwordField(form: FormData): string {
  const raw = form.get("password");
  if (typeof raw !== "string" || raw.length < 12 || raw.length > 128)
    throw new InputError("Please check your password.", { password: "Use 12–128 characters." });

  return raw;
}

/**
 * Validate credentials with asynchronous scrypt and persistent per-handle throttling.
 * Signup creates the account and its welcome thread atomically; fixture accounts cannot sign in.
 */

export async function authenticate(form: FormData, mode: string): Promise<User> {
  const handle = handleField(form);
  const password = passwordField(form);
  const now = Date.now();
  const db = database();
  deleteExpiredAttempts(db, { resetsAt: now });
  recordAttempt(db, { handle, resetsAt: now + 600_000 });
  const attempts = attemptCount(db, { handle });
  if ((attempts ?? 0) > 10) {
    throw new InputError("Too many attempts. Please try again in 10 minutes.");
  }
  if (mode === "signup") {
    const name = field(form, "name", 80, 1);
    const email = emailField(form);
    const salt = randomBytes(16).toString("hex");
    const hash = `${salt}:${(await derive(password, salt)).toString("hex")}`;
    return transaction((connection) => {
      if (memberByHandle(connection, { handle }) != null) {
        throw new InputError("That handle is already taken.", { handle: "Choose another handle." });
      }
      const id = crypto.randomUUID();
      insertMember(connection, { id, name, handle, email, passwordHash: hash });
      welcomeConversation(connection, id);
      const created = member(id, connection);
      if (created == null) {
        throw new Error("New member is missing");
      }
      deleteAttempts(connection, { handle });
      return created;
    });
  }
  const row = memberByHandle(db, { handle });
  // Do the same expensive derivation for an unknown account.
  const [salt, hash] = String(row?.passwordHash ?? `${"0".repeat(32)}:${"0".repeat(128)}`).split(
    ":",
  );
  const actual = await derive(password, salt);
  const expected = Buffer.from(hash, "hex");
  if (
    row == null ||
    row.passwordHash == null ||
    actual.length !== expected.length ||
    !timingSafeEqual(actual, expected)
  ) {
    throw new InputError("The handle or password is incorrect.");
  }
  deleteAttempts(db, { handle });

  return publicUser(row);
}

/**
 * Rotate a session atomically and return its HttpOnly cookie.
 * Persist only the token hash; the caller must write this header before streaming a response.
 */

export function issueSession(user: User, oldToken: string | null, secure: boolean): string {
  const token = randomBytes(32).toString("hex");
  transaction((db) => {
    if (oldToken != null) {
      deleteSession(db, { tokenHash: digest(oldToken) });
    }
    deleteExpiredSessions(db, { expiresAt: Date.now() });
    insertSession(db, {
      tokenHash: digest(token),
      memberId : user.id,
      expiresAt: Date.now() + TTL * 1000,
    });
  });

  return `${SESSION_COOKIE}=${token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=${TTL}${secure ? "; Secure" : ""}`;
}

/** Delete the presented session and return an expired cookie for the HTTP response. */

export function revokeSession(token: string | null, secure: boolean): string {
  if (token != null) {
    deleteSession(database(), { tokenHash: digest(token) });
  }

  return `${SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0${secure ? "; Secure" : ""}`;
}
