<script lang="ts">
  // Documents & Import section (WO-04 Stage A): the explicit home of the
  // document workflow. It reports, per document kind, the honest state —
  // imported OK / missing / not observed — plus the outcome of the most
  // recent import attempt (rejected imports are classified as failed
  // validation or unsupported), and presents the scenario registry with its
  // explicit synthetic provenance.

  import {
    dataSource,
    selectMockScenario,
    type ImportAttempt,
  } from '../lib/dataMode.svelte';
  import { scenarioRegistry, scenarioById, type ScenarioId } from '../lib/scenarios';
  import { documentViews, type DocumentSlotView } from '../lib/documents';
  import DataBadge from './DataBadge.svelte';

  let { onimportclick }: { onimportclick: () => void } = $props();

  const scenario = $derived(scenarioById(dataSource.mockScenario));
  const lastImport: ImportAttempt | null = $derived(dataSource.lastImport);

  const views = $derived(
    documentViews(
      {
        mode: dataSource.mode,
        snapshot: dataSource.snapshot,
        topology: dataSource.topology,
        mockScenarioName: scenario.name,
      },
      lastImport,
    ),
  );

  const statusMeta: Record<
    DocumentSlotView['status'],
    { label: string; tone: string; note: string }
  > = {
    'imported-ok': {
      label: 'imported OK',
      tone: 'ok',
      note: 'validated in full before anything was displayed',
    },
    missing: {
      label: 'missing',
      tone: 'missing',
      note: 'imported mode, but no document of this kind — the views that need it say so explicitly, never substituted mock data',
    },
    'not-observed': {
      label: 'not observed',
      tone: 'muted',
      note: 'no real document of this kind: the labeled synthetic scenario is shown instead',
    },
  };

  function slotTitle(kind: 'snapshot' | 'topology'): string {
    return kind === 'snapshot' ? 'Observation snapshot' : 'Topology document';
  }

  function slotProduceHint(kind: 'snapshot' | 'topology'): string {
    return kind === 'snapshot'
      ? 'araiguma snapshot > snapshot.json'
      : 'araiguma topology --input snapshot.json > topology.json';
  }

  function selectScenario(id: ScenarioId): void {
    selectMockScenario(id);
  }
</script>

<div class="documents-view" data-testid="documents-view">
  <header class="view-head">
    <div>
      <h1>Documents &amp; Import</h1>
      <p class="sub">
        Import observation documents produced by the Araiguma CLI. Documents
        are validated in full before anything is displayed; a rejected import
        never changes what is shown.
      </p>
    </div>
    <DataBadge />
  </header>

  <section class="panel import-panel" aria-label="Import">
    <button
      class="import-button"
      data-testid="documents-import-button"
      onclick={onimportclick}
    >
      Import observation document…
    </button>
    <p class="import-note">
      Accepted: an Araiguma snapshot (interfaces, addresses, routes, providers)
      or a topology document (nodes, edges), as JSON. Produce them with the
      CLI: <span class="mono">araiguma snapshot &gt; snapshot.json</span> or
      <span class="mono">araiguma topology --input snapshot.json &gt; topology.json</span>.
    </p>
  </section>

  <div class="slots">
    {#each [views.snapshotSlot, views.topologySlot] as slot (slot.kind)}
      {@const meta = statusMeta[slot.status]}
      <section
        class="panel slot-card"
        data-testid="document-slot-{slot.kind}"
        aria-label="{slotTitle(slot.kind)} state"
      >
        <header class="slot-head">
          <h2>{slotTitle(slot.kind)}</h2>
          <span class="status {meta.tone}" data-testid="document-status-{slot.kind}">
            {meta.label}
          </span>
        </header>

        {#if slot.status === 'imported-ok'}
          <dl class="slot-facts">
            <dt>File</dt>
            <dd class="mono">{slot.fileName}</dd>
            <dt>Observed</dt>
            <dd>{slot.observedAt} <span class="hint">(when the machine was observed)</span></dd>
            <dt>Imported</dt>
            <dd>{slot.importedAt} <span class="hint">(when the file was read — a different fact)</span></dd>
          </dl>
          <p class="slot-note">{meta.note}</p>
        {:else if slot.status === 'missing'}
          <p class="slot-note">{meta.note}.</p>
          <p class="produce">
            Produce one with <span class="mono">{slotProduceHint(slot.kind)}</span>
            and import it above, or return to the demonstration data.
          </p>
        {:else}
          <p class="slot-note">{meta.note}.</p>
          <p class="produce">
            Currently shown: scenario
            <strong>{slot.scenarioName}</strong> (synthetic, labeled).
          </p>
        {/if}
      </section>
    {/each}
  </div>

  {#if views.lastRejection}
    <section
      class="panel rejection-panel"
      data-testid="last-rejection"
      aria-label="Last rejected import"
    >
      <header class="slot-head">
        <h2>Last import attempt — rejected</h2>
        <span class="status bad" data-testid="rejection-class">
          {views.lastRejection.rejectionClass === 'unsupported'
            ? 'unsupported'
            : 'failed validation'}
        </span>
      </header>
      <p class="rejection-line">
        <span class="mono">“{views.lastRejection.fileName}”</span>
        {#if views.lastRejection.kind}
          (recognized as a {views.lastRejection.kind} document)
        {:else}
          (not recognizable as an Araiguma document)
        {/if}
      </p>
      <p class="rejection-reason" data-testid="rejection-reason">
        {views.lastRejection.reason}
      </p>
      {#if views.lastRejection.rejectionClass === 'unsupported'}
        <p class="slot-note">
          The document names a schema version this build does not support.
          Nothing was applied; the previously valid documents are untouched.
        </p>
      {:else}
        <p class="slot-note">
          The document failed validation. Nothing was applied; the previously
          valid documents are untouched.
        </p>
      {/if}
    </section>
  {:else if views.lastAccepted}
    <section class="panel accepted-panel" data-testid="last-accepted" aria-label="Last accepted import">
      <p class="rejection-line">
        Last import accepted: <span class="mono">“{views.lastAccepted.fileName}”</span>
        ({views.lastAccepted.kind} document) — validated and applied.
      </p>
    </section>
  {/if}

  <section class="panel scenarios-panel" aria-label="Demonstration scenarios">
    <header class="slot-head">
      <h2>Demonstration scenarios</h2>
      {#if dataSource.mode === 'imported'}
        <span class="status muted">selecting returns to mock data</span>
      {/if}
    </header>
    <p class="slot-note">
      Every scenario is a checked-in synthetic fixture pair (snapshot + Rust
      projected topology golden) — explicitly labeled mock data, never an
      observation of any machine.
    </p>
    <ul class="scenario-list">
      {#each scenarioRegistry as entry (entry.id)}
        <li>
          <button
            class="scenario-item"
            class:active={dataSource.mode === 'mock' && dataSource.mockScenario === entry.id}
            data-testid="scenario-{entry.id}"
            aria-pressed={dataSource.mode === 'mock' && dataSource.mockScenario === entry.id}
            onclick={() => selectScenario(entry.id)}
          >
            <span class="scenario-head">
              <span class="scenario-name">{entry.name}</span>
              {#if dataSource.mode === 'mock' && dataSource.mockScenario === entry.id}
                <span class="active-mark" data-testid="scenario-active-mark">shown now</span>
              {/if}
            </span>
            <span class="scenario-desc">{entry.description}</span>
            <span class="scenario-class">{entry.classificationLabel}</span>
          </button>
        </li>
      {/each}
    </ul>
  </section>
</div>

<style>
  .documents-view {
    padding: 22px 26px 32px;
    max-width: 980px;
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
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }

  .sub {
    margin: 4px 0 0;
    color: var(--text-secondary);
    font-size: 12.5px;
    max-width: 560px;
  }

  .panel {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    background: var(--bg-raised);
    padding: 14px 16px;
  }

  .import-panel {
    display: flex;
    align-items: center;
    gap: 16px;
    flex-wrap: wrap;
    margin-bottom: 14px;
    padding: 16px 18px;
  }

  .import-button {
    padding: 9px 16px;
    border: 1px solid color-mix(in srgb, var(--accent-blue) 55%, transparent);
    border-radius: var(--radius-m);
    color: var(--accent-blue);
    font-size: 13px;
    font-weight: 600;
    flex: none;
  }

  .import-button:hover {
    background: color-mix(in srgb, var(--accent-blue) 12%, transparent);
  }

  .import-note {
    flex: 1;
    min-width: 260px;
    margin: 0;
    color: var(--text-muted);
    font-size: 11.5px;
    line-height: 1.55;
  }

  .slots {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 14px;
    margin-bottom: 14px;
  }

  .slot-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 10px;
  }

  .status {
    padding: 2px 10px;
    border-radius: 999px;
    font-size: 11px;
    border: 1px solid;
    white-space: nowrap;
    flex: none;
  }

  .status.ok {
    color: var(--state-up);
    border-color: color-mix(in srgb, var(--state-up) 55%, transparent);
    background: color-mix(in srgb, var(--state-up) 10%, transparent);
  }

  .status.bad {
    color: var(--state-down);
    border-color: color-mix(in srgb, var(--state-down) 55%, transparent);
    background: color-mix(in srgb, var(--state-down) 10%, transparent);
  }

  .status.missing {
    color: var(--state-unknown);
    border-color: color-mix(in srgb, var(--state-unknown) 55%, transparent);
    background: color-mix(in srgb, var(--state-unknown) 10%, transparent);
  }

  .status.muted {
    color: var(--text-muted);
    border-color: var(--border-subtle);
  }

  .slot-facts {
    display: grid;
    grid-template-columns: 76px 1fr;
    row-gap: 6px;
    margin: 0 0 10px;
  }

  .slot-facts dt {
    color: var(--text-muted);
    font-size: 12px;
    padding-top: 1px;
  }

  .slot-facts dd {
    margin: 0;
    font-size: 12.5px;
    overflow-wrap: anywhere;
  }

  .hint {
    color: var(--text-muted);
    font-size: 11px;
  }

  .slot-note {
    margin: 0;
    color: var(--text-secondary);
    font-size: 12px;
    line-height: 1.55;
  }

  .produce {
    margin: 8px 0 0;
    color: var(--text-muted);
    font-size: 11.5px;
    line-height: 1.55;
  }

  .rejection-panel {
    margin-bottom: 14px;
    border-color: color-mix(in srgb, var(--state-down) 45%, transparent);
  }

  .accepted-panel {
    margin-bottom: 14px;
  }

  .rejection-line {
    margin: 0 0 6px;
    font-size: 12.5px;
    color: var(--text-primary);
  }

  .rejection-reason {
    margin: 0 0 8px;
    padding: 8px 10px;
    border-radius: var(--radius-s);
    background: color-mix(in srgb, var(--state-down) 8%, transparent);
    color: var(--state-down);
    font-size: 11.5px;
    font-family: var(--font-mono);
    overflow-wrap: anywhere;
    line-height: 1.5;
  }

  .scenarios-panel {
    margin-bottom: 0;
  }

  .scenario-list {
    list-style: none;
    margin: 12px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .scenario-item {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    text-align: left;
    padding: 9px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
  }

  .scenario-item:hover {
    background: var(--bg-hover);
  }

  .scenario-item.active {
    border-color: color-mix(in srgb, var(--accent) 55%, transparent);
    background: color-mix(in srgb, var(--accent) 8%, transparent);
  }

  .scenario-head {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .scenario-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .active-mark {
    padding: 1px 8px;
    border-radius: 999px;
    border: 1px solid color-mix(in srgb, var(--accent) 55%, transparent);
    color: var(--accent);
    font-size: 10.5px;
    font-family: var(--font-mono);
    letter-spacing: 0.05em;
  }

  .scenario-desc {
    font-size: 11.5px;
    color: var(--text-secondary);
    line-height: 1.5;
  }

  .scenario-class {
    font-size: 11px;
    color: var(--text-muted);
  }
</style>
