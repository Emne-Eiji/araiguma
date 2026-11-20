<script lang="ts">
  // Overview section (WO-04 Stage A): an honest summary of what the app is
  // currently rendering. Every number is derived from the current documents
  // (imported, or the labeled synthetic scenario); nothing is invented, and
  // a missing document is stated as missing — never silently substituted.

  import {
    dataSource,
    currentSnapshot,
    currentTopology,
    snapshotImported,
    topologyImported,
  } from '../lib/dataMode.svelte';
  import { scenarioById } from '../lib/scenarios';
  import { formatObservationTime } from '../lib/import';
  import { navigateTo, SECTIONS, SECTION_META } from '../lib/navigation.svelte';
  import type { SectionId } from '../lib/navigation.svelte';
  import DataBadge from './DataBadge.svelte';

  const scenario = $derived(scenarioById(dataSource.mockScenario));
  const snapshotAvailable = $derived(dataSource.mode === 'mock' || snapshotImported());
  const topologyAvailable = $derived(dataSource.mode === 'mock' || topologyImported());

  const snapshot = $derived(currentSnapshot());
  const topologyDocument = $derived(currentTopology());

  const providerSummary = $derived(
    snapshot.providers.map((p) => `${p.provider}: ${p.state}`).join(' · '),
  );

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
      <h1>Overview</h1>
      <p class="sub">
        A read-only observer. What follows is a summary of the documents the
        views render — nothing more than what was observed.
      </p>
    </div>
    <DataBadge />
  </header>

  <div class="columns">
    <section class="panel documents-summary" aria-label="Current documents">
      <h2>Current documents</h2>

      {#if dataSource.mode === 'mock'}
        <p class="mode-line" data-testid="overview-scenario">
          Scenario <strong>{scenario.name}</strong> — {scenario.description}.
        </p>
        <p class="provenance-note">{scenario.provenance}.</p>
      {:else}
        <ul class="doc-list">
          <li>
            <span class="doc-kind">Snapshot</span>
            {#if snapshotImported()}
              <span class="mono">“{dataSource.snapshot?.fileName}”</span>
              observed {formatObservationTime(dataSource.snapshot!.document.timestampMs)}
            {:else}
              <span class="missing-note">
                not imported — the views that need it say so explicitly
              </span>
            {/if}
          </li>
          <li>
            <span class="doc-kind">Topology</span>
            {#if topologyImported()}
              <span class="mono">“{dataSource.topology?.fileName}”</span>
              from an observation of
              {formatObservationTime(dataSource.topology!.document.sourceTimestampMs)}
            {:else}
              <span class="missing-note">
                not imported — the topology view says so explicitly
              </span>
            {/if}
          </li>
        </ul>
        <p class="provenance-note">
          A recorded observation, not a live connection. Observation time and
          import time are different facts; the Documents &amp; Import section
          keeps them distinct.
        </p>
      {/if}

      <dl class="counts">
        {#if snapshotAvailable}
          <dt>Observed interfaces</dt>
          <dd class="mono">{snapshot.interfaces.length}</dd>
          <dt>Observed addresses</dt>
          <dd class="mono">{snapshot.addresses.length}</dd>
          <dt>Observed routes</dt>
          <dd class="mono">{snapshot.routes.length}</dd>
          <dt>Provider states</dt>
          <dd>{providerSummary}</dd>
        {:else}
          <dt>Snapshot</dt>
          <dd class="missing-note">no snapshot document imported</dd>
        {/if}
        {#if topologyAvailable}
          <dt>Topology nodes</dt>
          <dd class="mono">{topologyDocument.nodes.length}</dd>
          <dt>Evidenced relations</dt>
          <dd class="mono">{topologyDocument.edges.length}</dd>
        {:else}
          <dt>Topology</dt>
          <dd class="missing-note">no topology document imported</dd>
        {/if}
      </dl>
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
    max-width: 1100px;
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
    max-width: 520px;
  }

  .columns {
    display: grid;
    grid-template-columns: minmax(320px, 5fr) minmax(260px, 4fr);
    gap: 14px;
    align-items: start;
  }

  @media (max-width: 860px) {
    .columns {
      grid-template-columns: 1fr;
    }
  }

  .panel {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    background: var(--bg-raised);
    padding: 16px 18px;
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
