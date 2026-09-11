// @flow
import { createHash, randomBytes, scrypt, timingSafeEqual } from "node:crypto";
import { cookies } from "@uniflowed/server";
import { database, transaction } from "./database.server.js";
import { member, publicUser, welcomeConversation } from "./repository.server.js";
import { InputError, field, handleField, emailField } from "./validation.server.js";
import type { User } from "../social-model.js";

export const SESSION_COOKIE = "commonplace.session";
const TTL = 60 * 60 * 24 * 7;
function digest(token: string): string {
  return createHash("sha256").update(token).digest("hex");
}
export function viewerFor(token: string | null): User | null {
  if (token == null || !/^[a-f0-9]{64}$/.test(token)) return null;
  const row = database()
    .prepare("SELECT member_id FROM sessions WHERE token_hash=? AND expires_at>?")
    .get(digest(token), Date.now());
  return row == null ? null : member(String(row.member_id));
}
export function viewer(): User | null {
  return viewerFor(cookies().get(SESSION_COOKIE));
}
export function requireViewer(): User {
  const current = viewer();
  if (current == null) throw new InputError("Please sign in to continue.");
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
export async function authenticate(form: FormData, mode: string): Promise<User> {
  const handle = handleField(form);
  const password = passwordField(form);
  const now = Date.now();
  const db = database();
  db.prepare("DELETE FROM auth_attempts WHERE resets_at<?").run(now);
  db.prepare(
    "INSERT INTO auth_attempts VALUES (?, 1, ?) ON CONFLICT(handle) DO UPDATE SET attempts=attempts+1",
  ).run(handle, now + 600_000);
  const attempts = db.prepare("SELECT attempts FROM auth_attempts WHERE handle=?").get(handle);
  if (Number(attempts?.attempts ?? 0) > 10)
    throw new InputError("Too many attempts. Please try again in 10 minutes.");
  if (mode === "signup") {
    const name = field(form, "name", 80, 1);
    const email = emailField(form);
    const salt = randomBytes(16).toString("hex");
    const hash = `${salt}:${(await derive(password, salt)).toString("hex")}`;
    return transaction((connection) => {
      if (connection.prepare("SELECT 1 FROM members WHERE handle=?").get(handle) != null)
        throw new InputError("That handle is already taken.", { handle: "Choose another handle." });
      const id = crypto.randomUUID();
      connection
        .prepare("INSERT INTO members (id,name,handle,email,password_hash) VALUES (?,?,?,?,?)")
        .run(id, name, handle, email, hash);
      welcomeConversation(id);
      const created = member(id);
      if (created == null) throw new Error("New member is missing");
      connection.prepare("DELETE FROM auth_attempts WHERE handle=?").run(handle);
      return created;
    });
  }
  const row = db.prepare("SELECT * FROM members WHERE handle=?").get(handle);
  // Do the same expensive derivation for an unknown account.
  const [salt, hash] = String(row?.password_hash ?? `${"0".repeat(32)}:${"0".repeat(128)}`).split(
    ":",
  );
  const actual = await derive(password, salt);
  const expected = Buffer.from(hash, "hex");
  if (
    row == null ||
    row.password_hash == null ||
    actual.length !== expected.length ||
    !timingSafeEqual(actual, expected)
  )
    throw new InputError("The handle or password is incorrect.");
  db.prepare("DELETE FROM auth_attempts WHERE handle=?").run(handle);
  return publicUser(row);
}
export function issueSession(user: User, oldToken: string | null, secure: boolean): string {
  const token = randomBytes(32).toString("hex");
  transaction((db) => {
    if (oldToken != null)
      db.prepare("DELETE FROM sessions WHERE token_hash=?").run(digest(oldToken));
    db.prepare("DELETE FROM sessions WHERE expires_at<=?").run(Date.now());
    db.prepare("INSERT INTO sessions VALUES (?, ?, ?)").run(
      digest(token),
      user.id,
      Date.now() + TTL * 1000,
    );
  });
  return `${SESSION_COOKIE}=${token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=${TTL}${secure ? "; Secure" : ""}`;
}
export function revokeSession(token: string | null, secure: boolean): string {
  if (token != null)
    database().prepare("DELETE FROM sessions WHERE token_hash=?").run(digest(token));
  return `${SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0${secure ? "; Secure" : ""}`;
}
