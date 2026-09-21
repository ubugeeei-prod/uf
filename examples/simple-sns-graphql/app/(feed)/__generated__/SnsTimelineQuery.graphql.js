/**
 * @generated SignedSource<<46785fe4fb9a04c76e54aaf0bc90cca7>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { SnsPost_post$fragmentType } from "./SnsPost_post.graphql";
export type SnsTimelineQuery$variables = {
  page: number,
  search: string,
  topic: string,
};
export type SnsTimelineQuery$data = {
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
};
export type SnsTimelineQuery = {
  response: SnsTimelineQuery$data,
  variables: SnsTimelineQuery$variables,
};
*/

var node/*: ConcreteRequest*/ = (function(){
var v0 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "page"
},
v1 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "search"
},
v2 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "topic"
},
v3 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
},
v4 = {
  "alias": null,
  "args": null,
  "concreteType": "User",
  "kind": "LinkedField",
  "name": "viewer",
  "plural": false,
  "selections": [
    (v3/*:: as any*/)
  ],
  "storageKey": null
},
v5 = [
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
v6 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "hasNext",
  "storageKey": null
};
return {
  "fragment": {
    "argumentDefinitions": [
      (v0/*:: as any*/),
      (v1/*:: as any*/),
      (v2/*:: as any*/)
    ],
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsTimelineQuery",
    "selections": [
      (v4/*:: as any*/),
      {
        "alias": null,
        "args": (v5/*:: as any*/),
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
              (v3/*:: as any*/),
              {
                "args": null,
                "kind": "FragmentSpread",
                "name": "SnsPost_post"
              }
            ],
            "storageKey": null
          },
          (v6/*:: as any*/)
        ],
        "storageKey": null
      }
    ],
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [
      (v2/*:: as any*/),
      (v1/*:: as any*/),
      (v0/*:: as any*/)
    ],
    "kind": "Operation",
    "name": "SnsTimelineQuery",
    "selections": [
      (v4/*:: as any*/),
      {
        "alias": null,
        "args": (v5/*:: as any*/),
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
              (v3/*:: as any*/),
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
                  (v3/*:: as any*/),
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
          (v6/*:: as any*/)
        ],
        "storageKey": null
      }
    ]
  },
  "params": {
    "cacheID": "c8c92afc2c26d6573f31f0e147b2ecd9",
    "id": null,
    "metadata": {},
    "name": "SnsTimelineQuery",
    "operationKind": "query",
    "text": "query SnsTimelineQuery(\n  $topic: String!\n  $search: String!\n  $page: Int!\n) {\n  viewer {\n    id\n  }\n  feed(topic: $topic, search: $search, page: $page) {\n    posts {\n      id\n      ...SnsPost_post\n    }\n    hasNext\n  }\n}\n\nfragment SnsAvatar_user on User {\n  id\n  photo\n  avatar\n}\n\nfragment SnsPost_post on Post {\n  id\n  body\n  topic\n  likes\n  liked\n  createdAt\n  author {\n    name\n    handle\n    ...SnsAvatar_user\n    id\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "ed2215da717f8bcf3123f118eb00a072";

export default ((node/*:: as any*/)/*:: as Query<
  SnsTimelineQuery$variables,
  SnsTimelineQuery$data,
>*/);
