<script lang="ts">
  // DNS Configuration section (WO-08). The v4 snapshot contract carries
  // resolver configuration observations as an OPTIONAL collection: absent
  // means "not observed", [] means "observed empty". This view renders
  // exactly that distinction, attributes each observation to its provider,
  // and never claims that name resolution succeeds — configuration evidence
  // is not a resolution observation.

  import type { DnsObservation, NetworkSnapshot } from '../types';
  import {
    dnsAvailability,
    dnsScopeLabel,
    dnsServerText,
    formatCount,
    outcomeSummary,
    providerDisplayName,
  } from '../lib/observability';
  import { navigate } from '../lib/navigation.svelte';
  import DataBadge from './DataBadge.svelte';

  let { snapshot }: { snapshot: NetworkSnapshot } = $props();

  const availability = $derived(dnsAvailability(snapshot));
  const observations = $derived(snapshot.dns ?? []);
  const serverCount = $derived(
    observations.reduce((total, observation) => total + observation.servers.length, 0),
  );
  const scopeCount = $derived(
    new Set(
      observations.map((observation) =>
        observation.scope.kind === 'global'
          ? 'global'
          : `link:${observation.scope.ifindex}`,
      ),
    ).size,
  );

  const dnsReports = $derived(
    snapshot.providers.flatMap((report) =>
      report.sections
        .filter((entry) => entry.section === 'dns')
        .map((entry) => ({
          provider: report.provider,
          state: report.state,
          summary: outcomeSummary(entry.outcome),
        })),
    ),
  );

  const groups = $derived.by(() => {
    const byScope = new Map<
      string,
      { key: string; label: string; observations: DnsObservation[] }
    >();
    for (const observation of observations) {
      const key =
        observation.scope.kind === 'global'
          ? 'global'
          : `link:${observation.scope.ifindex}`;
      const existing = byScope.get(key);
      if (existing) {
        existing.observations.push(observation);
      } else {
        byScope.set(key, {
          key,
          label: dnsScopeLabel(observation.scope, snapshot),
          observations: [observation],
        });
      }
    }
    return [...byScope.values()].sort((a, b) => {
      if (a.key === 'global') return -1;
      if (b.key === 'global') return 1;
      return a.key.localeCompare(b.key);
    });
  });

  function scopeInterface(observation: DnsObservation): number | null {
    return observation.scope.kind === 'link' ? observation.scope.ifindex : null;
  }

  function showInterface(ifindex: number): void {
    navigate({ section: 'interfaces', selection: { kind: 'interface', ifindex } });
  }
</script>

<div class="dns-view" data-testid="dns-view">
  <header class="view-head">
    <div>
      <h1>DNS configuration</h1>
      <p class="sub">
        Resolver configuration observed in the snapshot, grouped by scope.
        Configuration evidence only — Araiguma never performs or observes a
        name-resolution query.
      </p>
    </div>
    <DataBadge />
  </header>

  <section class="panel provider-strip" aria-label="DNS provider report">
    <div class="strip-head">
      <h2>Provider report</h2>
      <span class="availability {availability.tone}">{availability.label}</span>
    </div>
    <p class="strip-detail">{availability.detail}</p>
    {#if dnsReports.length > 0}
      <ul class="report-list">
        {#each dnsReports as report, i (i)}
          <li>
            <span class="report-provider">{providerDisplayName(report.provider)}</span>
            <span class="report-outcome {report.summary.tone}">{report.summary.label}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="muted">No provider report mentions DNS in this document.</p>
    {/if}
  </section>

  {#if availability.state === 'observed-empty'}
    <div class="empty-state" data-testid="dns-empty">
      <p>Observed: no DNS configuration.</p>
      <p class="sub">
        A provider looked for resolver configuration and found none. This is a
        valid observation — not missing data.
      </p>
    </div>
  {:else if availability.state !== 'observed'}
    <div class="empty-state" data-testid="dns-{availability.state}">
      <p>No DNS configuration is shown.</p>
      <p class="sub">{availability.detail}</p>
      <p class="sub">
        The provider report above is the reason. Nothing is substituted from
        another scenario or document.
      </p>
    </div>
  {:else}
    <div class="summary-line">
      {formatCount(observations.length, 'observation')} ·
      {formatCount(serverCount, 'server')} ·
      {formatCount(scopeCount, 'scope')}
    </div>

    {#each groups as group (group.key)}
      <section class="scope-group" aria-label={group.label}>
        <h2 class="scope-heading">{group.label}</h2>
        {#each group.observations as observation, index (group.key + ':' + index)}
          <article class="dns-card" data-testid="dns-observation-{group.key}-{index}">
            <header class="card-head">
              <div class="chips">
                <span class="chip scope">{group.label}</span>
                {#if observation.mode}
                  <span class="chip mode mono" title="observed resolver mode">{observation.mode}</span>
                {:else}
                  <span class="chip muted" title="the source did not report a resolver mode">mode not observed</span>
                {/if}
              </div>
              <div class="chips">
                {#each observation.sources as source, i (i)}
                  <span class="source">{source}</span>
                {/each}
              </div>
            </header>

            <div class="facts">
              <section>
                <h3>Servers ({observation.servers.length})</h3>
                {#if observation.servers.length === 0}
                  <p class="muted">no servers observed</p>
                {:else}
                  <ul class="server-list">
                    {#each observation.servers as server, i (i)}
                      <li>
                        <span class="mono server-address">{dnsServerText(server)}</span>
                        {#if server.port === undefined}
                          <span class="port-note">port not observed</span>
                        {/if}
                      </li>
                    {/each}
                  </ul>
                {/if}
              </section>

              <section>
                <h3>Search domains ({observation.searchDomains.length})</h3>
                {#if observation.searchDomains.length === 0}
                  <p class="muted">none present in this observation</p>
                {:else}
                  <ul class="domain-list">
                    {#each observation.searchDomains as domain, i (i)}
                      <li class="mono">{domain}</li>
                    {/each}
                  </ul>
                {/if}
              </section>

              <section>
                <h3>Routing domains ({observation.routingDomains.length})</h3>
                {#if observation.routingDomains.length === 0}
                  <p class="muted">none present in this observation</p>
                {:else}
                  <ul class="domain-list">
                    {#each observation.routingDomains as domain, i (i)}
                      <li class="mono">{domain}</li>
                    {/each}
                  </ul>
                {/if}
              </section>
            </div>

            {#if observation.notes.length > 0}
              <section class="notes-block">
                <h3>Observation notes</h3>
                <ul class="notes">
                  {#each observation.notes as note, i (i)}<li>{note}</li>{/each}
                </ul>
              </section>
            {/if}

            {#if scopeInterface(observation) !== null}
              <footer class="card-foot">
                <button
                  class="link-button"
                  onclick={() => showInterface(scopeInterface(observation)!)}
                >
                  Inspect {dnsScopeLabel(observation.scope, snapshot).replace('Link scope · ', '')} →
                </button>
              </footer>
            {/if}
          </article>
        {/each}
      </section>
    {/each}

    <p class="footnote">
      A configured server is not a claim that it answers, that it is reachable,
      or that any name resolves. Ports shown as “port not observed” were not
      supplied by the source; no default port is assumed.
    </p>
  {/if}
</div>

<style>
  .dns-view {
    padding: 22px 26px 32px;
    max-width: 1100px;
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

  h3 {
    margin: 0 0 6px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
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

  .scope-group {
    margin-bottom: 16px;
  }

  .scope-heading {
    margin: 0 0 8px;
    color: var(--text-secondary);
  }

  .dns-card {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    background: var(--bg-raised);
    padding: 14px 16px;
    margin-bottom: 10px;
  }

  .card-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: 12px;
  }

  .chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    align-items: center;
  }

  .chip {
    font-size: 11px;
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    padding: 1px 8px;
    color: var(--text-secondary);
  }

  .chip.scope {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
    background: color-mix(in srgb, var(--accent) 8%, transparent);
  }

  .chip.mode {
    color: var(--accent-blue);
    border-color: color-mix(in srgb, var(--accent-blue) 45%, transparent);
  }

  .chip.muted {
    color: var(--text-muted);
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

  .facts {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 14px;
  }

  @media (max-width: 760px) {
    .facts {
      grid-template-columns: 1fr;
    }
  }

  .server-list,
  .domain-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12.5px;
  }

  .server-address {
    color: var(--text-primary);
  }

  .port-note {
    display: block;
    color: var(--text-muted);
    font-size: 10.5px;
  }

  .domain-list li {
    color: var(--text-secondary);
  }

  .muted {
    color: var(--text-muted);
    font-size: 12px;
    margin: 0;
  }

  .notes-block {
    margin-top: 12px;
    padding-top: 10px;
    border-top: 1px solid var(--border-subtle);
  }

  .notes {
    margin: 0;
    padding-left: 16px;
    color: var(--state-unknown);
    font-size: 12px;
  }

  .card-foot {
    margin-top: 10px;
  }

  .link-button {
    color: var(--accent);
    font-size: 12px;
  }

  .link-button:hover {
    text-decoration: underline;
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
