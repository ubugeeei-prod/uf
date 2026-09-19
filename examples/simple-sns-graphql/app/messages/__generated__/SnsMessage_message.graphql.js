/**
 * @generated SignedSource<<437c8e2262dfec6f918bea1246e871d5>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { Fragment, ReaderFragment } from 'relay-runtime';
import type { FragmentType } from "relay-runtime";
declare export opaque type SnsMessage_message$fragmentType: FragmentType;
export type SnsMessage_message$data = {
  readonly author: string,
  readonly body: string,
  readonly sentAt: string,
  readonly $fragmentType: SnsMessage_message$fragmentType,
};
export type SnsMessage_message$key = {
  readonly $data?: SnsMessage_message$data,
  readonly $fragmentSpreads: SnsMessage_message$fragmentType,
  ...
};
*/

var node/*: ReaderFragment*/ = {
  "argumentDefinitions": [],
  "kind": "Fragment",
  "metadata": null,
  "name": "SnsMessage_message",
  "selections": [
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
  "type": "Message",
  "abstractKey": null
};

(node/*:: as any*/).hash = "36b98a52b3de3f1d76c44cedc723a31c";

export default ((node/*:: as any*/)/*:: as Fragment<
  SnsMessage_message$fragmentType,
  SnsMessage_message$data,
>*/);
