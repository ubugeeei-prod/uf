/**
 * @generated SignedSource<<2a1a2d4008a03b7f5a5868298c9cbd8c>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { FragmentType } from "relay-runtime";
import type { SnsTimeline_query$fragmentType } from "./SnsTimeline_query.graphql";
export type SnsTimelineRefetchQuery$variables = {
  page: number,
  search: string,
  topic: string,
};
export type SnsTimelineRefetchQuery$data = {
  readonly $fragmentSpreads: SnsTimeline_query$fragmentType,
};
export type SnsTimelineRefetchQuery = {
  response: SnsTimelineRefetchQuery$data,
  variables: SnsTimelineRefetchQuery$variables,
};
*/

var node/*: ConcreteRequest*/ = (function(){
var v0 = [
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
v1 = [
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
v2 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
};
return {
  "fragment": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsTimelineRefetchQuery",
    "selections": [
      {
        "args": (v1/*:: as any*/),
        "kind": "FragmentSpread",
        "name": "SnsTimeline_query"
      }
    ],
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "SnsTimelineRefetchQuery",
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "User",
        "kind": "LinkedField",
        "name": "viewer",
        "plural": false,
        "selections": [
          (v2/*:: as any*/)
        ],
        "storageKey": null
      },
      {
        "alias": null,
        "args": (v1/*:: as any*/),
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
              (v2/*:: as any*/),
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "body",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "topic",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "likes",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "liked",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "createdAt",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "concreteType": "User",
                "kind": "LinkedField",
                "name": "author",
                "plural": false,
                "selections": [
                  {
                    "alias": null,
                    "args": null,
                    "kind": "ScalarField",
                    "name": "name",
                    "storageKey": null
                  },
                  {
                    "alias": null,
                    "args": null,
                    "kind": "ScalarField",
                    "name": "handle",
                    "storageKey": null
                  },
                  (v2/*:: as any*/),
                  {
                    "alias": null,
                    "args": null,
                    "kind": "ScalarField",
                    "name": "photo",
                    "storageKey": null
                  },
                  {
                    "alias": null,
                    "args": null,
                    "kind": "ScalarField",
                    "name": "avatar",
                    "storageKey": null
                  }
                ],
                "storageKey": null
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
    ]
  },
  "params": {
    "cacheID": "fd3b9f44963b4af0d9961d49a5b578bc",
    "id": null,
    "metadata": {},
    "name": "SnsTimelineRefetchQuery",
    "operationKind": "query",
    "text": "query SnsTimelineRefetchQuery(\n  $page: Int!\n  $search: String!\n  $topic: String!\n) {\n  ...SnsTimeline_query_41NAMG\n}\n\nfragment SnsAvatar_user on User {\n  id\n  photo\n  avatar\n}\n\nfragment SnsPost_post on Post {\n  id\n  body\n  topic\n  likes\n  liked\n  createdAt\n  author {\n    name\n    handle\n    ...SnsAvatar_user\n    id\n  }\n}\n\nfragment SnsTimeline_query_41NAMG on Query {\n  viewer {\n    id\n  }\n  feed(topic: $topic, search: $search, page: $page) {\n    posts {\n      id\n      ...SnsPost_post\n    }\n    hasNext\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "74c2a53bfeea4245cf7b8a9e39aa3d1a";

export default ((node/*:: as any*/)/*:: as Query<
  SnsTimelineRefetchQuery$variables,
  SnsTimelineRefetchQuery$data,
>*/);
