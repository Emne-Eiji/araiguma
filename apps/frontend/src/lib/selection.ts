// Selection re-resolution across document changes (WO-03-F / AUD-09).
//
// When the active documents change (import, or returning to the mock
// fixtures), a selection made against the PREVIOUS documents may not exist
// in the new ones. Rendering its details as if they belonged to the
// current observation would mislead — so every selection re-resolves BY
// IDENTITY against the new documents: what resolves keeps pointing at the
// (new object of the) same entity; what does not resolve is cleared.
// Pure and unit-tested; App.svelte applies it on every data-generation
// change.

import type { Interface, NetworkSnapshot, Route, TopologyDocument, TopologyNode } from '../types';
import { nodeIdKey } from './topology';
import { sameRouteIdentity } from './routes';

export interface SelectionState {
  interface: Interface | null;
  node: TopologyNode | null;
  route: Route | null;
}

export function reResolveSelections(
  selection: SelectionState,
  snapshot: NetworkSnapshot | null,
  topology: TopologyDocument | null,
): SelectionState {
  return {
    interface:
      selection.interface === null || snapshot === null
        ? null
        : (snapshot.interfaces.find(
            (iface) => iface.ifindex === selection.interface!.ifindex,
          ) ?? null),
    node:
      selection.node === null || topology === null
        ? null
        : (topology.nodes.find(
            (candidate) => nodeIdKey(candidate.id) === nodeIdKey(selection.node!.id),
          ) ?? null),
    route:
      selection.route === null || snapshot === null
        ? null
        : (snapshot.routes.find((candidate) => sameRouteIdentity(candidate, selection.route!)) ??
          null),
  };
}
