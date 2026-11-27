<script lang="ts">
  // Connections section (WO-08). The v4 snapshot contract carries
  // NetworkManager connection objects as an OPTIONAL collection: absent
  // means "not observed", [] means "observed empty". This view renders the
  // observed objects and resolves device ifindexes against the CURRENT
  // snapshot only — an ifindex that is not present stays visibly unresolved.

  import type { ConnectionObservation, NetworkSnapshot } from '../types';
  import {
    connectionStateLabel,
    connectionsAvailability,
    deviceLabel,
    formatCount,
    outcomeSummary,
    providerDisplayName,
  } from '../lib/observability';
  import { navigate } from '../lib/navigation.svelte';
  import DataBadge from './DataBadge.svelte';

  let { snapshot }: { snapshot: NetworkSnapshot } = $props();

  const availability = $derived(connectionsAvailability(snapshot));
  const connections = $derived(snapshot.connections ?? []);
  const activatedCount = $derived(
    connections.filter((connection) => connection.state === 'activated').length,
  );
  const vpnCount = $derived(connections.filter((connection) => connection.vpn).length);

  const connectionReports = $derived(
    snapshot.providers.flatMap((report) =>
      report.sections
        .filter((entry) => entry.section === 'connections')
        .map((entry) => ({
          provider: report.provider,
          state: report.state,
          summary: outcomeSummary(entry.outcome),
        })),
    ),
  );

  const sortedConnections = $derived(
    [...connections].sort((a, b) => {
      const stateRank = (connection: ConnectionObservation): number =>
        connection.state === 'activated'
          ? 0
          : connection.state === 'activating'
            ? 1
            : connection.state === 'unknown'
              ? 2
              : connection.state === 'deactivating'
                ? 3
                : connection.state === 'deactivated'
                  ? 4
                  : 5;
      const byState = stateRank(a) - stateRank(b);
      if (byState !== 0) return byState;
      return a.connectionType.localeCompare(b.connectionType);
    }),
  );

  function showInterface(ifindex: number): void {
    navigate({ section: 'interfaces', selection: { kind: 'interface', ifindex } });
  }
</script>

<div class="connections-view" data-testid="connections-view">
  <header class="view-head">
    <div>
      <h1>Connections</h1>
      <p class="sub">
        NetworkManager connection objects observed in the snapshot. These are
        management objects — not socket, flow, or process telemetry.
      </p>
    </div>
    <DataBadge />
  </header>

  <section class="panel provider-strip" aria-label="Connections provider report">
    <div class="strip-head">
      <h2>Provider report</h2>
      <span class="availability {availability.tone}">{availability.label}</span>
    </div>
    <p class="strip-detail">{availability.detail}</p>
    {#if connectionReports.length > 0}
      <ul class="report-list">
        {#each connectionReports as report, i (i)}
          <li>
            <span class="report-provider">{providerDisplayName(report.provider)}</span>
            <span class="report-outcome {report.summary.tone}">{report.summary.label}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="muted">No provider report mentions connections in this document.</p>
    {/if}
  </section>

  {#if availability.state === 'observed-empty'}
    <div class="empty-state" data-testid="connections-empty">
      <p>Observed: no connections.</p>
      <p class="sub">
        A provider looked for connection objects and found none. This is a
        valid observation — not missing data.
      </p>
    </div>
  {:else if availability.state !== 'observed'}
    <div class="empty-state" data-testid="connections-{availability.state}">
      <p>No connection observations are shown.</p>
      <p class="sub">{availability.detail}</p>
      <p class="sub">
        The provider report above is the reason. Nothing is substituted from
        another scenario or document.
      </p>
    </div>
  {:else}
    <div class="summary-line">
      {formatCount(connections.length, 'connection')} ·
      {activatedCount} activated ·
      {formatCount(vpnCount, 'VPN')}
    </div>

    <div class="connection-list">
      {#each sortedConnections as connection, index (connection.connectionType + ':' + connection.devices.join(',') + ':' + index)}
        {@const state = connectionStateLabel(connection.state)}
        <article class="connection-card" data-testid="connection-{index}">
          <header class="card-head">
            <div class="title">
              <span class="mono type">{connection.connectionType}</span>
              {#if connection.vpn}
                <span class="chip vpn">VPN</span>
              {/if}
            </div>
            <span class="chip state {state.tone}">{state.label}</span>
          </header>

          <dl class="facts">
            <dt>Devices</dt>
            <dd>
              {#if connection.devices.length === 0}
                <span class="muted">no devices observed</span>
              {:else}
                <span class="device-list">
                  {#each connection.devices as ifindex, i (i)}
                    <button class="device-link mono" onclick={() => showInterface(ifindex)}>
                      {deviceLabel(snapshot, ifindex)}
                    </button>
                  {/each}
                </span>
              {/if}
            </dd>
            <dt>Observed by</dt>
            <dd>{connection.sources.join(', ')}</dd>
          </dl>

          {#if connection.notes.length > 0}
            <ul class="notes">
              {#each connection.notes as note, i (i)}<li>{note}</li>{/each}
            </ul>
          {/if}
        </article>
      {/each}
    </div>

    <p class="footnote">
      A connection object is not proof that traffic flows through it. Remote
      endpoints, processes, and per-socket state are not observed by this
      document and are not shown.
    </p>
  {/if}
</div>

<style>
  .connections-view {
    padding: 22px 26px 32px;
    max-width: 1000px;
  }

  .view-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 16px;
  }

  h1 {
    margin: 0;
    font-size: 19px;
    font-weight: 600;
  }

  h2 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--text-secondary);
  }

  .sub {
    margin: 4px 0 0;
    color: var(--text-secondary);
    font-size: 12.5px;
    max-width: 680px;
  }

  .panel {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    background: var(--bg-raised);
    padding: 14px 16px;
  }

  .provider-strip {
    margin-bottom: 14px;
  }

  .strip-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .availability {
    font-size: 11.5px;
    border: 1px solid;
    border-radius: 999px;
    padding: 1px 9px;
    white-space: nowrap;
  }

  .availability.ok {
    color: var(--state-up);
    border-color: color-mix(in srgb, var(--state-up) 55%, transparent);
    background: color-mix(in srgb, var(--state-up) 10%, transparent);
  }

  .availability.warn {
    color: var(--state-unknown);
    border-color: color-mix(in srgb, var(--state-unknown) 55%, transparent);
    background: color-mix(in srgb, var(--state-unknown) 10%, transparent);
  }

  .availability.bad {
    color: var(--state-down);
    border-color: color-mix(in srgb, var(--state-down) 55%, transparent);
    background: color-mix(in srgb, var(--state-down) 10%, transparent);
  }

  .availability.muted {
    color: var(--state-unavailable);
    border-color: color-mix(in srgb, var(--state-unavailable) 55%, transparent);
  }

  .strip-detail {
    margin: 6px 0 0;
    color: var(--text-secondary);
    font-size: 12px;
  }

  .report-list {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 12px;
  }

  .report-provider {
    color: var(--text-muted);
    min-width: 190px;
    display: inline-block;
  }

  .report-outcome.ok {
    color: var(--state-up);
  }

  .report-outcome.bad {
    color: var(--state-down);
  }

  .report-outcome.warn {
    color: var(--state-unknown);
  }

  .report-outcome.muted {
    color: var(--text-muted);
  }

  .summary-line {
    color: var(--text-secondary);
    font-size: 12px;
    margin: 0 0 12px;
  }

  .connection-list {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: 10px;
  }

  @media (max-width: 520px) {
    .connection-list {
      grid-template-columns: 1fr;
    }
  }

  .connection-card {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    background: var(--bg-raised);
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .card-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
  }

  .title {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .type {
    font-size: 13px;
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .chip {
    font-size: 11px;
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    padding: 1px 8px;
    white-space: nowrap;
  }

  .chip.vpn {
    color: var(--accent-blue);
    border-color: color-mix(in srgb, var(--accent-blue) 45%, transparent);
  }

  .chip.state.ok {
    color: var(--state-up);
    border-color: color-mix(in srgb, var(--state-up) 55%, transparent);
    background: color-mix(in srgb, var(--state-up) 10%, transparent);
  }

  .chip.state.warn {
    color: var(--state-unknown);
    border-color: color-mix(in srgb, var(--state-unknown) 55%, transparent);
    background: color-mix(in srgb, var(--state-unknown) 10%, transparent);
  }

  .chip.state.bad {
    color: var(--state-down);
    border-color: color-mix(in srgb, var(--state-down) 55%, transparent);
  }

  .chip.state.muted {
    color: var(--text-muted);
  }

  .facts {
    display: grid;
    grid-template-columns: 82px 1fr;
    gap: 5px 10px;
    margin: 0;
    font-size: 12.5px;
  }

  dt {
    color: var(--text-muted);
    font-size: 11.5px;
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .device-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .device-link {
    color: var(--accent);
    font-size: 12px;
  }

  .device-link:hover {
    text-decoration: underline;
  }

  .muted {
    color: var(--text-muted);
    font-size: 12px;
  }

  .notes {
    margin: 0;
    padding-left: 16px;
    color: var(--state-unknown);
    font-size: 12px;
  }

  .empty-state {
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-m);
    padding: 24px;
    text-align: center;
  }

  .empty-state p {
    margin: 0 0 6px;
  }

  .footnote {
    margin: 14px 0 0;
    color: var(--text-muted);
    font-size: 11.5px;
    max-width: 760px;
  }
</style>
