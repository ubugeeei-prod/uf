/**
 * @generated SignedSource<<c926b61a1c0fc7405ecbed91b738c376>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ReaderFragment, RefetchableFragment } from 'relay-runtime';
import type { SnsConversation_conversation$fragmentType } from "./SnsConversation_conversation.graphql";
import type { FragmentType } from "relay-runtime";
declare export opaque type SnsConversation_query$fragmentType: FragmentType;
type SnsConversationRefetchQuery$variables = any;
export type SnsConversation_query$data = {
  readonly conversation: ?{
    readonly thread: {
      readonly id: string,
    },
    readonly $fragmentSpreads: SnsConversation_conversation$fragmentType,
  },
  readonly $fragmentType: SnsConversation_query$fragmentType,
};
export type SnsConversation_query$key = {
  readonly $data?: SnsConversation_query$data,
  readonly $fragmentSpreads: SnsConversation_query$fragmentType,
  ...
};
*/

import SnsConversationRefetchQuery_graphql from './SnsConversationRefetchQuery.graphql';

var node/*: ReaderFragment*/ = {
  "argumentDefinitions": [
    {
      "defaultValue": null,
      "kind": "LocalArgument",
      "name": "thread"
    }
  ],
  "kind": "Fragment",
  "metadata": {
    "refetch": {
      "connection": null,
      "fragmentPathInResult": [],
      "operation": SnsConversationRefetchQuery_graphql
    }
  },
  "name": "SnsConversation_query",
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
            {
              "alias": null,
              "args": null,
              "kind": "ScalarField",
              "name": "id",
              "storageKey": null
            }
          ],
          "storageKey": null
        },
        {
          "args": null,
          "kind": "FragmentSpread",
          "name": "SnsConversation_conversation"
        }
      ],
      "storageKey": null
    }
  ],
  "type": "Query",
  "abstractKey": null
};

(node/*:: as any*/).hash = "58bac30ca64dc09c6dc3f7a2e3968388";

export default ((node/*:: as any*/)/*:: as RefetchableFragment<
  SnsConversation_query$fragmentType,
  SnsConversation_query$data,
  SnsConversationRefetchQuery$variables,
>*/);
