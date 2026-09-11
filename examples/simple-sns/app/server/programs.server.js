// @flow
import {
  effect,
  tag,
  suspend,
  succeed,
  fail,
  die,
  type Effect,
  type EffectGenerator,
  type Tag,
} from "@uniflowed/effect";
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
  type FieldErrors,
} from "../social-model.js";

export type Identity = {| readonly current: () => User | null |};
export type Store = {|
  readonly insertPost: (User, string, Topic, string) => Post,
  readonly setReaction: (User, string, boolean) => Post,
  readonly insertMessage: (User, string, string, string) => Message,
  readonly saveSettings: (User, Settings) => Settings,
|};
export type MutationProblem =
  | {| readonly kind: "unauthenticated" |}
  | {| readonly kind: "validation", readonly message: string, readonly fields: FieldErrors |};
export const IdentityService: Tag<Identity> = tag("commonplace/identity");
export const SocialStore: Tag<Store> = tag("commonplace/store");
export type Mutation<out T> = Effect<T, MutationProblem, Identity | Store>;

// Only expected input/ownership failures enter E. Bugs and database faults stay defects.
function attempt<T>(body: () => T): Effect<T, MutationProblem> {
  return suspend(() => {
    try {
      return succeed(body());
    } catch (error) {
      return error instanceof InputError
        ? fail({ kind: "validation", message: error.message, fields: error.fields })
        : die(error);
    }
  });
}
const authenticated: Effect<User, MutationProblem, Identity> = effect(function* (): EffectGenerator<
  User,
  MutationProblem,
  Identity,
> {
  const identity = yield* IdentityService;
  const user = identity.current();
  if (user == null) return yield* fail<MutationProblem>({ kind: "unauthenticated" });
  return user;
});

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
export function appreciateNote(id: string, liked: boolean): Mutation<Post> {
  return effect(function* (): EffectGenerator<Post, MutationProblem, Identity | Store> {
    const user = yield* authenticated;
    yield* attempt(() => {
      identifier(id);
      if (typeof liked !== "boolean") throw new InputError("Please try again.");
    });
    const store = yield* SocialStore;
    return yield* attempt(() => store.setReaction(user, id, liked));
  });
}
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
