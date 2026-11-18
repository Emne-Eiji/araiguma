<script lang="ts">
  // Contextual details for a selected route. Overlays the workspace (same
  // discipline as the overview and topology details panels) so the main
  // table is never permanently narrowed. This is where route flags,
  // observation notes (including representation limitations such as the
  // current multipath first-hop-only form), and provenance are fully
  // visible (WO-02 correction §4) — the table stays scannable.

  import type { Route } from '../types';
  import { formatGateway, formatIpNet, formatProtocol } from '../lib/format';
  import { routeHasLimitationNotes } from '../lib/routes';

  let {
    route,
    onclose,
  }: {
    route: Route;
    onclose: () => void;
  } = $props();

  function scopeLabel(): string {
    if (route.scope === null) return 'not observed';
    return typeof route.scope === 'string' ? route.scope : `scope-${route.scope.other}`;
  }

  function typeLabel(): string {
    if (route.routeType === null) return 'not observed';
    return typeof route.routeType === 'string'
      ? route.routeType
      : `type-${route.routeType.other}`;
  }
</script>

<aside class="panel" data-testid="route-details" aria-label="Route details">
  <header>
    <h2 class="mono">
      {formatIpNet(route.destination)} · {route.family}
    </h2>
    <button class="close" onclick={onclose} aria-label="Close route details">✕</button>
  </header>

  {#if routeHasLimitationNotes(route)}
    <p class="warning" data-testid="route-limitation-warning" role="note">
      This observation carries limitations — read the notes below before
      relying on it.
    </p>
  {/if}

  <section>
    <h3>Route</h3>
    <dl>
      <dt>Destination</dt>
      <dd class="mono">{formatIpNet(route.destination)}</dd>
      <dt>Gateway</dt>
      <dd class="mono" title={route.gateway === null
        ? 'the kernel reported no gateway for this route (on-link) — an observed absence'
        : undefined}
      >
        {formatGateway(route.gateway)}
      </dd>
      <dt>Output interface</dt>
      <dd class="mono">
        {route.outputInterfaceName
          ?? (route.outputInterface !== null ? `ifindex ${route.outputInterface}` : '—')}
        <span class="dim" title={route.outputInterface === null && route.outputInterfaceName === null
          ? 'output interface not observed'
          : undefined}
        >
          {#if route.outputInterface !== null && route.outputInterfaceName !== null}
            (#{route.outputInterface})
          {/if}
        </span>
      </dd>
      <dt>Protocol</dt>
      <dd>{formatProtocol(route.protocol)}</dd>
      <dt>Scope</dt>
      <dd>{scopeLabel()}</dd>
      <dt>Type</dt>
      <dd>{typeLabel()}</dd>
      <dt>Metric</dt>
      <dd class="mono" title={route.metric === null ? 'not observed' : undefined}>
        {route.metric ?? '—'}
      </dd>
      <dt>Table</dt>
      <dd class="mono">{route.table ?? '—'}</dd>
    </dl>
  </section>

  <section data-testid="route-flags">
    <h3>Flags</h3>
    {#if route.flags === null}
      <p class="empty">flags not observed</p>
    {:else if route.flags.length === 0}
      <p class="empty">observed — none set</p>
    {:else}
      <ul class="flags">
        {#each route.flags as flag (flag)}<li class="mono">{flag}</li>{/each}
      </ul>
    {/if}
  </section>

  <section data-testid="route-notes">
    <h3>Observation notes</h3>
    {#if route.notes.length === 0}
      <p class="empty">no notes recorded</p>
    {:else}
      <ul class="notes">
        {#each route.notes as note (note)}<li>{note}</li>{/each}
      </ul>
    {/if}
  </section>

  <section>
    <h3>Observed by</h3>
    <p class="sources" data-testid="route-sources">
      {#each route.sources as source, index (source)}{#if index > 0}, {/if}{source}{/each}
    </p>
  </section>
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
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 4px;
  }

  h2 {
    margin: 0;
    font-size: 16px;
    overflow-wrap: anywhere;
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
    flex: none;
  }

  .close:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .warning {
    margin: 10px 0 0;
    padding: 8px 10px;
    border: 1px solid color-mix(in srgb, var(--state-unknown) 55%, transparent);
    border-radius: var(--radius-s);
    background: color-mix(in srgb, var(--state-unknown) 10%, transparent);
    color: var(--state-unknown);
    font-size: 12px;
  }

  dl {
    display: grid;
    grid-template-columns: 110px 1fr;
    row-gap: 6px;
    margin: 0;
  }

  dt {
    color: var(--text-muted);
    font-size: 12px;
  }

  dd {
    margin: 0;
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  .dim {
    color: var(--text-muted);
    font-size: 11px;
  }

  .empty {
    color: var(--text-muted);
    font-size: 12.5px;
    margin: 0;
  }

  .flags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .flags li {
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    padding: 1px 6px;
    font-size: 11px;
    color: var(--text-secondary);
  }

  .notes {
    color: var(--state-unknown);
    font-size: 12px;
    padding-left: 16px;
    margin: 0;
  }

  .notes li {
    margin-bottom: 4px;
    overflow-wrap: anywhere;
  }

  .sources {
    margin: 0;
    font-size: 13px;
  }
</style>
