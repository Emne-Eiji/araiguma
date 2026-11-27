<script lang="ts">
  // Providers & Observation Health (WO-08 redesign): a provider × section
  // coverage matrix. The matrix shows exactly what each provider reported,
  // in the contract's outcome vocabulary, with no invented health facts.
  //
  //   observed N            — a provider supplied N records (0 is valid)
  //   not supported         — cannot observe in principle
  //   not observed          — NotObserved: not attempted in this run
  //   failed                — Failed, with the permitted error code
  //   unavailable           — Unavailable: service/resource unavailable now
  //   permission denied     — PermissionDenied
  //   not reported          — the provider has no SectionReport entry
  //
  // A provider failure never removes other providers' observations.

  import type { NetworkSnapshot, SectionKind } from '../types';
  import type { EvidenceTone } from '../lib/observability';
  import DataBadge from './DataBadge.svelte';
  import {
    outcomeSummary,
    providerDisplayName,
    providerOwnerLabel,
    providerStateCounts,
    providerStateSummary,
    SECTION_ORDER,
    sectionLabel,
    sectionOwner,
  } from '../lib/observability';

  let { snapshot }: { snapshot: NetworkSnapshot } = $props();

  // Columns: the v4 sections plus any additional section a provider reports,
  // preserving unknown kinds instead of dropping them.
  const columns = $derived.by(() => {
    const known = new Set<string>(SECTION_ORDER);
    const extra: string[] = [];
    for (const report of snapshot.providers) {
      for (const section of report.sections) {
        const key = section.section;
        if (!known.has(key) && !extra.includes(key)) extra.push(key);
      }
    }
    return [...SECTION_ORDER, ...extra];
  });

  const providerStates = $derived(providerStateCounts(snapshot.providers));

  function reportFor(provider: string, section: SectionKind) {
    const report = snapshot.providers.find((candidate) => candidate.provider === provider);
    return report?.sections.find((entry) => entry.section === section);
  }

  interface Cell {
    short: string;
    label: string;
    tone: EvidenceTone;
    title: string;
  }

  function cellFor(provider: string, section: SectionKind): Cell {
    const entry = reportFor(provider, section);
    if (!entry) {
      return {
        short: 'not reported',
        label: `${providerDisplayName(provider)} has no report for ${sectionLabel(section)}`,
        tone: 'muted',
        title: 'provider did not report this section; nothing is inferred',
      };
    }
    const summary = outcomeSummary(entry.outcome);
    return { short: summary.short, label: summary.label, tone: summary.tone, title: summary.title };
  }

  const noProviders = $derived(snapshot.providers.length === 0);
</script>

<div class="providers-view" data-testid="providers-view">
  <header class="view-head">
    <div>
      <h1>Providers &amp; observation health</h1>
      <p class="sub">
        What each observation source provided, what failed, and what it
        cannot provide — no invented health facts.
      </p>
    </div>
    <DataBadge />
  </header>

  {#if noProviders}
    <div class="empty-state" data-testid="providers-empty">
      <p>No provider reports are present in this document.</p>
      <p class="sub">
        This document cannot say which providers ran; absence of a report is
        not provider success.
      </p>
    </div>
  {:else}
    <div class="summary-line">
      {snapshot.providers.length}
      {snapshot.providers.length === 1 ? 'provider' : 'providers'} ·
      {#each providerStates as entry, index (entry.state)}
        {#if index > 0}·{/if}
        <span class="state-chip {entry.tone}">{entry.count} × {entry.label}</span>
      {/each}
    </div>

    <div class="matrix-wrap">
      <table class="matrix">
        <caption class="sr-only">
          Provider by section coverage matrix. Cell text is shortened; the
          accessible label names the full outcome.
        </caption>
        <thead>
          <tr>
            <th scope="col" class="provider-col">Provider</th>
            {#each columns as section (section)}
              {@const owner = sectionOwner(section as SectionKind)}
              <th scope="col">
                <span class="col-label">{sectionLabel(section)}</span>
                {#if owner}
                  <span class="owner-note">owned by {providerDisplayName(owner)}</span>
                {/if}
              </th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each snapshot.providers as report, index (index)}
            {@const state = providerStateSummary(report)}
            {@const owner = providerOwnerLabel(report.provider)}
            <tr data-testid="provider-{report.provider}">
              <th scope="row" class="provider-col">
                <span class="provider-name">{providerDisplayName(report.provider)}</span>
                <span class="provider-id mono">{report.provider}</span>
                <span class="state {state.tone}">{state.label}</span>
                {#if owner}<span class="owner">{owner}</span>{/if}
              </th>
              {#each columns as section (section)}
                {@const cell = cellFor(report.provider, section as SectionKind)}
                <td
                  class="cell {cell.tone}"
                  title={cell.title}
                  aria-label={`${sectionLabel(section as SectionKind)}: ${cell.label}`}
                >
                  {cell.short}
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <section class="vocab" aria-label="Outcome vocabulary">
      <h2>Outcome vocabulary</h2>
      <ul class="vocab-grid">
        <li><span class="key ok">N</span> observed N records — a zero is a valid observation</li>
        <li><span class="key muted">n/a</span> not supported by this source</li>
        <li><span class="key muted">—</span> not observed — not attempted in this run</li>
        <li><span class="key bad">failed</span> the section failed; error code shown in title</li>
        <li><span class="key bad">unavailable</span> service or resource unavailable for this run</li>
        <li><span class="key bad">denied</span> permission denied by policy</li>
        <li><span class="key muted">not reported</span> provider has no report entry</li>
      </ul>
    </section>

    {#each snapshot.providers as report, index (index)}
      <section class="provider-notes" aria-label="Notes for {report.provider}">
        <h2>{providerDisplayName(report.provider)} notes</h2>
        {#if report.notes.length > 0}
          <ul class="notes">
            {#each report.notes as note, i (i)}<li>{note}</li>{/each}
          </ul>
        {:else}
          <p class="muted">No provider notes.</p>
        {/if}
      </section>
    {/each}

    <p class="footnote">
      A provider failure never removes other providers' observations: the
      merged snapshot keeps every surviving observation and every failure
      report. Provider states: available · degraded (limited fidelity) ·
      partial / incomplete run · unavailable · not supported here · not yet
      observed.
    </p>
  {/if}
</div>

<style>
  .providers-view {
    padding: 22px 26px 32px;
    max-width: 1100px;
  }

  .view-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 14px;
  }

  h1 {
    margin: 0;
    font-size: 19px;
    font-weight: 600;
  }

  h2 {
    margin: 0 0 8px;
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
  }

  .summary-line {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
    color: var(--text-secondary);
    font-size: 12px;
    margin-bottom: 12px;
  }

  .state-chip {
    font-size: 10.5px;
    border: 1px solid;
    border-radius: 999px;
    padding: 1px 8px;
  }

  .state-chip.up { color: var(--state-up); border-color: color-mix(in srgb, var(--state-up) 50%, transparent); }
  .state-chip.down { color: var(--state-down); border-color: color-mix(in srgb, var(--state-down) 50%, transparent); }
  .state-chip.unknown { color: var(--state-unknown); border-color: color-mix(in srgb, var(--state-unknown) 50%, transparent); }
  .state-chip.unavailable { color: var(--state-unavailable); border-color: color-mix(in srgb, var(--state-unavailable) 50%, transparent); }

  .matrix-wrap {
    overflow-x: auto;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    background: var(--bg-raised);
    margin-bottom: 16px;
  }

  .matrix {
    border-collapse: collapse;
    width: 100%;
    min-width: 760px;
    font-size: 12px;
  }

  .matrix caption { padding: 8px; }

  th {
    text-align: left;
    vertical-align: bottom;
    color: var(--text-muted);
    font-weight: 600;
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 9px 8px;
    border-bottom: 1px solid var(--border-subtle);
    border-right: 1px solid color-mix(in srgb, var(--border-subtle) 55%, transparent);
    background: var(--bg-raised);
  }

  .col-label { display: block; }

  .owner-note {
    display: block;
    margin-top: 2px;
    color: var(--accent-blue);
    font-size: 9px;
    letter-spacing: 0.04em;
    text-transform: none;
    font-weight: 400;
  }

  .provider-col {
    min-width: 180px;
    border-right: 1px solid var(--border-subtle);
    vertical-align: top;
  }

  tbody th.provider-col {
    background: var(--bg-inset, var(--bg-panel));
  }

  .provider-name {
    display: block;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .provider-id {
    display: block;
    color: var(--text-muted);
    font-size: 10px;
    margin: 1px 0 5px;
  }

  .owner {
    display: inline-block;
    margin-top: 4px;
    color: var(--accent-blue);
    font-size: 9.5px;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .state {
    display: inline-block;
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 10.5px;
    border: 1px solid;
    white-space: nowrap;
  }

  .state.up { color: var(--state-up); border-color: color-mix(in srgb, var(--state-up) 55%, transparent); }
  .state.down { color: var(--state-down); border-color: color-mix(in srgb, var(--state-down) 55%, transparent); }
  .state.unknown { color: var(--state-unknown); border-color: color-mix(in srgb, var(--state-unknown) 55%, transparent); }
  .state.unavailable { color: var(--state-unavailable); border-color: color-mix(in srgb, var(--state-unavailable) 55%, transparent); }

  .cell {
    text-align: center;
    padding: 9px 8px;
    border-bottom: 1px solid color-mix(in srgb, var(--border-subtle) 50%, transparent);
    border-right: 1px solid color-mix(in srgb, var(--border-subtle) 45%, transparent);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .cell.ok { color: var(--state-up); }
  .cell.bad { color: var(--state-down); }
  .cell.muted { color: var(--text-muted); }
  .cell.warn { color: var(--state-unknown); }

  .vocab {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    background: var(--bg-raised);
    padding: 13px 16px;
    margin-bottom: 14px;
  }

  .vocab-grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 6px 16px;
    font-size: 11.5px;
    color: var(--text-secondary);
  }

  .key {
    display: inline-block;
    min-width: 58px;
    text-align: center;
    border: 1px solid;
    border-radius: 4px;
    padding: 0 5px;
    font-size: 10px;
    margin-right: 6px;
  }

  .key.ok { color: var(--state-up); border-color: color-mix(in srgb, var(--state-up) 45%, transparent); }
  .key.bad { color: var(--state-down); border-color: color-mix(in srgb, var(--state-down) 45%, transparent); }
  .key.muted { color: var(--text-muted); border-color: var(--border-subtle); }

  .provider-notes {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    background: var(--bg-raised);
    padding: 12px 16px;
    margin-bottom: 10px;
  }

  .notes {
    margin: 0;
    padding-left: 16px;
    color: var(--state-unknown);
    font-size: 12px;
  }

  .notes li { margin-bottom: 3px; }

  .muted { color: var(--text-muted); font-size: 11.5px; margin: 0; }

  .footnote {
    color: var(--text-muted);
    font-size: 11.5px;
    line-height: 1.5;
    max-width: 800px;
  }

  .empty-state {
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-m);
    padding: 28px;
    text-align: center;
  }

  .empty-state p { margin: 0 0 6px; }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
</style>
