/**
 * @generated SignedSource<<87dd0d001ee64e8114eee7633c388312>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { SnsScreen_query$fragmentType } from "./SnsScreen_query.graphql";
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
  readonly $fragmentSpreads: SnsScreen_query$fragmentType,
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
  "kind": "Variable",
  "name": "page",
  "variableName": "page"
},
v8 = {
  "kind": "Variable",
  "name": "search",
  "variableName": "search"
},
v9 = {
  "kind": "Variable",
  "name": "topic",
  "variableName": "topic"
},
v10 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "name",
  "storageKey": null
},
v11 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
},
v12 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "photo",
  "storageKey": null
},
v13 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "avatar",
  "storageKey": null
},
v14 = [
  (v10/*:: as any*/),
  {
    "alias": null,
    "args": null,
    "kind": "ScalarField",
    "name": "handle",
    "storageKey": null
  },
  (v11/*:: as any*/),
  (v12/*:: as any*/),
  (v13/*:: as any*/)
],
v15 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "body",
  "storageKey": null
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
      {
        "args": [
          {
            "kind": "Variable",
            "name": "feed",
            "variableName": "feed"
          },
          {
            "kind": "Variable",
            "name": "messages",
            "variableName": "messages"
          },
          (v7/*:: as any*/),
          (v8/*:: as any*/),
          {
            "kind": "Variable",
            "name": "settings",
            "variableName": "settings"
          },
          {
            "kind": "Variable",
            "name": "thread",
            "variableName": "thread"
          },
          (v9/*:: as any*/)
        ],
        "kind": "FragmentSpread",
        "name": "SnsScreen_query"
      }
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
      {
        "alias": null,
        "args": null,
        "concreteType": "User",
        "kind": "LinkedField",
        "name": "viewer",
        "plural": false,
        "selections": (v14/*:: as any*/),
        "storageKey": null
      },
      {
        "condition": "feed",
        "kind": "Condition",
        "passingValue": true,
        "selections": [
          {
            "alias": null,
            "args": [
              (v7/*:: as any*/),
              (v8/*:: as any*/),
              (v9/*:: as any*/)
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
                  (v11/*:: as any*/),
                  (v15/*:: as any*/),
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
                    "selections": (v14/*:: as any*/),
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
      {
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
            "selections": [
              (v11/*:: as any*/),
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "lastMessage",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "concreteType": "User",
                "kind": "LinkedField",
                "name": "participant",
                "plural": false,
                "selections": [
                  (v10/*:: as any*/),
                  (v11/*:: as any*/),
                  (v12/*:: as any*/),
                  (v13/*:: as any*/)
                ],
                "storageKey": null
              }
            ],
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
                "selections": [
                  (v11/*:: as any*/),
                  {
                    "alias": null,
                    "args": null,
                    "concreteType": "User",
                    "kind": "LinkedField",
                    "name": "participant",
                    "plural": false,
                    "selections": [
                      (v10/*:: as any*/),
                      (v11/*:: as any*/)
                    ],
                    "storageKey": null
                  }
                ],
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
                  (v11/*:: as any*/),
                  {
                    "alias": null,
                    "args": null,
                    "kind": "ScalarField",
                    "name": "author",
                    "storageKey": null
                  },
                  (v15/*:: as any*/),
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
      {
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
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "displayName",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "bio",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "email",
                "storageKey": null
              },
              (v11/*:: as any*/)
            ],
            "storageKey": null
          }
        ]
      }
    ]
  },
  "params": {
    "cacheID": "2c2a7da4b13f72ead29afeba1fea0be0",
    "id": null,
    "metadata": {},
    "name": "SnsScreenQuery",
    "operationKind": "query",
    "text": "query SnsScreenQuery(\n  $topic: String!\n  $search: String!\n  $page: Int!\n  $thread: ID!\n  $feed: Boolean!\n  $messages: Boolean!\n  $settings: Boolean!\n) {\n  ...SnsScreen_query_3nq1oD\n}\n\nfragment SnsAvatar_user on User {\n  id\n  photo\n  avatar\n}\n\nfragment SnsComposer_viewer on User {\n  name\n  ...SnsAvatar_user\n}\n\nfragment SnsConversation_conversation on Conversation {\n  thread {\n    id\n    participant {\n      name\n      id\n    }\n  }\n  messages {\n    id\n    ...SnsMessage_message\n  }\n}\n\nfragment SnsInbox_query_2omOIA on Query {\n  viewer {\n    id\n  }\n  threads {\n    id\n    ...SnsThread_thread\n  }\n  conversation(id: $thread) {\n    thread {\n      id\n    }\n    ...SnsConversation_conversation\n  }\n}\n\nfragment SnsMessage_message on Message {\n  author\n  body\n  sentAt\n}\n\nfragment SnsPost_post on Post {\n  id\n  body\n  topic\n  likes\n  liked\n  createdAt\n  author {\n    name\n    handle\n    ...SnsAvatar_user\n    id\n  }\n}\n\nfragment SnsScreen_query_3nq1oD on Query {\n  ...SnsSocialFrame_query\n  ...SnsTimeline_query_41NAMG @include(if: $feed)\n  ...SnsInbox_query_2omOIA @include(if: $messages)\n  ...SnsSettings_query @include(if: $settings)\n}\n\nfragment SnsSettings_query on Query {\n  settings {\n    displayName\n    bio\n    email\n    id\n  }\n}\n\nfragment SnsSocialFrame_query on Query {\n  viewer {\n    name\n    handle\n    ...SnsAvatar_user\n    id\n  }\n}\n\nfragment SnsThread_thread on Thread {\n  id\n  lastMessage\n  participant {\n    name\n    ...SnsAvatar_user\n    id\n  }\n}\n\nfragment SnsTimeline_query_41NAMG on Query {\n  viewer {\n    ...SnsComposer_viewer\n    id\n  }\n  feed(topic: $topic, search: $search, page: $page) {\n    posts {\n      id\n      ...SnsPost_post\n    }\n    hasNext\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "8f26ad60acc48095e5f1b4ae0dfe5d6c";

export default ((node/*:: as any*/)/*:: as Query<
  SnsScreenQuery$variables,
  SnsScreenQuery$data,
>*/);
