/**
 * @generated SignedSource<<edd1470998247610ed8bf38ba9cf852e>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ReaderFragment, RefetchableFragment } from 'relay-runtime';
import type { SnsThread_thread$fragmentType } from "./SnsThread_thread.graphql";
import type { FragmentType } from "relay-runtime";
declare export opaque type SnsThreads_query$fragmentType: FragmentType;
type SnsThreadsRefetchQuery$variables = any;
export type SnsThreads_query$data = {
  readonly threads: ReadonlyArray<{
    readonly id: string,
    readonly $fragmentSpreads: SnsThread_thread$fragmentType,
  }>,
  readonly $fragmentType: SnsThreads_query$fragmentType,
};
export type SnsThreads_query$key = {
  readonly $data?: SnsThreads_query$data,
  readonly $fragmentSpreads: SnsThreads_query$fragmentType,
  ...
};
*/

import SnsThreadsRefetchQuery_graphql from './SnsThreadsRefetchQuery.graphql';

var node/*: ReaderFragment*/ = {
  "argumentDefinitions": [],
  "kind": "Fragment",
  "metadata": {
    "refetch": {
      "connection": null,
      "fragmentPathInResult": [],
      "operation": SnsThreadsRefetchQuery_graphql
    }
  },
  "name": "SnsThreads_query",
  "selections": [
    {
      "alias": null,
      "args": null,
      "concreteType": "Thread",
      "kind": "LinkedField",
      "name": "threads",
      "plural": true,
      "selections": [
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "id",
          "storageKey": null
        },
        {
          "args": null,
          "kind": "FragmentSpread",
          "name": "SnsThread_thread"
        }
      ],
      "storageKey": null
    }
  ],
  "type": "Query",
  "abstractKey": null
};

(node/*:: as any*/).hash = "b2a6ed6704810daae46f74cc259a5230";

export default ((node/*:: as any*/)/*:: as RefetchableFragment<
  SnsThreads_query$fragmentType,
  SnsThreads_query$data,
  SnsThreadsRefetchQuery$variables,
>*/);
