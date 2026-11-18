<script lang="ts">
  import type { Address, Interface } from '../types';
  import { interfaceStateChip, kindLabel, summarizeAddresses } from '../lib/format';

  let {
    iface,
    addresses,
    onselect,
  }: {
    iface: Interface;
    addresses: Address[];
    onselect: (iface: Interface) => void;
  } = $props();

  const chip = $derived(interfaceStateChip(iface));
  const kind = $derived(kindLabel(iface.kind));
  const summary = $derived(summarizeAddresses(addresses));
</script>

<button
  class="card"
  data-testid="iface-card-{iface.name}"
  onclick={() => onselect(iface)}
>
  <span class="card-head">
    <span class="name mono">{iface.name}</span>
    <span class="chip {chip.tone}">{chip.label}</span>
  </span>
  <span class="kind">
    {kind}{#if iface.kindDetail}<span class="kind-detail mono"> ({iface.kindDetail})</span>{/if}
  </span>
  <span class="addrs">
    {#if summary.shown.length === 0}
      <span class="no-addr">no addresses observed</span>
    {:else}
      {#each summary.shown as address (address.address + '/' + address.prefixLen)}
        <span class="addr mono" class:dim={address.scope !== 'global'}>
          {address.address}/{address.prefixLen}
        </span>
      {/each}
      {#if summary.more > 0}<span class="more">+{summary.more} more</span>{/if}
    {/if}
  </span>
  <span class="sources">
    {#each iface.sources as source (source)}<span class="source">{source}</span>{/each}
  </span>
</button>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    text-align: left;
    padding: 14px 16px;
    background: var(--bg-raised);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    transition: border-color 120ms ease, background 120ms ease;
  }

  .card:hover {
    background: var(--bg-hover);
    border-color: var(--border-strong);
  }

  .card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .name {
    font-size: 15px;
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .chip {
    flex: none;
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 11px;
    border: 1px solid;
  }

  .chip.up {
    color: var(--state-up);
    border-color: color-mix(in srgb, var(--state-up) 55%, transparent);
    background: color-mix(in srgb, var(--state-up) 10%, transparent);
  }

  .chip.down {
    color: var(--state-down);
    border-color: color-mix(in srgb, var(--state-down) 55%, transparent);
    background: color-mix(in srgb, var(--state-down) 10%, transparent);
  }

  .chip.unknown {
    color: var(--state-unknown);
    border-color: color-mix(in srgb, var(--state-unknown) 55%, transparent);
    background: color-mix(in srgb, var(--state-unknown) 10%, transparent);
  }

  .kind {
    color: var(--text-secondary);
    font-size: 12.5px;
  }

  .kind-detail {
    color: var(--text-muted);
    font-size: 11px;
  }

  .addrs {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 12px;
    min-height: 18px;
  }

  .addr {
    color: var(--text-primary);
  }

  .addr.dim {
    color: var(--text-secondary);
  }

  .no-addr,
  .more {
    color: var(--text-muted);
    font-size: 12px;
  }

  .sources {
    display: flex;
    gap: 6px;
    margin-top: 2px;
  }

  .source {
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    padding: 0 5px;
  }
</style>
