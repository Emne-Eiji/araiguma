import { describe, expect, it } from 'vitest';
import {
  reResolveSelections,
  type SelectionState,
} from '../src/lib/selection';
import type { Interface, NetworkSnapshot, Route, TopologyDocument } from '../src/types';
import { topologyGoldens, defaultTopology } from '../src/lib/topology';
import { fixtures } from '../src/lib/fixtures';

// WO-03-F / AUD-09: selections from a previous document must never keep
// rendering as if they belonged to the current one. They re-resolve BY
// IDENTITY against the new documents; what does not resolve clears.

function snapshotWithRoutes(routes: Route[]): NetworkSnapshot {
  const base = fixtures.workstation.snapshot as NetworkSnapshot;
  return { ...base, routes };
}

const selectionWith = (overrides: Partial<SelectionState>): SelectionState => ({
  interface: null,
  node: null,
  route: null,
  ...overrides,
});

describe('selection re-resolution across document changes (AUD-09)', () => {
  it('clears a selected route that is absent from the new document', () => {
    const previous = fixtures.workstation.snapshot as NetworkSnapshot;
    const selectedRoute = previous.routes[0];
    const nextDoc: NetworkSnapshot = { ...previous, routes: [] };

    const resolved = reResolveSelections(
      selectionWith({ route: selectedRoute }),
      nextDoc,
      defaultTopology(),
    );
    expect(resolved.route === null, 'an absent route must clear the selection').toBe(true);
  });

  it('keeps a selection that resolves by identity in the new document', () => {
    const previous = fixtures.workstation.snapshot as NetworkSnapshot;
    const selectedRoute = previous.routes[0];
    // The same route, re-serialized: a NEW object with the same identity
    // (provenance fields may even differ).
    const sameRouteNewObject: Route = {
      ...structuredClone(selectedRoute),
      sources: selectedRoute.sources.slice(0, 1),
      notes: ['a fresh note from the new document'],
    };
    const nextDoc = snapshotWithRoutes([sameRouteNewObject, ...previous.routes.slice(1)]);

    const resolved = reResolveSelections(
      selectionWith({ route: selectedRoute }),
      nextDoc,
      defaultTopology(),
    );
    expect(resolved.route).toBe(sameRouteNewObject);
    expect(resolved.route?.notes).toContain('a fresh note from the new document');
  });

  it('does not mistake a different route sharing a superficial identifier for the selection', () => {
    const previous = fixtures.workstation.snapshot as NetworkSnapshot;
    const selectedRoute = previous.routes[0];
    // A different route with the same destination but a different gateway:
    // same "row" visually, different observed fact.
    const impostor: Route = {
      ...structuredClone(selectedRoute),
      gateway: selectedRoute.gateway === null ? '192.0.2.1' : null,
    };
    const nextDoc = snapshotWithRoutes([impostor]);

    const resolved = reResolveSelections(
      selectionWith({ route: selectedRoute }),
      nextDoc,
      defaultTopology(),
    );
    expect(
      resolved.route === null,
      'a superficially similar route is NOT the selection — identity requires the full discriminator set',
    ).toBe(true);
  });

  it('re-resolves interfaces by ifindex and clears missing ones', () => {
    const previous = fixtures.workstation.snapshot as NetworkSnapshot;
    const selected: Interface = structuredClone(previous.interfaces[0]);
    const withoutIt: NetworkSnapshot = {
      ...previous,
      interfaces: previous.interfaces.filter((i) => i.ifindex !== selected.ifindex),
    };

    expect(
      reResolveSelections(selectionWith({ interface: selected }), previous, defaultTopology())
        .interface,
    ).toBe(previous.interfaces[0]);
    expect(
      reResolveSelections(selectionWith({ interface: selected }), withoutIt, defaultTopology())
        .interface,
    ).toBeNull();
  });

  it('re-resolves topology nodes by id and clears missing ones', () => {
    const topology = topologyGoldens.find((t) => t.name === 'workstation')!.document;
    const node = topology.nodes.find((n) => n.nodeKind === 'interface')!;
    const withoutIt: TopologyDocument = {
      ...topology,
      nodes: topology.nodes.filter((n) => n.id !== node.id),
    };

    expect(
      reResolveSelections(selectionWith({ node }), fixtures.workstation.snapshot as NetworkSnapshot, topology)
        .node,
    ).toBe(node);
    expect(
      reResolveSelections(selectionWith({ node }), fixtures.workstation.snapshot as NetworkSnapshot, withoutIt)
        .node,
    ).toBeNull();
  });

  it('leaves empty selections untouched', () => {
    const resolved = reResolveSelections(
      selectionWith({}),
      fixtures.workstation.snapshot as NetworkSnapshot,
      defaultTopology(),
    );
    expect(resolved).toEqual({ interface: null, node: null, route: null });
  });
});
