<script lang="ts">
  import type { Address, Interface, NetworkSnapshot } from '../types';
  import { formatMissing, interfaceStateChip, kindLabel } from '../lib/format';

  let {
    iface,
    snapshot,
    onclose,
  }: {
    iface: Interface;
    snapshot: NetworkSnapshot;
    onclose: () => void;
  } = $props();

  const chip = $derived(interfaceStateChip(iface));
  const addresses = $derived(
    snapshot.addresses
      .filter((a) => a.ifindex === iface.ifindex)
      .sort((a, b) => a.address.localeCompare(b.address)),
  );
  const master = $derived(
    iface.masterIfindex !== null
      ? snapshot.interfaces.find((i) => i.ifindex === iface.masterIfindex)
      : undefined,
  );
  const parent = $derived(
    iface.parentIfindex !== null
      ? snapshot.interfaces.find((i) => i.ifindex === iface.parentIfindex)
      : undefined,
  );
  const routesUsing = $derived(
    snapshot.routes.filter((r) => r.outputInterface === iface.ifindex),
  );
</script>

<aside
  class="panel"
  data-testid="details-panel"
  aria-label="Interface details for {iface.name}"
>
  <header>
    <h2 class="mono">{iface.name}</h2>
    <button class="close" onclick={onclose} aria-label="Close details">✕</button>
  </header>

  <section>
    <h3>Identity</h3>
    <dl>
      <dt>Kind</dt>
      <dd>{kindLabel(iface.kind)}{#if iface.kindDetail} <span class="mono detail">({iface.kindDetail})</span>{/if}</dd>
      <dt>State</dt>
      <dd><span class="chip {chip.tone}">{chip.label}</span></dd>
      <dt>Admin state</dt>
      <dd>{iface.adminState ?? '—'}<span class="hint"> (IFF_UP)</span></dd>
      <dt>Oper state</dt>
      <dd>{iface.operState ?? '—'}</dd>
      <dt>ifindex</dt>
      <dd class="mono">{iface.ifindex}</dd>
      <dt>MAC address</dt>
      <dd class="mono" title={iface.macAddress ? undefined : 'not observed'}>{formatMissing(iface.macAddress)}</dd>
      <dt>MTU</dt>
      <dd class="mono">{iface.mtu ?? '—'}</dd>
      <dt>Master</dt>
      <dd class="mono">{master ? master.name : iface.masterIfindex !== null ? iface.masterIfindex : '—'}</dd>
      <dt>Parent link</dt>
      <dd class="mono">{parent ? parent.name : iface.parentIfindex !== null ? iface.parentIfindex : '—'}</dd>
      <dt>Observed by</dt>
      <dd>{iface.sources.join(', ')}</dd>
    </dl>
  </section>

  <section>
    <h3>Addresses ({addresses.length})</h3>
    {#if addresses.length === 0}
      <p class="empty">no addresses observed</p>
    {:else}
      <table>
        <thead>
          <tr><th>Address</th><th>Scope</th><th>Flags</th></tr>
        </thead>
        <tbody>
          {#each addresses as address (address.address + '/' + address.prefixLen)}
            <tr>
              <td class="mono">{address.address}/{address.prefixLen}</td>
              <td>{address.scope ?? '—'}</td>
              <td class="mono dim">{address.flags ? address.flags.join(', ') : '—'}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>

  <section>
    <h3>Routes using this interface ({routesUsing.length})</h3>
    {#if routesUsing.length === 0}
      <p class="empty">no routes observed via this interface</p>
    {:else}
      <ul>
        {#each routesUsing as route (route.family + route.destination?.address + route.gateway + route.metric)}
          <li class="mono route-line">{route.family} {route.destination ? (route.destination.prefixLen === 0 ? 'default' : route.destination.address + '/' + route.destination.prefixLen) : '—'}</li>
        {/each}
      </ul>
    {/if}
  </section>

  {#if iface.notes.length > 0}
    <section>
      <h3>Observation notes</h3>
      <ul class="notes">
        {#each iface.notes as note (note)}<li>{note}</li>{/each}
      </ul>
    </section>
  {/if}
</aside>

<style>
  .panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(360px, 90vw);
    overflow-y: auto;
    background: var(--bg-panel);
    border-left: 1px solid var(--border-strong);
    box-shadow: var(--shadow-panel);
    padding: 16px 18px 24px;
    z-index: 20;
    /* The panel overlays the workspace; it never shrinks it. */
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 4px;
  }

  h2 {
    margin: 0;
    font-size: 17px;
  }

  h3 {
    margin: 18px 0 8px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--text-secondary);
  }

  .close {
    color: var(--text-muted);
    width: 28px;
    height: 28px;
    border-radius: var(--radius-s);
  }

  .close:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  dl {
    display: grid;
    grid-template-columns: 96px 1fr;
    row-gap: 6px;
    margin: 0;
  }

  dt {
    color: var(--text-muted);
    font-size: 12px;
    padding-top: 1px;
  }

  dd {
    margin: 0;
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  .detail {
    color: var(--text-muted);
    font-size: 11px;
  }

  .hint {
    color: var(--text-muted);
    font-size: 11px;
  }

  .chip {
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 11px;
    border: 1px solid;
  }

  .chip.up {
    color: var(--state-up);
    border-color: color-mix(in srgb, var(--state-up) 55%, transparent);
  }

  .chip.down {
    color: var(--state-down);
    border-color: color-mix(in srgb, var(--state-down) 55%, transparent);
  }

  .chip.unknown {
    color: var(--state-unknown);
    border-color: color-mix(in srgb, var(--state-unknown) 55%, transparent);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
  }

  th {
    text-align: left;
    color: var(--text-muted);
    font-weight: 500;
    font-size: 11px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border-subtle);
  }

  td {
    padding: 5px 6px 5px 0;
    border-bottom: 1px solid color-mix(in srgb, var(--border-subtle) 55%, transparent);
  }

  .dim {
    color: var(--text-secondary);
  }

  ul {
    margin: 0;
    padding-left: 16px;
  }

  .route-line {
    font-size: 12px;
    color: var(--text-secondary);
  }

  .empty {
    color: var(--text-muted);
    font-size: 12.5px;
    margin: 0;
  }

  .notes {
    color: var(--state-unknown);
    font-size: 12px;
  }
</style>
