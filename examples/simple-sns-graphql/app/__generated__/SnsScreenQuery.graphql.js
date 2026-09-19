/**
 * @generated SignedSource<<a3f6f98bc4b59f31422e7318f3ed5e62>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { SnsPost_post$fragmentType } from "./SnsPost_post.graphql";
export type SnsScreenQuery$variables = {
  feed: boolean,
  messages: boolean,
  page: number,
  search: string,
  settings: boolean,
  thread: string,
  topic: string,
};
export type SnsScreenQuery$data = {
  readonly conversation?: ?{
    readonly messages: ReadonlyArray<{
      readonly author: string,
      readonly body: string,
      readonly id: string,
      readonly sentAt: string,
      readonly threadId: string,
    }>,
    readonly thread: {
      readonly avatar: string,
      readonly handle: string,
      readonly id: string,
      readonly lastMessage: string,
      readonly name: string,
      readonly photo: ?string,
    },
  },
  readonly feed?: {
    readonly hasNext: boolean,
    readonly posts: ReadonlyArray<{
      readonly id: string,
      readonly $fragmentSpreads: SnsPost_post$fragmentType,
    }>,
  },
  readonly settings?: ?{
    readonly bio: string,
    readonly displayName: string,
    readonly email: string,
    readonly handle: string,
    readonly id: string,
  },
  readonly threads?: ReadonlyArray<{
    readonly avatar: string,
    readonly handle: string,
    readonly id: string,
    readonly lastMessage: string,
    readonly name: string,
    readonly photo: ?string,
  }>,
  readonly viewer: ?{
    readonly avatar: string,
    readonly bio: string,
    readonly handle: string,
    readonly id: string,
    readonly name: string,
    readonly photo: ?string,
  },
};
export type SnsScreenQuery = {
  response: SnsScreenQuery$data,
  variables: SnsScreenQuery$variables,
};
*/

var node/*: ConcreteRequest*/ = (function(){
var v0 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "feed"
},
v1 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "messages"
},
v2 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "page"
},
v3 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "search"
},
v4 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "settings"
},
v5 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "thread"
},
v6 = {
  "defaultValue": null,
  "kind": "LocalArgument",
  "name": "topic"
},
v7 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
},
v8 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "name",
  "storageKey": null
},
v9 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "handle",
  "storageKey": null
},
v10 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "avatar",
  "storageKey": null
},
v11 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "photo",
  "storageKey": null
},
v12 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "bio",
  "storageKey": null
},
v13 = [
  (v7/*:: as any*/),
  (v8/*:: as any*/),
  (v9/*:: as any*/),
  (v10/*:: as any*/),
  (v11/*:: as any*/),
  (v12/*:: as any*/)
],
v14 = {
  "alias": null,
  "args": null,
  "concreteType": "User",
  "kind": "LinkedField",
  "name": "viewer",
  "plural": false,
  "selections": (v13/*:: as any*/),
  "storageKey": null
},
v15 = [
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
v16 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "hasNext",
  "storageKey": null
},
v17 = [
  (v7/*:: as any*/),
  (v8/*:: as any*/),
  (v9/*:: as any*/),
  (v10/*:: as any*/),
  (v11/*:: as any*/),
  {
    "alias": null,
    "args": null,
    "kind": "ScalarField",
    "name": "lastMessage",
    "storageKey": null
  }
],
v18 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "body",
  "storageKey": null
},
v19 = {
  "condition": "messages",
  "kind": "Condition",
  "passingValue": true,
  "selections": [
    {
      "alias": null,
      "args": null,
      "concreteType": "Thread",
      "kind": "LinkedField",
      "name": "threads",
      "plural": true,
      "selections": (v17/*:: as any*/),
      "storageKey": null
    },
    {
      "alias": null,
      "args": [
        {
          "kind": "Variable",
          "name": "id",
          "variableName": "thread"
        }
      ],
      "concreteType": "Conversation",
      "kind": "LinkedField",
      "name": "conversation",
      "plural": false,
      "selections": [
        {
          "alias": null,
          "args": null,
          "concreteType": "Thread",
          "kind": "LinkedField",
          "name": "thread",
          "plural": false,
          "selections": (v17/*:: as any*/),
          "storageKey": null
        },
        {
          "alias": null,
          "args": null,
          "concreteType": "Message",
          "kind": "LinkedField",
          "name": "messages",
          "plural": true,
          "selections": [
            (v7/*:: as any*/),
            {
              "alias": null,
              "args": null,
              "kind": "ScalarField",
              "name": "threadId",
              "storageKey": null
            },
            {
              "alias": null,
              "args": null,
              "kind": "ScalarField",
              "name": "author",
              "storageKey": null
            },
            (v18/*:: as any*/),
            {
              "alias": null,
              "args": null,
              "kind": "ScalarField",
              "name": "sentAt",
              "storageKey": null
            }
          ],
          "storageKey": null
        }
      ],
      "storageKey": null
    }
  ]
},
v20 = {
  "condition": "settings",
  "kind": "Condition",
  "passingValue": true,
  "selections": [
    {
      "alias": null,
      "args": null,
      "concreteType": "Settings",
      "kind": "LinkedField",
      "name": "settings",
      "plural": false,
      "selections": [
        (v7/*:: as any*/),
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "displayName",
          "storageKey": null
        },
        (v9/*:: as any*/),
        (v12/*:: as any*/),
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "email",
          "storageKey": null
        }
      ],
      "storageKey": null
    }
  ]
};
return {
  "fragment": {
    "argumentDefinitions": [
      (v0/*:: as any*/),
      (v1/*:: as any*/),
      (v2/*:: as any*/),
      (v3/*:: as any*/),
      (v4/*:: as any*/),
      (v5/*:: as any*/),
      (v6/*:: as any*/)
    ],
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsScreenQuery",
    "selections": [
      (v14/*:: as any*/),
      {
        "condition": "feed",
        "kind": "Condition",
        "passingValue": true,
        "selections": [
          {
            "alias": null,
            "args": (v15/*:: as any*/),
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
                  (v7/*:: as any*/),
                  {
                    "args": null,
                    "kind": "FragmentSpread",
                    "name": "SnsPost_post"
                  }
                ],
                "storageKey": null
              },
              (v16/*:: as any*/)
            ],
            "storageKey": null
          }
        ]
      },
      (v19/*:: as any*/),
      (v20/*:: as any*/)
    ],
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [
      (v6/*:: as any*/),
      (v3/*:: as any*/),
      (v2/*:: as any*/),
      (v5/*:: as any*/),
      (v0/*:: as any*/),
      (v1/*:: as any*/),
      (v4/*:: as any*/)
    ],
    "kind": "Operation",
    "name": "SnsScreenQuery",
    "selections": [
      (v14/*:: as any*/),
      {
        "condition": "feed",
        "kind": "Condition",
        "passingValue": true,
        "selections": [
          {
            "alias": null,
            "args": (v15/*:: as any*/),
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
                  (v7/*:: as any*/),
                  (v18/*:: as any*/),
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
                    "selections": (v13/*:: as any*/),
                    "storageKey": null
                  }
                ],
                "storageKey": null
              },
              (v16/*:: as any*/)
            ],
            "storageKey": null
          }
        ]
      },
      (v19/*:: as any*/),
      (v20/*:: as any*/)
    ]
  },
  "params": {
    "cacheID": "4ae2c5bcd5e9f763c25380b778420f7a",
    "id": null,
    "metadata": {},
    "name": "SnsScreenQuery",
    "operationKind": "query",
    "text": "query SnsScreenQuery(\n  $topic: String!\n  $search: String!\n  $page: Int!\n  $thread: ID!\n  $feed: Boolean!\n  $messages: Boolean!\n  $settings: Boolean!\n) {\n  viewer {\n    id\n    name\n    handle\n    avatar\n    photo\n    bio\n  }\n  feed(topic: $topic, search: $search, page: $page) @include(if: $feed) {\n    posts {\n      id\n      ...SnsPost_post\n    }\n    hasNext\n  }\n  threads @include(if: $messages) {\n    id\n    name\n    handle\n    avatar\n    photo\n    lastMessage\n  }\n  conversation(id: $thread) @include(if: $messages) {\n    thread {\n      id\n      name\n      handle\n      avatar\n      photo\n      lastMessage\n    }\n    messages {\n      id\n      threadId\n      author\n      body\n      sentAt\n    }\n  }\n  settings @include(if: $settings) {\n    id\n    displayName\n    handle\n    bio\n    email\n  }\n}\n\nfragment SnsPost_post on Post {\n  id\n  body\n  topic\n  likes\n  liked\n  createdAt\n  author {\n    id\n    name\n    handle\n    avatar\n    photo\n    bio\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "ad193c55becfeeb66e66285949289b83";

export default ((node/*:: as any*/)/*:: as Query<
  SnsScreenQuery$variables,
  SnsScreenQuery$data,
>*/);
