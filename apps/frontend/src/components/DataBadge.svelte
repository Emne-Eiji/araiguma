<script lang="ts">
  // Persistent, explicit marker of what the current view renders: the
  // labeled synthetic fixtures (MOCK DATA) or imported observation
  // documents (IMPORTED OBSERVATION — never presented as a live,
  // continuously updating connection). One of the two is always visible.

  import { dataSource } from '../lib/dataMode.svelte';

  let mode = $derived(dataSource.mode);
  let title = $derived(
    mode === 'mock'
      ? 'This view shows synthetic fixture data — not a live observation of this machine'
      : 'This view shows observation documents imported from Araiguma CLI output — a recorded observation, not a live connection',
  );
</script>

<span
  class="badge"
  class:imported={mode === 'imported'}
  data-testid={mode === 'mock' ? 'mock-badge' : 'imported-badge'}
  role="status"
  title={title}
>
  {mode === 'mock' ? 'MOCK DATA' : 'IMPORTED OBSERVATION'}
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
</style>
