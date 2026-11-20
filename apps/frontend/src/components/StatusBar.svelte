<script lang="ts">
  // Compact bottom status area (WO-04 Stage A): the persistent, always
  // visible line under the workspace. It states the data mode, the
  // provenance of what is rendered, and — only when a real imported
  // document provides one — its observation time. No invented timestamps,
  // no simulated realtime, no clock.

  import {
    dataSource,
    snapshotImported,
    topologyImported,
  } from '../lib/dataMode.svelte';
  import { scenarioById } from '../lib/scenarios';
  import { formatObservationTime } from '../lib/import';

  const mode = $derived(dataSource.mode);
  const scenario = $derived(scenarioById(dataSource.mockScenario));

  const modeLabel = $derived(mode === 'mock' ? 'MOCK DATA' : 'IMPORTED OBSERVATION');
  const modeTitle = $derived(
    mode === 'mock'
      ? 'The views show synthetic fixture data — not a live observation of this machine'
      : 'The views show observation documents imported from Araiguma CLI output — a recorded observation, not a live connection',
  );

  // Same discipline as the former App.svelte provenance line (asserted by
  // the e2e suite): mock names the scenario and stays explicit about being
  // synthetic; imported names each file with its own observation time.
  const provenance = $derived.by(() => {
    if (mode === 'mock') {
      return `Scenario “${scenario.name}” — Synthetic demonstration data (labeled fixtures), not an observation of this machine`;
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
