<script lang="ts">
  // Persistent, explicit marker of what the current view renders: the
  // labeled synthetic fixtures (MOCK DATA), imported observation documents
  // (IMPORTED OBSERVATION — never presented as a live connection), or the
  // live engine pair (LIVE / STALE / UNAVAILABLE). One truthful state is
  // always visible.

  import { dataSource, liveStatus } from '../lib/dataMode.svelte';

  let mode = $derived(dataSource.mode);
  let liveState = $derived(liveStatus());
  let label = $derived(
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
  let title = $derived(
    mode === 'mock'
      ? 'This view shows synthetic fixture data — not a live observation of this machine'
      : mode === 'imported'
        ? 'This view shows observation documents imported from Araiguma CLI output — a recorded observation, not a live connection'
        : liveState === 'live'
          ? 'This view shows a current live observation from the Araiguma engine'
          : liveState === 'stale'
            ? 'This view shows the last confirmed live observation, but a newer engine state is not confirmed'
            : 'No live observation is available from the Araiguma engine',
  );
</script>

<span
  class="badge"
  class:imported={mode === 'imported'}
  class:live={mode === 'live' && liveState === 'live'}
  class:stale={mode === 'live' && liveState === 'stale'}
  class:unavailable={mode === 'live' && liveState === 'unavailable'}
  data-testid={mode === 'mock' ? 'mock-badge' : mode === 'imported' ? 'imported-badge' : 'live-badge'}
  role="status"
  title={title}
>
  {label}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px;
    border: 1px solid color-mix(in srgb, var(--state-unknown) 65%, transparent);
    border-radius: 999px;
    color: var(--state-unknown);
    background: color-mix(in srgb, var(--state-unknown) 12%, transparent);
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 0.08em;
    white-space: nowrap;
  }

  .badge::before {
    content: '';
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--state-unknown);
  }

  .badge.imported {
    border-color: color-mix(in srgb, var(--accent-blue) 65%, transparent);
    color: var(--accent-blue);
    background: color-mix(in srgb, var(--accent-blue) 12%, transparent);
  }

  .badge.imported::before {
    background: var(--accent-blue);
  }

  .badge.live {
    border-color: color-mix(in srgb, var(--state-up) 65%, transparent);
    color: var(--state-up);
    background: color-mix(in srgb, var(--state-up) 12%, transparent);
  }

  .badge.live::before {
    background: var(--state-up);
  }

  .badge.stale {
    border-color: color-mix(in srgb, var(--state-unknown) 65%, transparent);
    color: var(--state-unknown);
    background: color-mix(in srgb, var(--state-unknown) 12%, transparent);
  }

  .badge.stale::before {
    background: var(--state-unknown);
  }

  .badge.unavailable {
    border-color: color-mix(in srgb, var(--state-unavailable) 65%, transparent);
    color: var(--state-unavailable);
    background: color-mix(in srgb, var(--state-unavailable) 12%, transparent);
  }

  .badge.unavailable::before {
    background: var(--state-unavailable);
  }
</style>
