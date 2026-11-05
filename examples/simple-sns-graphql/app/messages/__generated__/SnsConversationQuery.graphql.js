/**
 * @generated SignedSource<<e4502ba395d97011e3bcc8a1375842ca>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { SnsConversation_query$fragmentType } from "./SnsConversation_query.graphql";
export type SnsConversationQuery$variables = {
  thread: string,
};
export type SnsConversationQuery$data = {
  readonly $fragmentSpreads: SnsConversation_query$fragmentType,
};
export type SnsConversationQuery = {
  response: SnsConversationQuery$data,
  variables: SnsConversationQuery$variables,
};
*/

var node/*: ConcreteRequest*/ = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "thread"
  }
],
v1 = {
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
    "name": "SnsConversationQuery",
    "selections": [
      {
        "args": [
          {
            "kind": "Variable",
            "name": "thread",
            "variableName": "thread"
          }
        ],
        "kind": "FragmentSpread",
        "name": "SnsConversation_query"
      }
    ],
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "SnsConversationQuery",
    "selections": [
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
              (v1/*:: as any*/),
              {
                "alias": null,
                "args": null,
                "concreteType": "User",
                "kind": "LinkedField",
                "name": "participant",
                "plural": false,
                "selections": [
                  {
                    "alias": null,
                    "args": null,
                    "kind": "ScalarField",
                    "name": "name",
                    "storageKey": null
                  },
                  (v1/*:: as any*/)
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
              (v1/*:: as any*/),
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "author",
                "storageKey": null
              },
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
  "params": {
    "cacheID": "102e7639bdf1184990847fdcd3de8a45",
    "id": null,
    "metadata": {},
    "name": "SnsConversationQuery",
    "operationKind": "query",
    "text": "query SnsConversationQuery(\n  $thread: ID!\n) {\n  ...SnsConversation_query_2omOIA\n}\n\nfragment SnsConversation_conversation on Conversation {\n  thread {\n    id\n    participant {\n      name\n      id\n    }\n  }\n  messages {\n    id\n    ...SnsMessage_message\n  }\n}\n\nfragment SnsConversation_query_2omOIA on Query {\n  conversation(id: $thread) {\n    thread {\n      id\n    }\n    ...SnsConversation_conversation\n  }\n}\n\nfragment SnsMessage_message on Message {\n  author\n  body\n  sentAt\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "81b42d3f2676f3c3d9e12cdfcdd73f86";

export default ((node/*:: as any*/)/*:: as Query<
  SnsConversationQuery$variables,
  SnsConversationQuery$data,
>*/);
