<script lang="ts">
  import type { NetworkSnapshot } from '../types';
  import MockBadge from './MockBadge.svelte';
  import InterfaceCard from './InterfaceCard.svelte';

  let {
    snapshot,
    onselect,
  }: {
    snapshot: NetworkSnapshot;
    onselect: (iface: NetworkSnapshot['interfaces'][number]) => void;
  } = $props();
</script>

<div class="overview">
  <header class="view-head">
    <div>
      <h1>Network interfaces</h1>
      <p class="sub">
        {snapshot.interfaces.length} observed
        {snapshot.interfaces.length === 1 ? 'interface' : 'interfaces'} ·
        {snapshot.addresses.length} addresses ·
        {snapshot.routes.length} routes
      </p>
    </div>
    <MockBadge />
  </header>

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
        <InterfaceCard
          {iface}
          addresses={snapshot.addresses.filter((a) => a.ifindex === iface.ifindex)}
          {onselect}
        />
      {/each}
    </div>
  {/if}
</div>

<style>
  .overview {
    padding: 22px 26px 32px;
    max-width: 1200px;
  }

  .view-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 18px;
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

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
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
