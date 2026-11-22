import { beforeEach, describe, expect, it, vi } from 'vitest';
import workstation from '../../../fixtures/snapshots/workstation.json';
import workstationTopology from '../../../fixtures/topology/workstation.json';
import serverTunnel from '../../../fixtures/snapshots/server-tunnel.json';

const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  isTauri: vi.fn(),
  unlisten: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: tauri.invoke,
  isTauri: tauri.isTauri,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: tauri.listen,
}));

import {
  currentSnapshot,
  currentTopology,
  dataSource,
  enterLiveMode,
  importFile,
  liveStatus,
  returnToMock,
} from '../src/lib/dataMode.svelte';
import { initLiveMode, teardownLiveMode } from '../src/lib/liveMode';

type EventHandler = (event: { payload: unknown }) => void;

let eventHandler: EventHandler | undefined;
let snapshotResponse: unknown;
let topologyResponse: unknown;

function invokeCallCount(command: string): number {
  return tauri.invoke.mock.calls.filter(([name]) => name === command).length;
}

function lastInvokeCall(command: string): unknown[] | undefined {
  return [...tauri.invoke.mock.calls]
    .reverse()
    .find(([name]) => name === command);
}

function installDefaultMocks(): void {
  tauri.isTauri.mockReturnValue(true);
  tauri.unlisten.mockReset();
  tauri.listen.mockImplementation(async (_channel: string, handler: EventHandler) => {
    eventHandler = handler;
    return tauri.unlisten;
  });
  tauri.invoke.mockImplementation(async (command: string) => {
    switch (command) {
      case 'get_snapshot':
        return structuredClone(snapshotResponse);
      case 'get_topology':
        return structuredClone(topologyResponse);
      case 'subscribe_watch':
        return { subscriptionId: 'subscription-1' };
      case 'unsubscribe_watch':
        return null;
      default:
        throw new Error(`unexpected command: ${command}`);
    }
  });
}

async function emit(payload: unknown): Promise<void> {
  if (!eventHandler) throw new Error('live event listener was not installed');
  eventHandler({ payload });
}

describe('desktop live mode', () => {
  beforeEach(async () => {
    await teardownLiveMode();
    vi.clearAllMocks();

    dataSource.mode = 'mock';
    dataSource.snapshot = null;
    dataSource.topology = null;
    dataSource.mockScenario = 'workstation';
    dataSource.lastImport = null;
    dataSource.live = null;
    dataSource.liveTopology = null;
    dataSource.liveStale = false;
    dataSource.liveUnavailable = null;
    dataSource.liveWatchState = 'starting';
    dataSource.liveMonitorReason = null;
    dataSource.liveRecoveryConfirmed = false;
    dataSource.liveFreshObservation = false;
    dataSource.generation = 0;

    eventHandler = undefined;
    snapshotResponse = workstation;
    topologyResponse = workstationTopology;
    installDefaultMocks();
  });

  it('does nothing in a browser and leaves mock mode active', async () => {
    tauri.isTauri.mockReturnValue(false);

    await initLiveMode();

    expect(tauri.listen).not.toHaveBeenCalled();
    expect(tauri.invoke).not.toHaveBeenCalled();
    expect(dataSource.mode).toBe('mock');
  });

  it('installs the listener before subscribing and loads a coherent live pair', async () => {
    await initLiveMode();

    expect(dataSource.mode).toBe('live');
    expect(liveStatus()).toBe('live');
    expect(dataSource.live?.timestampMs).toBe(workstation.timestampMs);
    expect(dataSource.liveTopology?.sourceTimestampMs).toBe(workstation.timestampMs);

    expect(tauri.listen).toHaveBeenCalledWith('araiguma/event', expect.any(Function));
    expect(tauri.invoke).toHaveBeenCalledWith('get_snapshot');
    expect(tauri.invoke).toHaveBeenCalledWith('get_topology', {
      snapshot: expect.objectContaining({ timestampMs: workstation.timestampMs }),
    });

    const subscribeCall = tauri.invoke.mock.calls.findIndex(
      ([command]) => command === 'subscribe_watch',
    );
    expect(subscribeCall).toBeGreaterThanOrEqual(0);
    expect(tauri.listen.mock.invocationCallOrder[0]).toBeLessThan(
      tauri.invoke.mock.invocationCallOrder[subscribeCall],
    );
  });

  it('does not report LIVE until the backend watch is active', async () => {
    let resolveSubscribe: ((response: { subscriptionId: string }) => void) | undefined;
    const pendingSubscribe = new Promise<{ subscriptionId: string }>((resolve) => {
      resolveSubscribe = resolve;
    });
    tauri.invoke.mockImplementation(async (command: string) => {
      switch (command) {
        case 'get_snapshot':
          return structuredClone(snapshotResponse);
        case 'get_topology':
          return structuredClone(topologyResponse);
        case 'subscribe_watch':
          return pendingSubscribe;
        case 'unsubscribe_watch':
          return null;
        default:
          throw new Error(`unexpected command: ${command}`);
      }
    });

    const initialization = initLiveMode();
    await vi.waitFor(() => {
      expect(dataSource.live).not.toBeNull();
    });

    expect(liveStatus()).toBe('stale');
    expect(dataSource.liveWatchState).toBe('starting');

    resolveSubscribe?.({ subscriptionId: 'subscription-1' });
    await initialization;

    expect(dataSource.liveWatchState).toBe('active');
    expect(liveStatus()).toBe('live');
  });

  it('does not erase a degradation that arrives before subscribe resolves', async () => {
    let resolveSubscribe: ((response: { subscriptionId: string }) => void) | undefined;
    const pendingSubscribe = new Promise<{ subscriptionId: string }>((resolve) => {
      resolveSubscribe = resolve;
    });
    tauri.invoke.mockImplementation(async (command: string) => {
      switch (command) {
        case 'get_snapshot':
          return structuredClone(snapshotResponse);
        case 'get_topology':
          return structuredClone(topologyResponse);
        case 'subscribe_watch':
          return pendingSubscribe;
        case 'unsubscribe_watch':
          return null;
        default:
          throw new Error(`unexpected command: ${command}`);
      }
    });

    const initialization = initLiveMode();
    await vi.waitFor(() => {
      expect(dataSource.live).not.toBeNull();
    });
    await emit({
      event: 'monitorHealth',
      seq: 1,
      monitor: { state: 'degraded', reason: 'the event monitor disconnected' },
    });
    await vi.waitFor(() => {
      expect(dataSource.liveWatchState).toBe('degraded');
    });

    resolveSubscribe?.({ subscriptionId: 'subscription-1' });
    await initialization;

    expect(dataSource.liveWatchState).toBe('degraded');
    expect(liveStatus()).toBe('stale');
  });

  it('keeps a confirmed pair stale when the backend watch cannot be created', async () => {
    tauri.invoke.mockImplementation(async (command: string) => {
      switch (command) {
        case 'get_snapshot':
          return structuredClone(snapshotResponse);
        case 'get_topology':
          return structuredClone(topologyResponse);
        case 'subscribe_watch':
          throw new Error('monitoring unavailable');
        case 'unsubscribe_watch':
          return null;
        default:
          throw new Error(`unexpected command: ${command}`);
      }
    });

    await initLiveMode();

    expect(liveStatus()).toBe('stale');
    expect(dataSource.liveWatchState).toBe('failed');
    expect(dataSource.liveUnavailable).toMatch(/monitoring unavailable/i);
  });

  it('marks a confirmed pair stale when the event monitor disconnects', async () => {
    await initLiveMode();
    const confirmedTimestamp = dataSource.live?.timestampMs;

    await emit({
      event: 'monitorHealth',
      seq: 1,
      monitor: {
        state: 'degraded',
        reason: 'the netlink event monitor disconnected; reconnecting',
      },
    });

    await vi.waitFor(() => {
      expect(liveStatus()).toBe('stale');
    });
    expect(dataSource.live?.timestampMs).toBe(confirmedTimestamp);
    expect(dataSource.liveWatchState).toBe('degraded');
    expect(dataSource.liveMonitorReason).toMatch(/disconnected/i);
  });

  it('keeps a monitor without a confirmed pair unavailable', async () => {
    await initLiveMode();
    enterLiveMode();

    await emit({
      event: 'monitorHealth',
      seq: 1,
      monitor: {
        state: 'degraded',
        reason: 'the netlink event monitor disconnected; reconnecting',
      },
    });

    await vi.waitFor(() => {
      expect(liveStatus()).toBe('unavailable');
    });
    expect(dataSource.live).toBeNull();
    expect(dataSource.liveTopology).toBeNull();
    expect(dataSource.liveMonitorReason).toMatch(/disconnected/i);
  });

  it('reports a failed reconnect and stays stale until recovery', async () => {
    await initLiveMode();

    await emit({
      event: 'monitorHealth',
      seq: 1,
      monitor: { state: 'degraded', reason: 'the event monitor disconnected' },
    });
    await emit({
      event: 'monitorHealth',
      seq: 2,
      monitor: {
        state: 'degraded',
        reason: 'the event monitor reconnect failed: synthetic failure',
      },
    });

    await vi.waitFor(() => {
      expect(dataSource.liveMonitorReason).toMatch(/synthetic failure/i);
    });
    expect(liveStatus()).toBe('stale');
    expect(dataSource.live).not.toBeNull();
  });

  it('returns to LIVE only after a fresh resync and explicit recovery', async () => {
    await initLiveMode();

    await emit({
      event: 'monitorHealth',
      seq: 1,
      monitor: { state: 'degraded', reason: 'the event monitor disconnected' },
    });
    expect(liveStatus()).toBe('stale');

    const freshSnapshot = {
      ...workstation,
      timestampMs: workstation.timestampMs + 1,
    };
    snapshotResponse = freshSnapshot;
    topologyResponse = {
      ...workstationTopology,
      sourceTimestampMs: freshSnapshot.timestampMs,
    };

    await emit({
      event: 'resynchronized',
      seq: 2,
      snapshot: freshSnapshot,
      changes: [],
    });
    await vi.waitFor(() => {
      expect(dataSource.live?.timestampMs).toBe(freshSnapshot.timestampMs);
    });
    expect(liveStatus()).toBe('stale');

    await emit({
      event: 'monitorHealth',
      seq: 3,
      monitor: { state: 'recovered' },
    });
    await vi.waitFor(() => {
      expect(liveStatus()).toBe('live');
    });
    expect(dataSource.liveWatchState).toBe('active');
    expect(dataSource.liveMonitorReason).toBeNull();
  });

  it('does not return to LIVE on recovery without a fresh resynchronization', async () => {
    await initLiveMode();
    const confirmedTimestamp = dataSource.live?.timestampMs;

    await emit({
      event: 'monitorHealth',
      seq: 1,
      monitor: { state: 'degraded', reason: 'the event monitor disconnected' },
    });
    await emit({
      event: 'monitorHealth',
      seq: 2,
      monitor: { state: 'recovered' },
    });

    await vi.waitFor(() => {
      expect(dataSource.liveRecoveryConfirmed).toBe(true);
    });
    expect(liveStatus()).toBe('stale');
    expect(dataSource.live?.timestampMs).toBe(confirmedTimestamp);
    expect(dataSource.liveFreshObservation).toBe(false);
    expect(dataSource.liveMonitorReason).toMatch(/fresh live observation/i);
  });

  it('removes a listener that finishes installing after teardown', async () => {
    let resolveListen: ((unlisten: () => void) => void) | undefined;
    const pendingListen = new Promise<() => void>((resolve) => {
      resolveListen = resolve;
    });
    tauri.listen.mockImplementation(async () => pendingListen);

    const initialization = initLiveMode();
    await vi.waitFor(() => {
      expect(tauri.listen).toHaveBeenCalledOnce();
    });

    await teardownLiveMode();
    resolveListen?.(tauri.unlisten);
    await initialization;

    expect(tauri.unlisten).toHaveBeenCalledOnce();
  });

  it('cancels a subscription that arrives after teardown and can retry a failed cancellation', async () => {
    let resolveSubscribe: ((response: { subscriptionId: string }) => void) | undefined;
    const pendingSubscribe = new Promise<{ subscriptionId: string }>((resolve) => {
      resolveSubscribe = resolve;
    });
    let unsubscribeAttempts = 0;
    tauri.invoke.mockImplementation(async (command: string) => {
      switch (command) {
        case 'get_snapshot':
          return structuredClone(snapshotResponse);
        case 'get_topology':
          return structuredClone(topologyResponse);
        case 'subscribe_watch':
          return pendingSubscribe;
        case 'unsubscribe_watch':
          unsubscribeAttempts += 1;
          if (unsubscribeAttempts === 1) {
            throw new Error('temporary cancellation failure');
          }
          return null;
        default:
          throw new Error(`unexpected command: ${command}`);
      }
    });

    const initialization = initLiveMode();
    await vi.waitFor(() => {
      expect(tauri.invoke).toHaveBeenCalledWith('subscribe_watch');
    });

    const teardown = teardownLiveMode();
    resolveSubscribe?.({ subscriptionId: 'late-subscription' });
    await initialization;
    await teardown;

    expect(unsubscribeAttempts).toBe(1);
    expect(dataSource.liveUnavailable).toMatch(/temporary cancellation failure/i);

    await teardownLiveMode();
    expect(unsubscribeAttempts).toBe(2);
  });

  it('ignores duplicate and backward global sequence numbers', async () => {
    await initLiveMode();
    await emit({ event: 'snapshot', seq: 10, snapshot: workstation });
    await vi.waitFor(() => {
      expect(dataSource.live?.timestampMs).toBe(workstation.timestampMs);
    });

    const readsBefore = invokeCallCount('get_snapshot');
    await emit({ event: 'changed', seq: 10, changes: [], notifiedBy: ['newLink'] });
    await emit({ event: 'changed', seq: 9, changes: [], notifiedBy: ['newRoute'] });
    await emit({ event: 'changed', seq: 11, changes: [], notifiedBy: ['newAddress'] });

    await vi.waitFor(() => {
      expect(invokeCallCount('get_snapshot')).toBe(readsBefore + 1);
    });
    expect(invokeCallCount('get_snapshot')).toBe(readsBefore + 1);
  });

  it('refreshes from a fresh snapshot when a changed envelope arrives', async () => {
    await initLiveMode();
    await emit({ event: 'snapshot', seq: 1, snapshot: workstation });

    const freshSnapshot = {
      ...workstation,
      timestampMs: workstation.timestampMs + 1,
    };
    const freshTopology = {
      ...workstationTopology,
      sourceTimestampMs: freshSnapshot.timestampMs,
    };
    snapshotResponse = freshSnapshot;
    topologyResponse = freshTopology;

    await emit({ event: 'changed', seq: 2, changes: [], notifiedBy: ['newLink'] });

    await vi.waitFor(() => {
      expect(dataSource.live?.timestampMs).toBe(freshSnapshot.timestampMs);
      expect(liveStatus()).toBe('live');
    });
    expect(invokeCallCount('get_snapshot')).toBe(2);
    expect(lastInvokeCall('get_topology')).toEqual([
      'get_topology',
      { snapshot: dataSource.live },
    ]);
  });

  it('recovers snapshot-first when the event sequence has a gap', async () => {
    await initLiveMode();
    await emit({ event: 'snapshot', seq: 10, snapshot: workstation });

    const snapshotsBefore = invokeCallCount('get_snapshot');
    await emit({ event: 'changed', seq: 12, changes: [], notifiedBy: ['newRoute'] });

    await vi.waitFor(() => {
      expect(invokeCallCount('get_snapshot')).toBe(snapshotsBefore + 1);
      expect(liveStatus()).toBe('live');
    });
  });

  it('does not lose a recovery fact across a sequence gap', async () => {
    await initLiveMode();
    await emit({
      event: 'monitorHealth',
      seq: 1,
      monitor: { state: 'degraded', reason: 'the event monitor disconnected' },
    });

    const freshSnapshot = {
      ...workstation,
      timestampMs: workstation.timestampMs + 1,
    };
    snapshotResponse = freshSnapshot;
    topologyResponse = {
      ...workstationTopology,
      sourceTimestampMs: freshSnapshot.timestampMs,
    };

    // Sequence 2 was missed. The recovery fact at sequence 3 must still be
    // applied, but only after the gap-triggered fresh pair is installed.
    await emit({
      event: 'monitorHealth',
      seq: 3,
      monitor: { state: 'recovered' },
    });

    await vi.waitFor(() => {
      expect(liveStatus()).toBe('live');
    });
    expect(dataSource.live?.timestampMs).toBe(freshSnapshot.timestampMs);
    expect(dataSource.liveRecoveryConfirmed).toBe(true);
    expect(dataSource.liveFreshObservation).toBe(true);
  });

  it('uses a resynchronized envelope snapshot and refreshes its topology', async () => {
    await initLiveMode();
    await emit({ event: 'snapshot', seq: 1, snapshot: workstation });

    const resynchronizedSnapshot = {
      ...workstation,
      timestampMs: workstation.timestampMs + 1,
    };
    topologyResponse = {
      ...workstationTopology,
      sourceTimestampMs: resynchronizedSnapshot.timestampMs,
    };
    const topologyReadsBefore = invokeCallCount('get_topology');
    const snapshotReadsBefore = invokeCallCount('get_snapshot');

    await emit({
      event: 'resynchronized',
      seq: 2,
      snapshot: resynchronizedSnapshot,
      changes: [],
    });

    await vi.waitFor(() => {
      expect(dataSource.live?.timestampMs).toBe(resynchronizedSnapshot.timestampMs);
      expect(dataSource.liveTopology?.sourceTimestampMs).toBe(
        resynchronizedSnapshot.timestampMs,
      );
    });
    expect(invokeCallCount('get_topology')).toBe(topologyReadsBefore + 1);
    expect(invokeCallCount('get_snapshot')).toBe(snapshotReadsBefore);
    expect(lastInvokeCall('get_topology')).toEqual([
      'get_topology',
      { snapshot: dataSource.live },
    ]);
  });

  it('records unavailable when the initial snapshot is invalid', async () => {
    snapshotResponse = { ...workstation, schemaVersion: 99 };

    await initLiveMode();

    expect(dataSource.mode).toBe('live');
    expect(dataSource.live).toBeNull();
    expect(currentSnapshot()).toBeNull();
    expect(currentTopology()).toBeNull();
    expect(liveStatus()).toBe('unavailable');
    expect(dataSource.liveUnavailable).toMatch(/invalid snapshot/i);
  });

  it('keeps the last live pair and records stale when a refresh fails', async () => {
    await initLiveMode();
    await emit({ event: 'snapshot', seq: 1, snapshot: workstation });

    tauri.invoke.mockImplementation(async (command: string) => {
      if (command === 'get_snapshot') throw new Error('engine refresh failed');
      if (command === 'get_topology') return structuredClone(topologyResponse);
      if (command === 'subscribe_watch') return { subscriptionId: 'subscription-1' };
      if (command === 'unsubscribe_watch') return null;
      throw new Error(`unexpected command: ${command}`);
    });

    await emit({ event: 'changed', seq: 2, changes: [], notifiedBy: ['newLink'] });

    await vi.waitFor(() => {
      expect(liveStatus()).toBe('stale');
      expect(dataSource.live?.timestampMs).toBe(workstation.timestampMs);
    });
    expect(dataSource.liveUnavailable).toMatch(/engine refresh failed/i);
  });

  it('unlistens and cancels the backend subscription during teardown', async () => {
    await initLiveMode();

    await teardownLiveMode();

    expect(tauri.unlisten).toHaveBeenCalledOnce();
    expect(tauri.invoke).toHaveBeenCalledWith('unsubscribe_watch', {
      subscriptionId: 'subscription-1',
    });

    await teardownLiveMode();
    expect(tauri.unlisten).toHaveBeenCalledOnce();
    expect(invokeCallCount('unsubscribe_watch')).toBe(1);
  });

  it('keeps explicit import and mock modes active while live events continue', async () => {
    await initLiveMode();
    await emit({ event: 'snapshot', seq: 1, snapshot: workstation });

    importFile('server-tunnel.json', JSON.stringify(serverTunnel));
    expect(dataSource.mode).toBe('imported');
    expect(currentSnapshot()!.timestampMs).toBe(serverTunnel.timestampMs);

    const freshSnapshot = {
      ...workstation,
      timestampMs: workstation.timestampMs + 1,
    };
    snapshotResponse = freshSnapshot;
    topologyResponse = {
      ...workstationTopology,
      sourceTimestampMs: freshSnapshot.timestampMs,
    };
    await emit({ event: 'changed', seq: 2, changes: [], notifiedBy: ['newLink'] });

    await vi.waitFor(() => {
      expect(dataSource.live?.timestampMs).toBe(freshSnapshot.timestampMs);
    });
    expect(dataSource.mode).toBe('imported');
    expect(currentSnapshot()!.timestampMs).toBe(serverTunnel.timestampMs);

    returnToMock();
    expect(dataSource.mode).toBe('mock');
    await emit({ event: 'changed', seq: 3, changes: [], notifiedBy: ['newRoute'] });
    await vi.waitFor(() => {
      expect(invokeCallCount('get_snapshot')).toBeGreaterThan(2);
    });
    expect(dataSource.mode).toBe('mock');
    expect(currentSnapshot()!.timestampMs).not.toBe(freshSnapshot.timestampMs);
  });
});
