<script lang="ts">
  // Interfaces section (WO-08 redesign): a scannable interface board. Every
  // card is an observed interface from the snapshot; counts are derived from
  // the same document; selection opens the shared details panel with the
  // full observed signal path.

  import type { NetworkSnapshot } from '../types';
  import { interfaceStateCounts } from '../lib/observability';
  import DataBadge from './DataBadge.svelte';
  import InterfaceCard from './InterfaceCard.svelte';

  let {
    snapshot,
    onselect,
  }: {
    snapshot: NetworkSnapshot;
    onselect: (iface: NetworkSnapshot['interfaces'][number]) => void;
  } = $props();

  const counts = $derived(interfaceStateCounts(snapshot));
  const globalDnsScope = $derived(
    snapshot.dns === undefined ? null : snapshot.dns.filter((o) => o.scope.kind === 'global').length,
  );
  const connectionsKnown = $derived(snapshot.connections !== undefined);
</script>

<div class="interfaces-view" data-testid="interfaces-view">
  <header class="view-head">
    <div>
      <h1>Network interfaces</h1>
      <p class="sub">
        {counts.total} observed {counts.total === 1 ? 'interface' : 'interfaces'} ·
        {snapshot.addresses.length} addresses · {snapshot.routes.length} routes ·
        {counts.up} up · {counts.down} down · {counts.unknown} unknown
      </p>
    </div>
    <DataBadge />
  </header>

  <div class="context-strip">
    <span class="context-chip observed">Interfaces · observed</span>
    {#if snapshot.dns === undefined}
      <span class="context-chip muted">DNS · not observed</span>
    {:else if (snapshot.dns ?? []).length === 0}
      <span class="context-chip">DNS · observed empty</span>
    {:else}
      <span class="context-chip">
        DNS · observed · {globalDnsScope ?? 0} global-scope
        {globalDnsScope === 1 ? 'observation' : 'observations'}
      </span>
    {/if}
    {#if connectionsKnown}
      <span class="context-chip">Connections · observed · {snapshot.connections?.length ?? 0}</span>
    {:else}
      <span class="context-chip muted">Connections · not observed</span>
    {/if}
  </div>

  {#if snapshot.interfaces.length === 0}
    <div class="empty-state" data-testid="empty-state">
      <p>No interfaces observed.</p>
      <p class="sub">
        This is a valid observation of an empty environment — not an error.
        Provider status explains what each source observed.
      </p>
    </div>
  {:else}
    <div class="grid">
      {#each snapshot.interfaces as iface (iface.ifindex)}
        <InterfaceCard {iface} {snapshot} {onselect} />
      {/each}
    </div>
  {/if}
</div>

<style>
  .interfaces-view {
    padding: 22px 26px 32px;
    max-width: 1200px;
  }

  .view-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 12px;
  }

  h1 {
    margin: 0;
    font-size: 19px;
    font-weight: 600;
  }

  .sub {
    margin: 4px 0 0;
    color: var(--text-secondary);
    font-size: 12.5px;
  }

  .context-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 14px;
  }

  .context-chip {
    font-size: 10.5px;
    letter-spacing: 0.03em;
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    padding: 1px 9px;
    color: var(--text-secondary);
    background: color-mix(in srgb, var(--bg-raised) 70%, transparent);
  }

  .context-chip.observed {
    color: var(--state-up);
    border-color: color-mix(in srgb, var(--state-up) 45%, transparent);
  }

  .context-chip.muted {
    color: var(--state-unknown);
    border-color: color-mix(in srgb, var(--state-unknown) 45%, transparent);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 12px;
  }

  .empty-state {
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-m);
    padding: 28px;
    text-align: center;
  }

  .empty-state p {
    margin: 0 0 6px;
  }
</style>
