/**
 * @generated SignedSource<<77a73fa4b052a9a8b833bc3c3b36fdf3>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { SnsSocialFrame_query$fragmentType } from "./../../_shared/__generated__/SnsSocialFrame_query.graphql";
import type { SnsTimeline_query$fragmentType } from "./SnsTimeline_query.graphql";
export type SnsFeedPageQuery$variables = {
  page: number,
  search: string,
  topic: string,
};
export type SnsFeedPageQuery$data = {
  readonly $fragmentSpreads: SnsSocialFrame_query$fragmentType & SnsTimeline_query$fragmentType,
};
export type SnsFeedPageQuery = {
  response: SnsFeedPageQuery$data,
  variables: SnsFeedPageQuery$variables,
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
v3 = [
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
v4 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
},
v5 = [
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
  (v4/*:: as any*/),
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
];
return {
  "fragment": {
    "argumentDefinitions": [
      (v0/*:: as any*/),
      (v1/*:: as any*/),
      (v2/*:: as any*/)
    ],
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsFeedPageQuery",
    "selections": [
      {
        "args": null,
        "kind": "FragmentSpread",
        "name": "SnsSocialFrame_query"
      },
      {
        "args": (v3/*:: as any*/),
        "kind": "FragmentSpread",
        "name": "SnsTimeline_query"
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
    "name": "SnsFeedPageQuery",
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "User",
        "kind": "LinkedField",
        "name": "viewer",
        "plural": false,
        "selections": (v5/*:: as any*/),
        "storageKey": null
      },
      {
        "alias": null,
        "args": (v3/*:: as any*/),
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
              (v4/*:: as any*/),
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
                "selections": (v5/*:: as any*/),
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
    "cacheID": "42f7566df85895d9a4256375cda85319",
    "id": null,
    "metadata": {},
    "name": "SnsFeedPageQuery",
    "operationKind": "query",
    "text": "query SnsFeedPageQuery(\n  $topic: String!\n  $search: String!\n  $page: Int!\n) {\n  ...SnsSocialFrame_query\n  ...SnsTimeline_query_41NAMG\n}\n\nfragment SnsAvatar_user on User {\n  id\n  photo\n  avatar\n}\n\nfragment SnsComposer_viewer on User {\n  name\n  ...SnsAvatar_user\n}\n\nfragment SnsPost_post on Post {\n  id\n  body\n  topic\n  likes\n  liked\n  createdAt\n  author {\n    name\n    handle\n    ...SnsAvatar_user\n    id\n  }\n}\n\nfragment SnsSocialFrame_query on Query {\n  viewer {\n    name\n    handle\n    ...SnsAvatar_user\n    id\n  }\n}\n\nfragment SnsTimeline_query_41NAMG on Query {\n  viewer {\n    ...SnsComposer_viewer\n    id\n  }\n  feed(topic: $topic, search: $search, page: $page) {\n    posts {\n      id\n      ...SnsPost_post\n    }\n    hasNext\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "553a79a377d1671edebed8ce93322e7d";

export default ((node/*:: as any*/)/*:: as Query<
  SnsFeedPageQuery$variables,
  SnsFeedPageQuery$data,
>*/);
