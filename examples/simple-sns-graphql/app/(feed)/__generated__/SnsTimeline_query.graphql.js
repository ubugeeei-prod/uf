/**
 * @generated SignedSource<<d77d74ea8bfb362fc6b0a2fa55313959>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ReaderFragment, RefetchableFragment } from 'relay-runtime';
import type { SnsPost_post$fragmentType } from "./SnsPost_post.graphql";
import type { FragmentType } from "relay-runtime";
declare export opaque type SnsTimeline_query$fragmentType: FragmentType;
type SnsTimelineRefetchQuery$variables = any;
export type SnsTimeline_query$data = {
  readonly feed: {
    readonly hasNext: boolean,
    readonly posts: ReadonlyArray<{
      readonly id: string,
      readonly $fragmentSpreads: SnsPost_post$fragmentType,
    }>,
  },
  readonly viewer: ?{
    readonly id: string,
  },
  readonly $fragmentType: SnsTimeline_query$fragmentType,
};
export type SnsTimeline_query$key = {
  readonly $data?: SnsTimeline_query$data,
  readonly $fragmentSpreads: SnsTimeline_query$fragmentType,
  ...
};
*/

import SnsTimelineRefetchQuery_graphql from './SnsTimelineRefetchQuery.graphql';

var node/*: ReaderFragment*/ = (function(){
var v0 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
};
return {
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
  "metadata": {
    "refetch": {
      "connection": null,
      "fragmentPathInResult": [],
      "operation": SnsTimelineRefetchQuery_graphql
    }
  },
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
        (v0/*:: as any*/)
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
            (v0/*:: as any*/),
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
})();

(node/*:: as any*/).hash = "74c2a53bfeea4245cf7b8a9e39aa3d1a";

export default ((node/*:: as any*/)/*:: as RefetchableFragment<
  SnsTimeline_query$fragmentType,
  SnsTimeline_query$data,
  SnsTimelineRefetchQuery$variables,
>*/);
