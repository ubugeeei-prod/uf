// @flow
//
// `@uniflowed/story/story`: declaring a component's states, as data.
//
// A story is a name, a component, and the props that put it in one state.
// Declaring one runs nothing, mounts nothing and touches no global: this
// module is entirely value-level, so importing a story file is cheap and a
// story catalogue can be built by a tool that has no DOM.
//
//   export const stories = defineStories({
//     title: "Button",
//     component: Button,
//     props: { label: "Save", pending: false },
//     stories: {
//       Primary: {},
//       Pending: { props: { pending: true } },
//     },
//   });
//
// # A set carries complete props; a story is a delta
//
// [`StorySetConfig.props`] is `Props`, not `Partial<Props>`, and each story's
// own `props` is the partial one. That is the whole of the inheritance rule,
// and it is what makes "every story in this set renders" true by construction
// rather than by hope: there is no way to declare a story whose props are
// incomplete, because the set already supplied them.
//
// Storybook allows partial `args` at both levels and finds out at render time
// which component ended up without a required one. uf has a type checker; a
// missing prop is a type error at the declaration, in the file that made the
// mistake.
//
// They are called `props` rather than `args` because that is what they are.
// uf is a React toolchain, the value is spread onto a React component, and a
// second word for props would only be a word to translate.
//
// # Where the type parameter earns its keep, and where it stops
//
// `defineStories` is generic in `Props`, and that is where every check
// happens: `component` must accept them, the set's `props` must be complete
// for it, each story's overrides must be a subset of the same shape, and a
// `play` function is handed a context whose `props` field is `Props`.
//
// [`Story`] and [`StorySet`] are *not* generic. A catalogue holds the stories
// of many components, and those have no common type parameter — a
// `StorySet<Props>` is neither covariant nor contravariant in `Props`, because
// `props` reads it and `component` and `play` consume it. Keeping the
// parameter would mean an `any` at the point where the catalogue is built,
// and this package does not use `any`. So the parameter is checked at the
// declaration and erased into the catalogue: [`Story.props`] is
// [`StoryProps`], and [`Story.element`] is a closure that already has the
// typed props inside it, applied to the typed component. Nothing downstream
// can get the pairing wrong, because nothing downstream can see the two
// halves separately.
//
// # Identity
//
// A story's [`Story.id`] is `<title>--<name>`, slugified. It is the name a
// failing CI job prints, the name a visual-regression baseline is filed
// under, and the name a URL would carry — so it has to be stable under
// re-ordering, safe in a path, and derived from what a person wrote rather
// than from a counter. Two stories that slugify to one id are rejected by
// `collect.js`, which is the only place that can see both.

import type { MockHandler } from "@uniflowed/mock";
import type * as React from "@uniflowed/react";

import type { PlayContext, PlayFunction, PlayStage } from "./play.js";

/**
 * A story's props once the type parameter is gone.
 *
 * `mixed`, not `any`: a consumer showing a story's props in a panel has to
 * narrow each one, which is correct — it genuinely does not know what they
 * are.
 */
export type StoryProps = { readonly [string]: mixed };

/**
 * Something wrapped around a story before it is mounted.
 *
 * A theme provider, a router context, a fixed-width frame. It takes the node
 * rather than a render function because there is nothing else useful to give
 * it: React already defers the work, and a decorator that could choose *not*
 * to call its child would be a decorator that can silently render nothing.
 */
export type Decorator = (children: React.Node) => React.Node;

/** One named state, as the caller writes it. */
export type StoryDeclaration<Props extends { ... }> = {|
  /**
   * What a person calls this state. Defaults to the key it was declared
   * under, which is usually already the right words.
   */
  readonly name?: string,
  /** What differs from the set's props. */
  readonly props?: Partial<Props>,
  /** Wrapped inside the set's decorators. */
  readonly decorators?: $ReadOnlyArray<Decorator>,
  /** Offered before the set's, so a story can override one of them. */
  readonly mocks?: $ReadOnlyArray<MockHandler>,
  /** Drives this state and asserts on it. Replaces the set's `play`. */
  readonly play?: (context: PlayContext<Props>) => mixed,
|};

/** A component's stories, as the caller writes them. */
export type StorySetConfig<Props extends { ... }> = {|
  /** What the component is called. `"Forms/Button"` groups it. */
  readonly title: string,
  /**
   * The component every story in the set renders.
   *
   * `component(...Props)` rather than `component(...Props) renders X`: a
   * story renders whatever its component renders, and constraining that here
   * would be this package having an opinion about a component it was handed.
   */
  readonly component: component(...Props),
  /** Complete props, so every story below is renderable. */
  readonly props: Props,
  /** Wrapped around every story, outside the story's own decorators. */
  readonly decorators?: $ReadOnlyArray<Decorator>,
  /** In force for every story in the set, behind the story's own. */
  readonly mocks?: $ReadOnlyArray<MockHandler>,
  /** Run for every story that does not declare its own. */
  readonly play?: (context: PlayContext<Props>) => mixed,
  /**
   * The states, in declaration order.
   *
   * An object rather than an array because the key is the story's identity —
   * it is what a test names and what an id is built from — and an array of
   * `{ name, … }` records makes that a field somebody can forget.
   */
  readonly stories: { readonly [key: string]: StoryDeclaration<Props> },
|};

/** One story, resolved: everything it needs to be rendered, and nothing else. */
export type Story = {|
  /** `"button--pending"`. Stable, path-safe, and derived from what was written. */
  readonly id: string,
  /** The key it was declared under. */
  readonly key: string,
  /** What a person calls it. */
  readonly name: string,
  /** The set's title, repeated here so a story is self-describing. */
  readonly title: string,
  /** The set's props with this story's overrides applied, erased to `mixed`. */
  readonly props: StoryProps,
  /** The set's decorators, then this story's. First is outermost. */
  readonly decorators: $ReadOnlyArray<Decorator>,
  /**
   * This story's handlers, then the set's.
   *
   * That way round because `@uniflowed/mock` offers a request to handlers in
   * order and the first that matches answers it: a story that declares
   * `GET /users/:id` overrides the set's handler for the same route, which is
   * what a story called `Missing` is for.
   */
  readonly mocks: $ReadOnlyArray<MockHandler>,
  /** What drives it, or `null`. */
  readonly play: PlayFunction | null,
  /**
   * The element, built on demand.
   *
   * A function rather than a node so that declaring a thousand stories costs
   * a thousand closures rather than a thousand React elements, and so a story
   * rendered twice gets two elements rather than one shared one.
   */
  readonly element: () => React.Node,
|};

/**
 * The brand [`isStorySet`] looks for.
 *
 * A string rather than a class or a `Symbol()`, because the check has to hold
 * across two copies of this package in one process — a linked workspace
 * beside a nested install is the ordinary way that happens — and `instanceof`
 * does not.
 */
export const STORY_SET: "uniflowed/story-set" = "uniflowed/story-set";

/** A component's stories, resolved. */
export type StorySet = {|
  readonly kind: typeof STORY_SET,
  readonly title: string,
  readonly stories: $ReadOnlyArray<Story>,
|};

/**
 * Resolve a component's stories.
 *
 * Everything is computed here: inheritance, names, ids and the element
 * closures. A [`StorySet`] is therefore inert data — the reason a tool can
 * import a story file to list what is in it without a DOM, a runner or a
 * network.
 *
 * Throws when the set is empty. A story file that declares no stories is a
 * file somebody meant to finish, and reporting it as zero stories hides that
 * at exactly the moment it is cheap to notice.
 */
export function defineStories<Props extends { ... }>(config: StorySetConfig<Props>): StorySet {
  const keys = Object.keys(config.stories);
  if (keys.length === 0) {
    throw new Error(`@uniflowed/story: ${config.title} declares no stories`);
  }

  const Component = config.component;
  const stories = keys.map((key) => {
    const declaration = config.stories[key];
    const name = declaration.name ?? key;
    // Spread rather than `Object.assign`: the result is a new object each
    // time, so no story can reach another's props, and Flow reads the spread
    // of a `Partial<Props>` over a `Props` as `Props` — which is what makes
    // the element below check.
    const props: Props = { ...config.props, ...declaration.props };
    const declaredPlay = declaration.play ?? config.play;

    return {
      id: storyId(config.title, name),
      key,
      name,
      props,
      title: config.title,
      decorators: [...(config.decorators ?? []), ...(declaration.decorators ?? [])],
      mocks: [...(declaration.mocks ?? []), ...(config.mocks ?? [])],
      // The one place the type parameter crosses into the erased world, and it
      // crosses without a cast: `props` is still `Props` in this scope, so the
      // context handed to the caller's function is a real `PlayContext<Props>`
      // and the closure that remains is `PlayFunction`.
      play: declaredPlay == null ? null : (stage: PlayStage) => declaredPlay({ ...stage, props }),
      element: () => <Component {...props} />,
    };
  });

  return { kind: STORY_SET, title: config.title, stories };
}

/**
 * The id `title` and `name` produce.
 *
 * Exported because a visual-regression baseline, a URL and a report all have
 * to agree on it, and each computing its own would agree until the day one of
 * them handled a slash differently.
 */
export function storyId(title: string, name: string): string {
  return `${slugify(title)}--${slugify(name)}`;
}

/**
 * `"Forms/Text Field"` becomes `"forms-text-field"`.
 *
 * Lowercase, and every run of anything else becomes a single dash. That loses
 * information — `"A/B"` and `"A B"` are one slug — which is why duplicate ids
 * are an error where they can be seen rather than a silent overwrite.
 */
function slugify(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

/** Whether `value` is a [`StorySet`]. */
export function isStorySet(value: mixed): boolean {
  return (
    typeof value === "object" &&
    value != null &&
    value.kind === STORY_SET &&
    Array.isArray(value.stories)
  );
}

/**
 * The story in `set` under `key`, or named `name`.
 *
 * Throws rather than returning `undefined`, and says what the set does hold.
 * The caller is a test naming a story it believes exists; handing it `void`
 * turns a renamed story into a `TypeError` three lines later, in the runner
 * rather than in the test.
 */
export function findStory(set: StorySet, key: string): Story {
  // A key first, then a name. Both are looked up because a story's name is
  // what a report prints and a key is what the file says — but a set where
  // one story's *name* is another story's *key* would otherwise answer with
  // whichever came first in the file, and mount the wrong component.
  const found =
    set.stories.find((story) => story.key === key) ??
    set.stories.find((story) => story.name === key);
  if (found == null) {
    const known = set.stories.map((story) => story.key).join(", ");
    throw new Error(`@uniflowed/story: ${set.title} has no story ${key}; it has ${known}`);
  }
  return found;
}
