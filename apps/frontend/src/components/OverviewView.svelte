<script lang="ts">
  // Overview section (WO-08 redesign): a state board, not a document index.
  // Every number is derived from the current documents (imported, live, or
  // the labeled synthetic scenario); nothing is invented, and missing data
  // keeps its exact meaning — not observed, unsupported, unavailable, or a
  // valid observed-empty collection. The evidence key is the shared
  // vocabulary used by every view.

  import {
    dataSource,
    currentSnapshot,
    currentTopology,
    liveStatus,
    snapshotImported,
    topologyImported,
  } from '../lib/dataMode.svelte';
  import { scenarioById } from '../lib/scenarios';
  import { formatObservationTime } from '../lib/import';
  import { navigateTo, SECTIONS, SECTION_META } from '../lib/navigation.svelte';
  import type { SectionId } from '../lib/navigation.svelte';
  import type { ProviderReport } from '../types';
  import { interfaceStateChip } from '../lib/format';
  import {
    availabilityEvidenceLabel,
    connectionsAvailability,
    dnsAvailability,
    formatCount,
    gatewayObservations,
    defaultRoutes,
    interfaceStateCounts,
    problemSections,
    providerDisplayName,
    providerOwnerLabel,
    providerStateCounts,
    providerStateSummary,
    routeGateways,
    sectionLabel,
  } from '../lib/observability';
  import DataBadge from './DataBadge.svelte';

  const scenario = $derived(scenarioById(dataSource.mockScenario));
  const liveState = $derived(liveStatus());

  const snapshot = $derived(currentSnapshot());
  const topologyDocument = $derived(currentTopology());

  const ifaceCounts = $derived(
    snapshot
      ? interfaceStateCounts(snapshot)
      : { total: 0, up: 0, down: 0, unknown: 0 },
  );
  const defaults = $derived(snapshot ? defaultRoutes(snapshot) : []);
  const gateways = $derived(snapshot ? gatewayObservations(snapshot) : []);
  const onLinkDefaults = $derived(
    defaults.filter((route) => routeGateways(route).length === 0).length,
  );

  const dns = $derived(snapshot ? dnsAvailability(snapshot) : null);
  const dnsObservations = $derived(snapshot?.dns ?? []);
  const dnsServerCount = $derived(
    dnsObservations.reduce((total, observation) => total + observation.servers.length, 0),
  );
  const dnsLinkScopeCount = $derived(
    dnsObservations.filter((observation) => observation.scope.kind === 'link').length,
  );

  const connections = $derived(snapshot ? connectionsAvailability(snapshot) : null);
  const connectionObservations = $derived(snapshot?.connections ?? []);
  const activeConnections = $derived(
    connectionObservations.filter((connection) => connection.state === 'activated').length,
  );
  const vpnConnections = $derived(
    connectionObservations.filter((connection) => connection.vpn).length,
  );

  const providerCounts = $derived(
    snapshot ? providerStateCounts(snapshot.providers) : [],
  );
  const problems = $derived(snapshot ? problemSections(snapshot.providers) : []);

  interface AttentionItem {
    id: string;
    tone: 'bad' | 'warn' | 'info';
    title: string;
    detail: string;
    section: SectionId;
  }

  const attention = $derived.by(() => {
    const items: AttentionItem[] = [];
    if (!snapshot) {
      items.push({
        id: 'missing-snapshot',
        tone: 'warn',
        title: 'No snapshot document',
        detail:
          'The snapshot-driven views state this explicitly; no other document is substituted.',
        section: 'documents',
      });
    } else {
      const down = snapshot.interfaces.filter(
        (iface) => interfaceStateChip(iface).tone === 'down',
      );
      if (down.length > 0) {
        items.push({
          id: 'interfaces-down',
          tone: 'bad',
          title: `${formatCount(down.length, 'interface')} down`,
          detail: down.map((iface) => iface.name).join(', '),
          section: 'interfaces',
        });
      }
      const unknown = snapshot.interfaces.filter(
        (iface) => interfaceStateChip(iface).tone === 'unknown',
      );
      if (unknown.length > 0) {
        items.push({
          id: 'interfaces-unknown',
          tone: 'warn',
          title: `${formatCount(unknown.length, 'interface')} with unknown state`,
          detail: `${unknown.map((iface) => iface.name).join(', ')} — the observed oper state is not up or down.`,
          section: 'interfaces',
        });
      }
      if (defaults.length > 0 && gateways.length === 0) {
        items.push({
          id: 'defaults-on-link',
          tone: 'info',
          title: 'Default routes are on-link',
          detail:
            'No observed route names a next-hop gateway; anything beyond the local link is not observed.',
          section: 'routes',
        });
      }
      if (
        dns &&
        dns.state !== 'observed' &&
        dns.state !== 'observed-empty'
      ) {
        items.push({
          id: 'dns-availability',
          tone: dns.tone === 'bad' ? 'bad' : dns.tone === 'warn' ? 'warn' : 'info',
          title: `DNS configuration: ${dns.label}`,
          detail: dns.detail,
          section: 'dns',
        });
      }
      if (
        connections &&
        connections.state !== 'observed' &&
        connections.state !== 'observed-empty'
      ) {
        items.push({
          id: 'connections-availability',
          tone:
            connections.tone === 'bad'
              ? 'bad'
              : connections.tone === 'warn'
                ? 'warn'
                : 'info',
          title: `Connections: ${connections.label}`,
          detail: connections.detail,
          section: 'connections',
        });
      }
    }

    for (const problem of problems) {
      items.push({
        id: `problem-${problem.provider}-${problem.section}`,
        tone: problem.summary.tone === 'bad' ? 'bad' : 'warn',
        title: `${providerDisplayName(problem.provider)} · ${sectionLabel(problem.section)}`,
        detail: problem.summary.label,
        section: 'providers',
      });
    }

    if (!topologyDocument) {
      items.push({
        id: 'missing-topology',
        tone: 'warn',
        title: 'No topology document',
        detail:
          'The topology view states this explicitly; no relationships are inferred from another document.',
        section: 'documents',
      });
    }
    return items;
  });

  function providerCoverage(report: ProviderReport): string {
    const observed = report.sections.filter(
      (section) => section.outcome.kind === 'observed',
    ).length;
    const unsupported = report.sections.filter(
      (section) => section.outcome.kind === 'unsupported',
    ).length;
    const trouble = report.sections.filter(
      (section) =>
        section.outcome.kind === 'failed' ||
        section.outcome.kind === 'unavailable' ||
        section.outcome.kind === 'permissionDenied',
    ).length;
    const parts = [`${observed} observed`, `${unsupported} n/a`];
    if (trouble > 0) parts.push(`${trouble} problem${trouble === 1 ? '' : 's'}`);
    return parts.join(' · ');
  }

  // Section links exclude the overview itself (the user is already here);
  // the list exercises the same stable destinations the rail and a future
  // tray use.
  const otherSections = $derived(SECTIONS.filter((s) => s !== 'overview') as SectionId[]);

  function open(section: SectionId) {
    navigateTo(section);
  }
</script>

<div class="overview-view" data-testid="overview-view">
  <header class="view-head">
    <div>
      <h1>Network state</h1>
      <p class="sub">
        A read-only observation board. Every value below is derived from the
        current documents; missing data keeps its exact meaning.
      </p>
    </div>
    <DataBadge />
  </header>

  <div class="stat-grid">
    <button
      class="stat"
      data-testid="overview-stat-interfaces"
      onclick={() => open('interfaces')}
    >
      <span class="stat-top">
        <span class="stat-label">Interfaces</span>
        {#if snapshot}
          <span class="evidence observed">
            {ifaceCounts.total === 0 ? 'OBSERVED EMPTY' : 'OBSERVED'}
          </span>
        {:else}
          <span class="evidence not-observed">NOT OBSERVED</span>
        {/if}
      </span>
      <span class="stat-value">
        {#if snapshot}
          {ifaceCounts.up}<span class="stat-unit"> / {ifaceCounts.total} up</span>
        {:else}
          —<span class="stat-unit"> no snapshot</span>
        {/if}
      </span>
      <span class="stat-detail">
        {#if !snapshot}
          no interfaces observed — no snapshot document
        {:else if ifaceCounts.total === 0}
          no interfaces observed
        {:else}
          {formatCount(ifaceCounts.down, 'down', 'down')} ·
          {formatCount(ifaceCounts.unknown, 'unknown', 'unknown')}
        {/if}
      </span>
    </button>

    <button class="stat" data-testid="overview-stat-routes" onclick={() => open('routes')}>
      <span class="stat-top">
        <span class="stat-label">Default routes</span>
        {#if snapshot}
          <span class="evidence observed">OBSERVED</span>
        {:else}
          <span class="evidence not-observed">NOT OBSERVED</span>
        {/if}
      </span>
      <span class="stat-value">
        {#if snapshot}
          {defaults.length}<span class="stat-unit"> default</span>
        {:else}
          —<span class="stat-unit"> no snapshot</span>
        {/if}
      </span>
      <span class="stat-detail">
        {#if snapshot}
          {formatCount(gateways.length, 'gateway')} named ·
          {formatCount(onLinkDefaults, 'on-link default', 'on-link defaults')}
        {:else}
          no routes observed — no snapshot document
        {/if}
      </span>
    </button>

    <button class="stat" data-testid="overview-stat-dns" onclick={() => open('dns')}>
      <span class="stat-top">
        <span class="stat-label">DNS configuration</span>
        {#if dns}
          <span class="evidence {dns.state}">{availabilityEvidenceLabel(dns.state)}</span>
        {:else}
          <span class="evidence not-observed">NOT OBSERVED</span>
        {/if}
      </span>
      <span class="stat-value">
        {#if dns?.state === 'observed'}
          {dnsObservations.length}<span class="stat-unit"> scopes</span>
        {:else if dns?.state === 'observed-empty'}
          0<span class="stat-unit"> observed</span>
        {:else}
          —<span class="stat-unit"> {dns?.label ?? 'no snapshot'}</span>
        {/if}
      </span>
      <span class="stat-detail">
        {#if dns?.state === 'observed'}
          {formatCount(dnsServerCount, 'server')} ·
          {formatCount(dnsLinkScopeCount, 'link-scoped observation')}
        {:else}
          {dns?.detail ?? 'The DNS view explains the provider outcome.'}
        {/if}
      </span>
    </button>

    <button
      class="stat"
      data-testid="overview-stat-connections"
      onclick={() => open('connections')}
    >
      <span class="stat-top">
        <span class="stat-label">Connections</span>
        {#if connections}
          <span class="evidence {connections.state}">
            {availabilityEvidenceLabel(connections.state)}
          </span>
        {:else}
          <span class="evidence not-observed">NOT OBSERVED</span>
        {/if}
      </span>
      <span class="stat-value">
        {#if connections?.state === 'observed'}
          {activeConnections}<span class="stat-unit"> / {connectionObservations.length} active</span>
        {:else if connections?.state === 'observed-empty'}
          0<span class="stat-unit"> observed</span>
        {:else}
          —<span class="stat-unit"> {connections?.label ?? 'no snapshot'}</span>
        {/if}
      </span>
      <span class="stat-detail">
        {#if connections?.state === 'observed'}
          {formatCount(vpnConnections, 'VPN')} ·
          {formatCount(connectionObservations.length, 'connection object')}
        {:else}
          {connections?.detail ?? 'The Connections view explains the provider outcome.'}
        {/if}
      </span>
    </button>
  </div>

  <div class="columns">
    <section class="panel attention" aria-label="Needs attention">
      <h2>Needs attention</h2>
      {#if attention.length === 0}
        <p class="empty-note">
          No down interfaces, failed sections, or unobserved optional services
          are present in this document.
        </p>
      {:else}
        <ul class="attention-list">
          {#each attention as item (item.id)}
            <li>
              <button class="attention-item" onclick={() => open(item.section)}>
                <span class="dot {item.tone}" aria-hidden="true"></span>
                <span class="attention-copy">
                  <span class="attention-title">{item.title}</span>
                  <span class="attention-detail">{item.detail}</span>
                </span>
                <span class="attention-arrow" aria-hidden="true">→</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="panel coverage" aria-label="Observation coverage">
      <div class="panel-head">
        <h2>Observation coverage</h2>
        <button class="text-link" onclick={() => open('providers')}>Provider health →</button>
      </div>
      {#if snapshot && snapshot.providers.length > 0}
        <div class="state-chips">
          {#each providerCounts as entry (entry.state)}
            <span class="state-chip {entry.tone}">
              {entry.count} × {entry.label}
            </span>
          {/each}
        </div>
        <ul class="provider-list">
          {#each snapshot.providers as report, index (index)}
            {@const state = providerStateSummary(report)}
            {@const owner = providerOwnerLabel(report.provider)}
            <li>
              <span class="provider-name">
                {providerDisplayName(report.provider)}
                {#if owner}<span class="owner">{owner}</span>{/if}
              </span>
              <span class="provider-state {state.tone}">{state.label}</span>
              <span class="provider-coverage">{providerCoverage(report)}</span>
            </li>
          {/each}
        </ul>
      {:else if snapshot}
        <p class="empty-note">No provider reports are present in this document.</p>
      {:else}
        <p class="empty-note">No snapshot document is available.</p>
      {/if}
    </section>
  </div>

  <section class="panel evidence-key" aria-label="Evidence key">
    <h2>Reading the evidence</h2>
    <div class="key-grid">
      <div class="key-item">
        <span class="key-mark observed" aria-hidden="true"></span>
        <div>
          <span class="key-label">Observed</span>
          <span class="key-detail">A provider supplied the value; provenance is listed on the view.</span>
        </div>
      </div>
      <div class="key-item">
        <span class="key-mark observed-empty" aria-hidden="true"></span>
        <div>
          <span class="key-label">Observed empty</span>
          <span class="key-detail">A provider looked and found none. This is not missing data.</span>
        </div>
      </div>
      <div class="key-item">
        <span class="key-mark inferred" aria-hidden="true"></span>
        <div>
          <span class="key-label">Inferred</span>
          <span class="key-detail">Derived by the topology projection from observed facts; never presented as an observation.</span>
        </div>
      </div>
      <div class="key-item">
        <span class="key-mark not-observed" aria-hidden="true"></span>
        <div>
          <span class="key-label">Not observed</span>
          <span class="key-detail">No provider attempted or supplied this. Not the same as “none”.</span>
        </div>
      </div>
      <div class="key-item">
        <span class="key-mark unsupported" aria-hidden="true"></span>
        <div>
          <span class="key-label">Unsupported</span>
          <span class="key-detail">The source cannot observe this in principle.</span>
        </div>
      </div>
    </div>
  </section>

  <div class="columns lower">
    <section class="panel provenance" aria-label="Current documents">
      <h2>Current documents</h2>

      {#if dataSource.mode === 'mock'}
        <p class="mode-line" data-testid="overview-scenario">
          Scenario <strong>{scenario.name}</strong> — {scenario.description}.
        </p>
        <p class="provenance-note">{scenario.provenance}.</p>
      {:else if dataSource.mode === 'live'}
        <ul class="doc-list">
          <li>
            <span class="doc-kind">Snapshot</span>
            {#if dataSource.live}
              <span class="mono">live engine observation</span>
              observed {formatObservationTime(dataSource.live.timestampMs)}
            {:else}
              <span class="missing-note">no live snapshot is available</span>
            {/if}
          </li>
          <li>
            <span class="doc-kind">Topology</span>
            {#if dataSource.liveTopology}
              <span class="mono">live engine projection</span>
              from an observation of
              {formatObservationTime(dataSource.liveTopology.sourceTimestampMs)}
            {:else}
              <span class="missing-note">no live topology is available</span>
            {/if}
          </li>
        </ul>
        <p class="provenance-note">
          {#if liveState === 'live'}
            A live engine observation projected from the same validated snapshot.
          {:else if liveState === 'stale'}
            The last confirmed live observation remains visible, but its newer engine state is not confirmed.
          {:else}
            No live observation is currently available; no imported or synthetic document is presented as live.
          {/if}
        </p>
      {:else}
        <ul class="doc-list">
          <li>
            <span class="doc-kind">Snapshot</span>
            {#if snapshotImported()}
              <span class="mono">“{dataSource.snapshot?.fileName}”</span>
              observed {formatObservationTime(dataSource.snapshot!.document.timestampMs)}
            {:else}
              <span class="missing-note">not imported</span>
            {/if}
          </li>
          <li>
            <span class="doc-kind">Topology</span>
            {#if topologyImported()}
              <span class="mono">“{dataSource.topology?.fileName}”</span>
              from an observation of
              {formatObservationTime(dataSource.topology!.document.sourceTimestampMs)}
            {:else}
              <span class="missing-note">not imported</span>
            {/if}
          </li>
        </ul>
        <p class="provenance-note">
          A recorded observation, not a live connection. Observation time and
          import time are different facts.
        </p>
      {/if}

      <dl class="counts">
        {#if snapshot}
          <dt>Observed interfaces</dt>
          <dd class="mono">{snapshot.interfaces.length}</dd>
          <dt>Observed addresses</dt>
          <dd class="mono">{snapshot.addresses.length}</dd>
          <dt>Observed routes</dt>
          <dd class="mono">{snapshot.routes.length}</dd>
          <dt>DNS observations</dt>
          <dd class="mono">
            {snapshot.dns === undefined ? 'not observed' : snapshot.dns.length}
          </dd>
          <dt>Connection observations</dt>
          <dd class="mono">
            {snapshot.connections === undefined ? 'not observed' : snapshot.connections.length}
          </dd>
        {:else}
          <dt>Snapshot</dt>
          <dd class="missing-note">no snapshot document available</dd>
        {/if}
        {#if topologyDocument}
          <dt>Topology nodes</dt>
          <dd class="mono">{topologyDocument.nodes.length}</dd>
          <dt>Evidenced relations</dt>
          <dd class="mono">{topologyDocument.edges.length}</dd>
        {:else}
          <dt>Topology</dt>
          <dd class="missing-note">no topology document available</dd>
        {/if}
      </dl>
      <p class="not-shown">
        Not observed by these documents: reachability, latency, throughput,
        name resolution, remote endpoints, and internet connectivity.
      </p>
    </section>

    <section class="panel sections-panel" aria-label="Sections">
      <h2>Sections</h2>
      <ul class="section-list">
        {#each otherSections as section (section)}
          <li>
            <button
              class="section-link"
              data-testid="overview-link-{section}"
              onclick={() => open(section)}
            >
              <span class="section-label">{SECTION_META[section].label}</span>
              <span class="section-blurb">{SECTION_META[section].blurb}</span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  </div>
</div>

<style>
  .overview-view {
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

  h2 {
    margin: 0 0 12px;
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
    max-width: 620px;
  }

  .stat-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 10px;
    margin-bottom: 14px;
  }

  @media (max-width: 980px) {
    .stat-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @media (max-width: 560px) {
    .stat-grid {
      grid-template-columns: 1fr;
    }
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 4px;
    text-align: left;
    padding: 12px 14px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    background: var(--bg-raised);
    transition: border-color 120ms ease, background 120ms ease;
  }

  .stat:hover {
    background: var(--bg-hover);
    border-color: var(--border-strong);
  }

  .stat-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .stat-label {
    color: var(--text-muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .stat-value {
    font-size: 23px;
    font-weight: 600;
    line-height: 1.2;
    color: var(--text-primary);
  }

  .stat-unit {
    font-size: 12px;
    font-weight: 400;
    color: var(--text-secondary);
  }

  .stat-detail {
    color: var(--text-muted);
    font-size: 11.5px;
    line-height: 1.4;
  }

  .evidence {
    font-size: 9.5px;
    letter-spacing: 0.07em;
    border: 1px solid;
    border-radius: 4px;
    padding: 0 5px;
    white-space: nowrap;
  }

  .evidence.observed,
  .evidence.observed-empty {
    color: var(--state-up);
    border-color: color-mix(in srgb, var(--state-up) 50%, transparent);
  }

  .evidence.not-observed {
    color: var(--state-unknown);
    border-color: color-mix(in srgb, var(--state-unknown) 50%, transparent);
  }

  .evidence.unsupported,
  .evidence.permission-denied {
    color: var(--state-unavailable);
    border-color: color-mix(in srgb, var(--state-unavailable) 50%, transparent);
  }

  .evidence.failed,
  .evidence.unavailable {
    color: var(--state-down);
    border-color: color-mix(in srgb, var(--state-down) 50%, transparent);
  }

  .columns {
    display: grid;
    grid-template-columns: minmax(0, 1.1fr) minmax(0, 0.9fr);
    gap: 14px;
    align-items: start;
    margin-bottom: 14px;
  }

  @media (max-width: 900px) {
    .columns {
      grid-template-columns: 1fr;
    }
  }

  .columns.lower {
    margin-top: 14px;
  }

  .panel {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    background: var(--bg-raised);
    padding: 16px 18px;
  }

  .panel-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
  }

  .text-link {
    color: var(--accent);
    font-size: 12px;
  }

  .text-link:hover {
    text-decoration: underline;
  }

  .attention-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .attention-item {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    width: 100%;
    text-align: left;
    padding: 8px 10px;
    border-radius: var(--radius-s);
    border: 1px solid transparent;
  }

  .attention-item:hover {
    background: var(--bg-hover);
    border-color: var(--border-subtle);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    margin-top: 5px;
    flex: none;
  }

  .dot.bad {
    background: var(--state-down);
  }

  .dot.warn {
    background: var(--state-unknown);
  }

  .dot.info {
    background: var(--accent-blue);
  }

  .attention-copy {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    min-width: 0;
  }

  .attention-title {
    font-size: 12.5px;
    color: var(--text-primary);
    font-weight: 600;
  }

  .attention-detail {
    font-size: 11.5px;
    color: var(--text-muted);
    overflow-wrap: anywhere;
  }

  .attention-arrow {
    color: var(--text-muted);
    flex: none;
  }

  .empty-note {
    margin: 0;
    color: var(--text-muted);
    font-size: 12px;
    line-height: 1.5;
  }

  .state-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 10px;
  }

  .state-chip {
    font-size: 10.5px;
    border: 1px solid;
    border-radius: 999px;
    padding: 1px 8px;
  }

  .state-chip.up {
    color: var(--state-up);
    border-color: color-mix(in srgb, var(--state-up) 50%, transparent);
  }

  .state-chip.down {
    color: var(--state-down);
    border-color: color-mix(in srgb, var(--state-down) 50%, transparent);
  }

  .state-chip.unknown {
    color: var(--state-unknown);
    border-color: color-mix(in srgb, var(--state-unknown) 50%, transparent);
  }

  .state-chip.unavailable {
    color: var(--state-unavailable);
    border-color: color-mix(in srgb, var(--state-unavailable) 50%, transparent);
  }

  .provider-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .provider-list li {
    display: grid;
    grid-template-columns: minmax(140px, 1fr) auto;
    gap: 2px 10px;
    padding-bottom: 7px;
    border-bottom: 1px solid color-mix(in srgb, var(--border-subtle) 60%, transparent);
    font-size: 12px;
  }

  .provider-list li:last-child {
    border-bottom: none;
    padding-bottom: 0;
  }

  .provider-name {
    color: var(--text-primary);
  }

  .owner {
    display: inline-block;
    margin-left: 6px;
    color: var(--accent-blue);
    font-size: 9.5px;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .provider-state {
    justify-self: end;
    font-size: 11px;
  }

  .provider-state.up {
    color: var(--state-up);
  }

  .provider-state.down {
    color: var(--state-down);
  }

  .provider-state.unknown {
    color: var(--state-unknown);
  }

  .provider-state.unavailable {
    color: var(--state-unavailable);
  }

  .provider-coverage {
    grid-column: 1 / -1;
    color: var(--text-muted);
    font-size: 10.5px;
  }

  .evidence-key {
    margin-bottom: 0;
  }

  .key-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 10px 18px;
  }

  .key-item {
    display: flex;
    align-items: flex-start;
    gap: 9px;
  }

  .key-mark {
    width: 14px;
    height: 14px;
    margin-top: 2px;
    flex: none;
    border-radius: 50%;
    border: 2px solid;
  }

  .key-mark.observed {
    border-color: var(--state-up);
    background: color-mix(in srgb, var(--state-up) 25%, transparent);
  }

  .key-mark.observed-empty {
    border-color: var(--state-up);
    background: transparent;
  }

  .key-mark.inferred {
    border: none;
    border-top: 2px dashed var(--accent-blue);
    border-radius: 0;
    height: 0;
    margin-top: 8px;
  }

  .key-mark.not-observed {
    border-color: var(--state-unknown);
    background: transparent;
  }

  .key-mark.unsupported {
    border-radius: 2px;
    border-color: var(--state-unavailable);
    background: transparent;
  }

  .key-label {
    display: block;
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 600;
  }

  .key-detail {
    display: block;
    color: var(--text-muted);
    font-size: 11px;
    line-height: 1.4;
  }

  .mode-line {
    margin: 0 0 6px;
    font-size: 13px;
    color: var(--text-primary);
  }

  .provenance-note {
    margin: 0 0 12px;
    color: var(--text-muted);
    font-size: 11.5px;
    line-height: 1.5;
  }

  .doc-list {
    list-style: none;
    margin: 0 0 10px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-secondary);
  }

  .doc-kind {
    display: inline-block;
    min-width: 76px;
    color: var(--text-muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .missing-note {
    color: var(--state-unknown);
  }

  .counts {
    display: grid;
    grid-template-columns: auto 1fr;
    column-gap: 14px;
    row-gap: 7px;
    margin: 0;
    padding-top: 10px;
    border-top: 1px solid var(--border-subtle);
  }

  .counts dt {
    color: var(--text-muted);
    font-size: 12px;
  }

  .counts dd {
    margin: 0;
    font-size: 12.5px;
    color: var(--text-primary);
    overflow-wrap: anywhere;
  }

  .not-shown {
    margin: 12px 0 0;
    padding-top: 10px;
    border-top: 1px solid var(--border-subtle);
    color: var(--text-muted);
    font-size: 11px;
    line-height: 1.5;
  }

  .section-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .section-link {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    text-align: left;
    padding: 8px 12px;
    border: 1px solid transparent;
    border-radius: var(--radius-m);
  }

  .section-link:hover {
    background: var(--bg-hover);
    border-color: var(--border-subtle);
  }

  .section-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .section-blurb {
    font-size: 11.5px;
    color: var(--text-muted);
    line-height: 1.45;
  }
</style>
