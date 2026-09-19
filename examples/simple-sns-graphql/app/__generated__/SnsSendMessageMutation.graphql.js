/**
 * @generated SignedSource<<24b3f51664f47318e0a3b93f19d820b3>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Mutation } from 'relay-runtime';
export type MessageInput = {
  body: string,
  requestId: string,
  threadId: string,
};
export type SnsSendMessageMutation$variables = {
  input: MessageInput,
};
export type SnsSendMessageMutation$data = {
  readonly sendMessage: {
    readonly author: string,
    readonly body: string,
    readonly id: string,
    readonly sentAt: string,
    readonly threadId: string,
  },
};
export type SnsSendMessageMutation = {
  response: SnsSendMessageMutation$data,
  variables: SnsSendMessageMutation$variables,
};
*/

var node/*: ConcreteRequest*/ = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "input"
  }
],
v1 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Variable",
        "name": "input",
        "variableName": "input"
      }
    ],
    "concreteType": "Message",
    "kind": "LinkedField",
    "name": "sendMessage",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "id",
        "storageKey": null
      },
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
];
return {
  "fragment": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsSendMessageMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "SnsSendMessageMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "65ca016896675fb4fe29568aaaba9dc6",
    "id": null,
    "metadata": {},
    "name": "SnsSendMessageMutation",
    "operationKind": "mutation",
    "text": "mutation SnsSendMessageMutation(\n  $input: MessageInput!\n) {\n  sendMessage(input: $input) {\n    id\n    threadId\n    author\n    body\n    sentAt\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "f7d059ff6edd60f267bafd1e4b3dc683";

export default ((node/*:: as any*/)/*:: as Mutation<
  SnsSendMessageMutation$variables,
  SnsSendMessageMutation$data,
>*/);
