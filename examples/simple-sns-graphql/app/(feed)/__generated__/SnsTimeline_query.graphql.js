/**
 * @generated SignedSource<<cae2292de5156ee2bbca41d2772461dc>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { Fragment, ReaderFragment } from 'relay-runtime';
import type { SnsComposer_viewer$fragmentType } from "./SnsComposer_viewer.graphql";
import type { SnsPost_post$fragmentType } from "./SnsPost_post.graphql";
import type { FragmentType } from "relay-runtime";
declare export opaque type SnsTimeline_query$fragmentType: FragmentType;
export type SnsTimeline_query$data = {
  readonly feed: {
    readonly hasNext: boolean,
    readonly posts: ReadonlyArray<{
      readonly id: string,
      readonly $fragmentSpreads: SnsPost_post$fragmentType,
    }>,
  },
  readonly viewer: ?{
    readonly $fragmentSpreads: SnsComposer_viewer$fragmentType,
  },
  readonly $fragmentType: SnsTimeline_query$fragmentType,
};
export type SnsTimeline_query$key = {
  readonly $data?: SnsTimeline_query$data,
  readonly $fragmentSpreads: SnsTimeline_query$fragmentType,
  ...
};
*/

var node/*: ReaderFragment*/ = {
  "argumentDefinitions": [
    {
      "defaultValue": null,
      "kind": "LocalArgument",
      "name": "page"
    },
    {
      "defaultValue": null,
      "kind": "LocalArgument",
      "name": "search"
    },
    {
      "defaultValue": null,
      "kind": "LocalArgument",
      "name": "topic"
    }
  ],
  "kind": "Fragment",
  "metadata": null,
  "name": "SnsTimeline_query",
  "selections": [
    {
      "alias": null,
      "args": null,
      "concreteType": "User",
      "kind": "LinkedField",
      "name": "viewer",
      "plural": false,
      "selections": [
        {
          "args": null,
          "kind": "FragmentSpread",
          "name": "SnsComposer_viewer"
        }
      ],
      "storageKey": null
    },
    {
      "alias": null,
      "args": [
        {
          "kind": "Variable",
          "name": "page",
          "variableName": "page"
        },
        {
          "kind": "Variable",
          "name": "search",
          "variableName": "search"
        },
        {
          "kind": "Variable",
          "name": "topic",
          "variableName": "topic"
        }
      ],
      "concreteType": "Feed",
      "kind": "LinkedField",
      "name": "feed",
      "plural": false,
      "selections": [
        {
          "alias": null,
          "args": null,
          "concreteType": "Post",
          "kind": "LinkedField",
          "name": "posts",
          "plural": true,
          "selections": [
            {
              "alias": null,
              "args": null,
              "kind": "ScalarField",
              "name": "id",
              "storageKey": null
            },
            {
              "args": null,
              "kind": "FragmentSpread",
              "name": "SnsPost_post"
            }
          ],
          "storageKey": null
        },
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "hasNext",
          "storageKey": null
        }
      ],
      "storageKey": null
    }
  ],
  "type": "Query",
  "abstractKey": null
};

(node/*:: as any*/).hash = "c4b029334a552bbbf267b468235bf6b7";

export default ((node/*:: as any*/)/*:: as Fragment<
  SnsTimeline_query$fragmentType,
  SnsTimeline_query$data,
>*/);
