// The frontend's mock data. Everything here comes from the checked-in
// synthetic snapshot fixtures (fixtures/snapshots/*.json), which are
// schema-locked by BOTH the Rust tests and the frontend tests — the UI never
// hand-writes network state, and every byte of mock data is labeled.
//
// All five snapshot fixtures are loaded (WO-04 Stage A scenario registry):
// each pairs with a topology golden of the same name (lib/topology.ts), and
// the registry (lib/scenarios.ts) is the single place that describes them
// for presentation.

import workstationJson from '../../../../fixtures/snapshots/workstation.json';
import incompleteJson from '../../../../fixtures/snapshots/incomplete-observations.json';
import serverTunnelJson from '../../../../fixtures/snapshots/server-tunnel.json';
import dummyVirtualJson from '../../../../fixtures/snapshots/dummy-virtual.json';
import emptyJson from '../../../../fixtures/snapshots/empty.json';
import { parseSnapshot } from './snapshot';
import type { NetworkSnapshot } from '../types';

export type FixtureName =
  | 'workstation'
  | 'incomplete-observations'
  | 'server-tunnel'
  | 'dummy-virtual'
  | 'empty';

export interface FixtureSource {
  name: FixtureName;
  description: string;
  snapshot: NetworkSnapshot;
}

const rawFixtures: Record<FixtureName, { description: string; json: unknown }> = {
  workstation: {
    description: 'Synthetic laptop: ethernet + Wi-Fi + WireGuard + bridge + dummy',
    json: workstationJson,
  },
  'incomplete-observations': {
    description:
      'Synthetic degraded case: netlink unavailable, only sysfs/procfs observed',
    json: incompleteJson,
  },
  'server-tunnel': {
    description:
      'Synthetic server: tunnel default (no gateway) alongside an ethernet default',
    json: serverTunnelJson,
  },
  'dummy-virtual': {
    description:
      'Synthetic virtual structures: bridge port, vlan parent, veth pair, dummy',
    json: dummyVirtualJson,
  },
  empty: {
    description: 'Synthetic empty observation: no interfaces, addresses, or routes',
    json: emptyJson,
  },
};

export const fixtures: Record<FixtureName, FixtureSource> = {
  workstation: {
    name: 'workstation',
    description: rawFixtures.workstation.description,
    snapshot: parseSnapshot(rawFixtures.workstation.json),
  },
  'incomplete-observations': {
    name: 'incomplete-observations',
    description: rawFixtures['incomplete-observations'].description,
    snapshot: parseSnapshot(rawFixtures['incomplete-observations'].json),
  },
  'server-tunnel': {
    name: 'server-tunnel',
    description: rawFixtures['server-tunnel'].description,
    snapshot: parseSnapshot(rawFixtures['server-tunnel'].json),
  },
  'dummy-virtual': {
    name: 'dummy-virtual',
    description: rawFixtures['dummy-virtual'].description,
    snapshot: parseSnapshot(rawFixtures['dummy-virtual'].json),
  },
  empty: {
    name: 'empty',
    description: rawFixtures.empty.description,
    snapshot: parseSnapshot(rawFixtures.empty.json),
  },
};

export function fixtureNames(): FixtureName[] {
  return Object.keys(fixtures) as FixtureName[];
}
