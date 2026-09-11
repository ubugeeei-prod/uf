// @flow

import {
  effect,
  tag,
  trySync,
  fail,
  type Effect,
  type EffectGenerator,
  type Tag,
} from "@uniflowed/effect";
import { inputEffect, type InputProblem } from "./input-effect.server.js";
import { InputError, field, handleField, emailField, identifier } from "./validation.server.js";
import {
  MAX_POST_LENGTH,
  MAX_MESSAGE_LENGTH,
  topicFrom,
  type User,
  type Post,
  type Topic,
  type Message,
  type Settings,
} from "../social-model.js";

/** Request-time identity capability; implementations must not cache a global current user. */
export type Identity = {| readonly current: () => User | null |};

/** Persistence capabilities injected into mutation programs for production or isolated tests. */
export type Store = {|
  readonly insertPost: (User, string, Topic, string) => Post,
  readonly setReaction: (User, string, boolean) => Post,
  readonly insertMessage: (User, string, string, string) => Message,
  readonly saveSettings: (User, Settings) => Settings,
|};

/** Expected failures callers can act on; unexpected storage faults remain defects. */
export type MutationProblem = {| readonly kind: "unauthenticated" |} | InputProblem;

/** Dependency key for resolving the account when the program runs. */
export const IdentityService: Tag<Identity> = tag("commonplace/identity");

/** Dependency key for authorized persistence operations. */
export const SocialStore: Tag<Store> = tag("commonplace/store");

/** A typed write program requiring identity and persistence capabilities. */
export type Mutation<out T> = Effect<T, MutationProblem, Identity | Store>;

/** Run a repository operation lazily, classifying only its expected rejections. */
function attempt<T>(body: () => T): Effect<T, MutationProblem> {
  return inputEffect(trySync({ try: body, catch: (error) => error }));
}

const authenticated: Effect<User, MutationProblem, Identity> = effect(function* (): EffectGenerator<
  User,
  MutationProblem,
  Identity,
> {
  const identity = yield* IdentityService;
  const user = identity.current();
  if (user == null) {
    return yield* fail<MutationProblem>({ kind: "unauthenticated" });
  }

  return user;
});

/** Authenticate, validate the form, and publish using an idempotent submission ID. */
export function publishNote(form: FormData): Mutation<Post> {
  return effect(function* (): EffectGenerator<Post, MutationProblem, Identity | Store> {
    const user = yield* authenticated;
    const input = yield* attempt(() => {
      const body = field(form, "body", MAX_POST_LENGTH, 1);
      const topic = topicFrom(field(form, "topic", 20, 1));
      if (topic == null)
        throw new InputError("Choose a channel.", { topic: "Choose an available channel." });
      return { body, topic, requestId: identifier(field(form, "requestId", 100, 1)) };
    });
    const store = yield* SocialStore;
    return yield* attempt(() => store.insertPost(user, input.body, input.topic, input.requestId));
  });
}

/** Authenticate and validate a requested reaction state before updating the store. */
export function appreciateNote(id: string, liked: boolean): Mutation<Post> {
  return effect(function* (): EffectGenerator<Post, MutationProblem, Identity | Store> {
    const user = yield* authenticated;
    yield* attempt(() => {
      identifier(id);
      if (typeof liked !== "boolean") {
        throw new InputError("Please try again.");
      }
    });
    const store = yield* SocialStore;
    return yield* attempt(() => store.setReaction(user, id, liked));
  });
}

/** Authenticate and validate a message before the repository checks thread membership. */
export function deliverMessage(form: FormData): Mutation<Message> {
  return effect(function* (): EffectGenerator<Message, MutationProblem, Identity | Store> {
    const user = yield* authenticated;
    const input = yield* attempt(() => ({
      threadId: identifier(field(form, "threadId", 100, 1)),
      body: field(form, "body", MAX_MESSAGE_LENGTH, 1),
      requestId: identifier(field(form, "requestId", 100, 1)),
    }));
    const store = yield* SocialStore;
    return yield* attempt(() =>
      store.insertMessage(user, input.threadId, input.body, input.requestId),
    );
  });
}

/** Authenticate and validate private profile edits before the transactional update. */
export function changeProfile(form: FormData): Mutation<Settings> {
  return effect(function* (): EffectGenerator<Settings, MutationProblem, Identity | Store> {
    const user = yield* authenticated;
    const next = yield* attempt(() => ({
      displayName: field(form, "displayName", 80, 1),
      handle: handleField(form),
      bio: field(form, "bio", 160),
      email: emailField(form),
    }));
    const store = yield* SocialStore;
    return yield* attempt(() => store.saveSettings(user, next));
  });
}
