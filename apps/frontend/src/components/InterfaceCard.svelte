<script lang="ts">
  // Interface card (WO-08 redesign): an at-a-glance evidence summary for one
  // observed interface. Every count is derived from the current snapshot;
  // an optional collection that was never observed is labeled as such, not
  // shown as zero. Selection opens the shared details panel.

  import type { Interface, NetworkSnapshot } from '../types';
  import { interfaceStateChip, kindLabel, summarizeAddresses } from '../lib/format';
  import {
    connectionsForInterface,
    dnsForInterface,
    gatewaysForInterface,
    routesForInterface,
  } from '../lib/observability';

  let {
    iface,
    snapshot,
    onselect,
  }: {
    iface: Interface;
    snapshot: NetworkSnapshot;
    onselect: (iface: Interface) => void;
  } = $props();

  const chip = $derived(interfaceStateChip(iface));
  const kind = $derived(kindLabel(iface.kind));
  const addresses = $derived(
    snapshot.addresses
      .filter((address) => address.ifindex === iface.ifindex)
      .sort((a, b) => a.address.localeCompare(b.address)),
  );
  const summary = $derived(summarizeAddresses(addresses));
  const routes = $derived(routesForInterface(snapshot, iface.ifindex));
  const gateways = $derived(gatewaysForInterface(snapshot, iface.ifindex));
  const linkDns = $derived(dnsForInterface(snapshot, iface.ifindex));
  const connections = $derived(connectionsForInterface(snapshot, iface.ifindex));

  const master = $derived(
    iface.masterIfindex !== null
      ? snapshot.interfaces.find((candidate) => candidate.ifindex === iface.masterIfindex)
      : undefined,
  );
  const parent = $derived(
    iface.parentIfindex !== null
      ? snapshot.interfaces.find((candidate) => candidate.ifindex === iface.parentIfindex)
      : undefined,
  );
  const peer = $derived(
    iface.peerIfindex !== null
      ? snapshot.interfaces.find((candidate) => candidate.ifindex === iface.peerIfindex)
      : undefined,
  );

  const dnsValue = $derived(
    snapshot.dns === undefined ? 'not observed' : String(linkDns.length),
  );
  const connectionValue = $derived(
    snapshot.connections === undefined ? 'not observed' : String(connections.length),
  );
</script>

<button
  class="card"
  data-testid="iface-card-{iface.name}"
  onclick={() => onselect(iface)}
  aria-label="Inspect {iface.name}, {kind}, state {chip.label}"
>
  <span class="card-head">
    <span class="name-line">
      <span class="state-dot {chip.tone}" aria-hidden="true"></span>
      <span class="name mono">{iface.name}</span>
    </span>
    <span class="chip {chip.tone}">{chip.label}</span>
  </span>

  <span class="kind-line">
    <span class="kind">{kind}</span>
    {#if iface.kindDetail}<span class="kind-detail mono">({iface.kindDetail})</span>{/if}
    <span class="ifindex mono">ifindex {iface.ifindex}</span>
  </span>

  <span class="addrs">
    {#if summary.shown.length === 0}
      <span class="no-addr">no addresses observed</span>
    {:else}
      {#each summary.shown as address, i (i)}
        <span class="addr mono" class:dim={address.scope !== 'global'}>
          {address.address}/{address.prefixLen}
        </span>
      {/each}
      {#if summary.more > 0}<span class="more">+{summary.more} more</span>{/if}
    {/if}
  </span>

  <span class="facts">
    <span class="fact">
      <span class="fact-value mono">{routes.length}</span>
      <span class="fact-label">routes</span>
    </span>
    <span class="fact">
      <span class="fact-value mono">{gateways.length}</span>
      <span class="fact-label">gateways</span>
    </span>
    <span class="fact">
      <span class="fact-value mono">{dnsValue}</span>
      <span class="fact-label">link DNS</span>
    </span>
    <span class="fact">
      <span class="fact-value mono">{connectionValue}</span>
      <span class="fact-label">connections</span>
    </span>
  </span>

  {#if master || parent || peer || iface.masterIfindex !== null || iface.parentIfindex !== null || iface.peerIfindex !== null}
    <span class="relations">
      {#if iface.masterIfindex !== null}
        <span class="relation">master <span class="mono">{master ? master.name : `#${iface.masterIfindex}`}</span></span>
      {/if}
      {#if iface.parentIfindex !== null}
        <span class="relation">parent <span class="mono">{parent ? parent.name : `#${iface.parentIfindex}`}</span></span>
      {/if}
      {#if iface.peerIfindex !== null}
        <span class="relation">peer <span class="mono">{peer ? peer.name : `#${iface.peerIfindex} (not observed)`}</span></span>
      {/if}
    </span>
  {/if}

  <span class="sources">
    {#each iface.sources as source, i (i)}<span class="source">{source}</span>{/each}
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

  .name-line {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .state-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
  }

  .state-dot.up {
    background: var(--state-up);
  }

  .state-dot.down {
    background: var(--state-down);
  }

  .state-dot.unknown {
    background: var(--state-unknown);
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

  .kind-line {
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex-wrap: wrap;
    color: var(--text-secondary);
    font-size: 12.5px;
  }

  .kind-detail {
    color: var(--text-muted);
    font-size: 11px;
  }

  .ifindex {
    margin-left: auto;
    color: var(--text-muted);
    font-size: 10.5px;
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

  .facts {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 6px 12px;
    padding: 7px 9px;
    border-radius: var(--radius-s);
    background: color-mix(in srgb, var(--bg-inset) 65%, transparent);
    border: 1px solid color-mix(in srgb, var(--border-subtle) 60%, transparent);
  }

  .fact {
    display: flex;
    align-items: baseline;
    gap: 5px;
    min-width: 0;
  }

  .fact-value {
    color: var(--text-primary);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .fact-label {
    color: var(--text-muted);
    font-size: 9.5px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .relations {
    display: flex;
    flex-wrap: wrap;
    gap: 5px 10px;
    color: var(--text-muted);
    font-size: 11px;
  }

  .relation .mono {
    color: var(--text-secondary);
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
