<script lang="ts">
  // Compact bottom status area (WO-04 Stage A): the persistent, always
  // visible line under the workspace. It states the data mode, the
  // provenance of what is rendered, and — only when a real document
  // provides one — its observation time. Live mode distinguishes current,
  // stale, and unavailable states truthfully. No invented timestamps, no
  // simulated realtime, no clock.

  import {
    dataSource,
    liveStatus,
    snapshotImported,
    topologyImported,
  } from '../lib/dataMode.svelte';
  import { scenarioById } from '../lib/scenarios';
  import { formatObservationTime } from '../lib/import';

  const mode = $derived(dataSource.mode);
  const scenario = $derived(scenarioById(dataSource.mockScenario));
  const liveState = $derived(liveStatus());

  const modeLabel = $derived(
    mode === 'mock'
      ? 'MOCK DATA'
      : mode === 'imported'
        ? 'IMPORTED OBSERVATION'
        : liveState === 'live'
          ? 'LIVE'
          : liveState === 'stale'
            ? 'STALE'
            : 'UNAVAILABLE',
  );
  const modeTitle = $derived(
    mode === 'mock'
      ? 'The views show synthetic fixture data — not a live observation of this machine'
      : mode === 'imported'
        ? 'The views show observation documents imported from Araiguma CLI output — a recorded observation, not a live connection'
        : liveState === 'live'
          ? 'The views show a current live observation from the Araiguma engine'
          : liveState === 'stale'
            ? 'The last confirmed live observation remains visible, but a newer engine state is not confirmed'
            : 'No live observation is available from the Araiguma engine',
  );

  // Same discipline as the former App.svelte provenance line (asserted by
  // the e2e suite): mock names the scenario and stays explicit about being
  // synthetic; imported names each file with its own observation time.
  const provenance = $derived.by(() => {
    if (mode === 'mock') {
      return `Scenario “${scenario.name}” — Synthetic demonstration data (labeled fixtures), not an observation of this machine`;
    }
    if (mode === 'live') {
      if (liveState === 'live' && dataSource.live) {
        return `Live engine observation — snapshot observed ${formatObservationTime(dataSource.live.timestampMs)}; topology projected from the same observation.`;
      }
      if (liveState === 'stale') {
        return `Live engine data is stale — ${dataSource.liveMonitorReason ?? dataSource.liveUnavailable ?? 'a refresh is pending'}. The last confirmed live observation remains visible.`;
      }
      return `Live engine data is unavailable — ${dataSource.liveMonitorReason ?? dataSource.liveUnavailable ?? 'no live observation has been received'}. No imported or synthetic document is presented as live.`;
    }
    const parts: string[] = [];
    if (snapshotImported()) {
      parts.push(
        `snapshot “${dataSource.snapshot?.fileName}” observed ${formatObservationTime(dataSource.snapshot!.document.timestampMs)}`,
      );
    } else {
      parts.push('no snapshot document imported (the affected views explain what is missing)');
    }
    if (topologyImported()) {
      parts.push(
        `topology “${dataSource.topology?.fileName}” from an observation of ${formatObservationTime(dataSource.topology!.document.sourceTimestampMs)}`,
      );
    } else {
      parts.push('no topology document imported (the topology view explains what is missing)');
    }
    return `Imported observation documents — ${parts.join(' · ')}. A recorded observation, not a live connection.`;
  });
</script>

<footer class="status-bar" data-testid="status-bar">
  <span
    class="mode"
    class:imported={mode === 'imported'}
    class:live={mode === 'live' && liveState === 'live'}
    class:stale={mode === 'live' && liveState === 'stale'}
    class:unavailable={mode === 'live' && liveState === 'unavailable'}
    data-testid="status-mode"
    role="status"
    title={modeTitle}
  >
    {modeLabel}
  </span>
  <span class="provenance" data-testid="data-provenance" title={provenance}>
    {provenance}
  </span>
</footer>

<style>
  .status-bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 14px;
    height: 34px;
    padding: 0 14px;
    background: var(--bg-panel);
    border-top: 1px solid var(--border-subtle);
  }

  .mode {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 2px 9px;
    border: 1px solid color-mix(in srgb, var(--state-unknown) 65%, transparent);
    border-radius: 999px;
    color: var(--state-unknown);
    background: color-mix(in srgb, var(--state-unknown) 12%, transparent);
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.08em;
    white-space: nowrap;
    flex: none;
  }

  .mode::before {
    content: '';
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--state-unknown);
  }

  .mode.imported {
    border-color: color-mix(in srgb, var(--accent-blue) 65%, transparent);
    color: var(--accent-blue);
    background: color-mix(in srgb, var(--accent-blue) 12%, transparent);
  }

  .mode.imported::before {
    background: var(--accent-blue);
  }

  .mode.live {
    border-color: color-mix(in srgb, var(--state-up) 65%, transparent);
    color: var(--state-up);
    background: color-mix(in srgb, var(--state-up) 12%, transparent);
  }

  .mode.live::before {
    background: var(--state-up);
  }

  .mode.stale {
    border-color: color-mix(in srgb, var(--state-unknown) 65%, transparent);
    color: var(--state-unknown);
    background: color-mix(in srgb, var(--state-unknown) 12%, transparent);
  }

  .mode.stale::before {
    background: var(--state-unknown);
  }

  .mode.unavailable {
    border-color: color-mix(in srgb, var(--state-unavailable) 65%, transparent);
    color: var(--state-unavailable);
    background: color-mix(in srgb, var(--state-unavailable) 12%, transparent);
  }

  .mode.unavailable::before {
    background: var(--state-unavailable);
  }

  .provenance {
    flex: 1;
    min-width: 0;
    color: var(--text-muted);
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
