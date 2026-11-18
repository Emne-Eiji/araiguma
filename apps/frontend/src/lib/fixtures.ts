// The frontend's mock data. Everything here comes from the checked-in
// synthetic snapshot fixtures (fixtures/snapshots/*.json), which are
// schema-locked by BOTH the Rust tests and the frontend tests — the UI never
// hand-writes network state, and every byte of mock data is labeled.

import workstationJson from '../../../../fixtures/snapshots/workstation.json';
import incompleteJson from '../../../../fixtures/snapshots/incomplete-observations.json';
import { parseSnapshot } from './snapshot';
import type { NetworkSnapshot } from '../types';

export type FixtureName = 'workstation' | 'incomplete-observations';

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
};

export function fixtureNames(): FixtureName[] {
  return Object.keys(fixtures) as FixtureName[];
}
