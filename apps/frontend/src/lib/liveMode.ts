// Live mode (desktop): the Svelte app runs inside the Tauri 2 window and
// subscribes to the engine's live event stream (document 04 §7). Live
// documents enter the same validating loaders as file import (doc 04 §5,
// binding) — one contract, one boundary, three transports (CLI stdout,
// file, IPC) per OD-03.
//
// The server-side engine (crates/engine) mirrors its envelopes 1:1 here —
// the IPC and desktop crates are the only ones allowed to know the
// transport details; the frontend validates every live document before
// rendering (lib/snapshot.ts).

import * as tauriCore from '@tauri-apps/api/core';
import { listen, type Event, type UnlistenFn } from '@tauri-apps/api/event';
import type { NetworkSnapshot, TopologyDocument } from '../types';
import {
  enterLiveMode,
  setLiveDocuments,
  setLiveMonitorDegraded,
  setLiveMonitorFreshObservation,
  setLiveMonitorRecovered,
  setLiveUnavailable,
  setLiveWatchActive,
  setLiveWatchFailed,
} from './dataMode.svelte';
import { parseSnapshot } from './snapshot';
import { parseTopology } from './topology';

const LIVE_EVENT_CHANNEL = 'araiguma/event';

/** Engine event envelopes (crates/engine/src/engine.rs, document 04 §7). */
export interface LiveEnvelope {
  event: 'snapshot' | 'changed' | 'resynchronized' | 'monitorHealth';
  seq: number;
  snapshot?: NetworkSnapshot;
  changes?: unknown[];
  notifiedBy?: string[];
  monitor?: {
    state: 'degraded' | 'recovered';
    reason?: string;
  };
}

/**
 * The IPC contract names the field `subscriptionId`. The snake-case alias
 * is accepted as well because a serde struct may emit its Rust field name;
 * accepting both keeps teardown truthful across that wire-shape difference
 * without inventing another id.
 */
export interface SubscribeWatchResponse {
  subscriptionId: string;
  subscription_id?: string;
}

export type LiveModeTeardown = () => Promise<void>;

interface LiveDocumentPair {
  snapshot: NetworkSnapshot;
  topology: TopologyDocument;
}

let activeTeardown: LiveModeTeardown | null = null;

/** True when running inside the Tauri 2 webview. */
export function isDesktopRuntime(): boolean {
  try {
    if (typeof tauriCore.isTauri === 'function' && tauriCore.isTauri()) return true;
  } catch {
    // Fall through to the internals probe below.
  }

  try {
    if ((globalThis as { isTauri?: unknown }).isTauri === true) return true;
    if (typeof window === 'undefined') return false;
    const internals = (
      window as unknown as {
        __TAURI_INTERNALS__?: {
          invoke?: unknown;
          transformCallback?: unknown;
        };
      }
    ).__TAURI_INTERNALS__;
    return (
      typeof internals?.invoke === 'function' ||
      typeof internals?.transformCallback === 'function'
    );
  } catch {
    return false;
  }
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}

/**
 * Tear down the active live-mode session. It is safe to call more than
 * once and safe to call from Svelte's `onDestroy` (where the returned
 * promise is deliberately not awaited).
 */
export function teardownLiveMode(): Promise<void> {
  const teardown = activeTeardown;
  if (!teardown) return Promise.resolve();
  return teardown();
}

/**
 * Initialize live mode.
 *
 * The listener is installed before `subscribe_watch`, because the first
 * engine envelope is emitted as part of creating the subscription. The
 * initial snapshot/topology pair is fetched before subscribing as well;
 * the subscription's opening snapshot then closes the small interval
 * between that pair and the watch without leaving an event gap.
 *
 * The returned function unlistens the event and cancels the backend
 * subscription. `teardownLiveMode` exposes the same operation for Svelte
 * lifecycle cleanup.
 */
export async function initLiveMode(): Promise<LiveModeTeardown> {
  if (activeTeardown) await teardownLiveMode();
  if (!isDesktopRuntime()) return async () => {};

  enterLiveMode();

  let disposed = false;
  let teardownPromise: Promise<void> | null = null;
  let subscriptionCleanupFailed = false;
  let unlisten: UnlistenFn | undefined;
  let subscriptionId: string | undefined;
  let subscriptionSettled: Promise<void> | null = null;
  let lastSeq: number | null = null;
  let eventQueue: Promise<void> = Promise.resolve();

  const teardown: LiveModeTeardown = async () => {
    if (teardownPromise) {
      await teardownPromise;
      if (!subscriptionCleanupFailed) return;
    }

    subscriptionCleanupFailed = false;
    disposed = true;
    const cleanup = (async () => {
      setLiveWatchFailed('The live watch was stopped.');

      const removeListener = unlisten;
      unlisten = undefined;
      try {
        removeListener?.();
      } catch (error) {
        setLiveUnavailable(`Live event listener cleanup failed: ${errorMessage(error)}`);
      }

      // If `subscribe_watch` was still in flight, wait for its id before
      // cancelling. Otherwise a late response could create a backend
      // subscription after teardown had already finished.
      if (subscriptionSettled) {
        await subscriptionSettled;
      }

      if (subscriptionId) {
        try {
          await tauriCore.invoke('unsubscribe_watch', { subscriptionId });
          subscriptionId = undefined;
        } catch (error) {
          subscriptionCleanupFailed = true;
          setLiveUnavailable(`Live watch cancellation failed: ${errorMessage(error)}`);
        }
      }
    })();
    teardownPromise = cleanup;
    await cleanup;

    if (!subscriptionCleanupFailed && activeTeardown === teardown) {
      activeTeardown = null;
    }
  };
  activeTeardown = teardown;

  /**
   * Fetch and validate a snapshot, then ask the backend to project that
   * exact snapshot. Projecting `current` instead could advance between
   * the two IPC calls and produce a topology from a different observation.
   */
  async function fetchLivePair(): Promise<LiveDocumentPair> {
    const snapshot = parseSnapshot(await tauriCore.invoke<unknown>('get_snapshot'));
    const topology = parseTopology(
      await tauriCore.invoke<unknown>('get_topology', {
        snapshot,
      }),
    );

    if (topology.sourceTimestampMs !== snapshot.timestampMs) {
      throw new Error(
        `live topology timestamp ${topology.sourceTimestampMs} does not match snapshot timestamp ${snapshot.timestampMs}`,
      );
    }
    return { snapshot, topology };
  }

  /** Refresh atomically; a failed refresh keeps the previous live pair. */
  async function refreshLivePair(reason: string): Promise<boolean> {
    if (disposed) return false;
    setLiveUnavailable(reason);

    try {
      const pair = await fetchLivePair();
      if (disposed) return false;
      setLiveDocuments(pair.snapshot, pair.topology);
      return true;
    } catch (error) {
      if (!disposed) {
        setLiveUnavailable(`Live refresh failed: ${errorMessage(error)}`);
      }
      return false;
    }
  }

  /**
   * Apply an embedded snapshot and refresh its topology. A topology from a
   * different observation is not accepted; the complete pair is fetched
   * again so the rendered documents remain coherent.
   */
  async function applyEmbeddedSnapshot(
    rawSnapshot: unknown,
    refreshTopology: boolean,
  ): Promise<boolean> {
    let snapshot: NetworkSnapshot;
    try {
      snapshot = parseSnapshot(rawSnapshot);
    } catch (error) {
      return refreshLivePair(
        `Live engine sent an invalid snapshot; recovering from a fresh snapshot: ${errorMessage(error)}`,
      );
    }

    setLiveUnavailable(
      refreshTopology
        ? 'Live engine resynchronized; refreshing the live topology.'
        : 'Live engine sent a snapshot; refreshing the live topology.',
    );

    try {
      const rawTopology = await tauriCore.invoke<unknown>('get_topology', {
        snapshot,
      });
      const topology = parseTopology(rawTopology);
      if (disposed) return false;

      if (topology.sourceTimestampMs !== snapshot.timestampMs) {
        return refreshLivePair(
          'Live topology did not match the engine snapshot; recovering with a coherent live pair.',
        );
      }
      setLiveDocuments(snapshot, topology);
      return true;
    } catch (error) {
      if (!disposed) {
        setLiveUnavailable(`Live topology refresh failed: ${errorMessage(error)}`);
      }
      return false;
    }
  }

  async function processMonitorHealth(envelope: LiveEnvelope): Promise<void> {
    const monitor = envelope.monitor;
    if (!monitor || (monitor.state !== 'degraded' && monitor.state !== 'recovered')) {
      await refreshLivePair(
        'Live engine sent an invalid monitor-health event; recovering from a fresh snapshot.',
      );
      return;
    }
    if (monitor.state === 'degraded') {
      setLiveMonitorDegraded(
        monitor.reason ?? 'The live event monitor is reconnecting.',
      );
    } else {
      setLiveMonitorRecovered();
    }
  }

  async function processEnvelope(envelope: LiveEnvelope | undefined): Promise<void> {
    if (disposed) return;

    if (
      !envelope ||
      !Number.isSafeInteger(envelope.seq) ||
      envelope.seq < 0
    ) {
      await refreshLivePair(
        'Live engine sent an invalid event envelope; recovering from a fresh snapshot.',
      );
      return;
    }

    const seq = envelope.seq;
    if (lastSeq !== null) {
      // The desktop currently has one global event stream but one delivery
      // pump per subscription. A second subscription can therefore deliver
      // the same global sequence again (or interleave an older envelope).
      // Neither case is a forward gap: it must not trigger a false refresh.
      if (seq <= lastSeq) return;
      if (seq !== lastSeq + 1) {
        const expected = lastSeq + 1;
        lastSeq = seq;
        const refreshed = await refreshLivePair(
          `Live event sequence gap (expected ${expected}, received ${seq}); recovering from a fresh snapshot.`,
        );

        // A gap must not discard a health fact. In particular, a recovery
        // envelope may be the first fact observed after a missed
        // resynchronization. The fresh pair above is post-recovery when
        // the gap-triggering envelope says `recovered`; record it before
        // applying that fact so LIVE is restored only with both conditions.
        if (envelope.event === 'monitorHealth') {
          if (refreshed && envelope.monitor?.state === 'recovered') {
            setLiveMonitorFreshObservation();
          }
          await processMonitorHealth(envelope);
        } else if (envelope.event === 'resynchronized' && refreshed) {
          setLiveMonitorFreshObservation();
        }
        return;
      }
    }
    lastSeq = seq;

    if (envelope.event === 'monitorHealth') {
      await processMonitorHealth(envelope);
      return;
    }

    if (envelope.event === 'snapshot') {
      await applyEmbeddedSnapshot(envelope.snapshot, false);
    } else if (envelope.event === 'resynchronized') {
      // The resynchronization envelope carries an authoritative full
      // snapshot. Use it, then refresh the topology from the engine. A
      // recovery signal can restore LIVE only after this coherent pair is
      // actually installed.
      const refreshed = await applyEmbeddedSnapshot(envelope.snapshot, true);
      if (refreshed) {
        setLiveMonitorFreshObservation();
      }
    } else if (envelope.event === 'changed') {
      // A changed envelope is a pure diff. The frontend never applies
      // diffs itself; it always asks for a fresh full snapshot (and the
      // matching topology) through the validated document boundary.
      await refreshLivePair(
        'Live engine reported changes; waiting for a fresh live snapshot.',
      );
    } else {
      await refreshLivePair(
        'Live engine sent an unknown event; recovering from a fresh snapshot.',
      );
    }
  }

  const onEvent = (event: Event<LiveEnvelope>): void => {
    eventQueue = eventQueue
      .then(() => processEnvelope(event.payload))
      .catch(async (error: unknown) => {
        await refreshLivePair(
          `Live event processing failed; recovering from a fresh snapshot: ${errorMessage(error)}`,
        );
      });
  };

  let listenerFailure: string | null = null;
  try {
    // Resolve `listen` before subscribing: `subscribe_watch` immediately
    // emits the opening snapshot envelope, so reversing these calls can
    // lose that first event.
    const installedUnlisten = await listen<LiveEnvelope>(LIVE_EVENT_CHANNEL, onEvent);
    if (disposed) {
      // Teardown raced listener installation. The listener did not exist
      // when teardown ran, so remove this late-installed resource now.
      try {
        installedUnlisten();
      } catch (error) {
        setLiveUnavailable(`Live event listener cleanup failed: ${errorMessage(error)}`);
      }
      return teardown;
    }
    unlisten = installedUnlisten;
  } catch (error) {
    listenerFailure = `Live event listener could not be installed: ${errorMessage(error)}`;
  }

  await refreshLivePair('Waiting for the live engine snapshot and topology.');

  if (disposed) return teardown;

  if (listenerFailure) {
    setLiveWatchFailed(listenerFailure);
    return teardown;
  }

  let settleSubscription: (() => void) | undefined;
  const subscriptionFinished = new Promise<void>((resolve) => {
    settleSubscription = resolve;
  });
  subscriptionSettled = subscriptionFinished;
  try {
    const response = await tauriCore.invoke<SubscribeWatchResponse>('subscribe_watch');
    const id = response.subscriptionId ?? response.subscription_id;
    if (!id) throw new Error('subscribe_watch did not return a subscription id');

    // Store the id even when teardown raced the command. The teardown path
    // waits for this promise and then cancels the late subscription.
    subscriptionId = id;
    if (!disposed) {
      setLiveWatchActive();
    }
  } catch (error) {
    if (!disposed) {
      setLiveWatchFailed(`Live watch subscription failed: ${errorMessage(error)}`);
    }
  } finally {
    settleSubscription?.();
    settleSubscription = undefined;
    subscriptionSettled = null;
  }

  return teardown;
}
