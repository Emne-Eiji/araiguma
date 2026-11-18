<script lang="ts">
  import type { NetworkSnapshot, Route } from '../types';
  import { formatGateway, formatIpNet, formatProtocol } from '../lib/format';
  import { routeKey, tableOptions, visibleRoutes, MAIN_TABLE } from '../lib/routes';
  import MockBadge from './MockBadge.svelte';

  let {
    snapshot,
    selectedRoute,
    onselect,
  }: {
    snapshot: NetworkSnapshot;
    selectedRoute: Route | null;
    onselect: (route: Route | null) => void;
  } = $props();

  // The default presentation shows the main table only; other tables are
  // opt-in through the visible selector below. Nothing is ever discarded.
  let selectedTables = $state<number[]>([MAIN_TABLE]);
  const tables = $derived(tableOptions(snapshot));
  const routes = $derived(visibleRoutes(snapshot, selectedTables));
  const otherTableCount = $derived(
    tables.filter((t) => t.table !== MAIN_TABLE).reduce((sum, t) => sum + t.count, 0),
  );

  function toggleTable(table: number) {
    selectedTables = selectedTables.includes(table)
      ? selectedTables.filter((t) => t !== table)
      : [...selectedTables, table].sort((a, b) => a - b);
  }

  function scopeLabel(route: Route): string {
    if (route.scope === null) return '—';
    return typeof route.scope === 'string' ? route.scope : `scope-${route.scope.other}`;
  }

  function typeLabel(route: Route): string {
    if (route.routeType === null) return '—';
    return typeof route.routeType === 'string' ? route.routeType : `type-${route.routeType.other}`;
  }
</script>

<div class="routes-view">
  <header class="view-head">
    <div>
      <h1>Routes</h1>
      <p class="sub">
        {routes.length} of {snapshot.routes.length} observed routes shown
        {#if otherTableCount > 0 && !selectedTables.some((t) => t !== MAIN_TABLE)}
          · {otherTableCount} route{otherTableCount === 1 ? '' : 's'} in other tables not shown
        {/if}
      </p>
    </div>
    <MockBadge />
  </header>

  <div class="filter-row" role="group" aria-label="Routing table filter">
    <span class="filter-label">Tables:</span>
    {#each tables as option (option.table)}
      <button
        class="table-chip"
        class:active={selectedTables.includes(option.table)}
        aria-pressed={selectedTables.includes(option.table)}
        onclick={() => toggleTable(option.table)}
      >
        {option.label}
        <span class="count">{option.count}</span>
      </button>
    {/each}
    <span class="filter-note">
      the default view shows the main table (254) only — other tables stay
      available, unmerged and clearly labeled
    </span>
  </div>

  {#if routes.length === 0}
    <div class="empty-state" data-testid="routes-empty">
      <p>No routes in the selected tables.</p>
      <p class="sub">
        Zero routes is a valid observation; the provider status explains what
        each source could see.
      </p>
    </div>
  {:else}
    <div class="table-wrap">
      <table data-testid="routes-table">
        <thead>
          <tr>
            <th>Destination</th>
            <th>Gateway</th>
            <th>Interface</th>
            <th>Family</th>
            <th>Protocol</th>
            <th>Scope</th>
            <th>Type</th>
            <th>Metric</th>
            <th>Table</th>
          </tr>
        </thead>
        <tbody>
          {#each routes as route, index (routeKey(route, index))}
            <tr
              class:selected={selectedRoute === route}
              class:default-route={route.destination !== null && route.destination.prefixLen === 0}
              onclick={() => onselect(selectedRoute === route ? null : route)}
            >
              <td class="mono dest">
                {#if route.destination === null}
                  <span title="destination not observed">—</span>
                {:else if route.destination.prefixLen === 0}
                  <span class="default-label">default</span>
                {:else}
                  {route.destination.address}/{route.destination.prefixLen}
                {/if}
              </td>
              <td class="mono" title={route.gateway === null ? 'the kernel reported no gateway for this route (on-link)' : undefined}>
                {formatGateway(route.gateway)}
              </td>
              <td
                class="mono oif"
                title={route.outputInterfaceName ?? (route.outputInterface !== null ? `ifindex ${route.outputInterface}` : 'output interface not observed')}
              >
                {route.outputInterfaceName ?? (route.outputInterface !== null ? `#${route.outputInterface}` : '—')}
              </td>
              <td><span class="family {route.family}">{route.family}</span></td>
              <td title={route.protocol === null ? 'not observed' : undefined}>
                {formatProtocol(route.protocol)}
              </td>
              <td title={route.scope === null ? 'not observed' : undefined}>{scopeLabel(route)}</td>
              <td title={route.routeType === null ? 'not observed' : undefined}>{typeLabel(route)}</td>
              <td class="mono" title={route.metric === null ? 'not observed' : undefined}>
                {route.metric ?? '—'}
              </td>
              <td class="mono">{route.table ?? '—'}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    {#if selectedRoute}
      <p class="selection-note" data-testid="route-selection-note">
        Route selected — switch to <strong>Topology</strong> to see its
        interface and gateway highlighted (highlight only; nothing is
        modified).
      </p>
    {/if}
  {/if}
</div>

<style>
  .routes-view {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 22px 26px 26px;
  }

  .view-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
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

  .filter-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin: 14px 0;
  }

  .filter-label {
    color: var(--text-muted);
    font-size: 12px;
  }

  .table-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px;
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    color: var(--text-secondary);
    font-size: 12px;
  }

  .table-chip:hover {
    border-color: var(--border-strong);
  }

  .table-chip.active {
    border-color: var(--accent);
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }

  .count {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-muted);
  }

  .filter-note {
    color: var(--text-muted);
    font-size: 11px;
    margin-left: 4px;
  }

  .table-wrap {
    flex: 1;
    min-height: 0;
    overflow: auto;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
  }

  th {
    position: sticky;
    top: 0;
    background: var(--bg-panel);
    text-align: left;
    color: var(--text-muted);
    font-weight: 500;
    font-size: 11px;
    letter-spacing: 0.03em;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border-subtle);
    white-space: nowrap;
  }

  td {
    padding: 7px 10px;
    border-bottom: 1px solid color-mix(in srgb, var(--border-subtle) 55%, transparent);
    white-space: nowrap;
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  tbody tr {
    cursor: pointer;
  }

  tbody tr:hover {
    background: var(--bg-hover);
  }

  tbody tr.selected {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }

  tr.default-route .dest .default-label,
  .default-label {
    color: var(--accent);
    font-weight: 600;
  }

  tr.default-route {
    background: color-mix(in srgb, var(--accent) 4%, transparent);
  }

  .family {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 4px;
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
  }

  .family.ipv6 {
    color: var(--accent-blue);
    border-color: color-mix(in srgb, var(--accent-blue) 45%, transparent);
  }

  .selection-note {
    margin: 10px 0 0;
    color: var(--text-secondary);
    font-size: 12px;
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
