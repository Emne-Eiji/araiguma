<script lang="ts">
  import type { NetworkSnapshot, SectionReport } from '../types';
  import { providerStateLabel } from '../lib/format';
  import MockBadge from './MockBadge.svelte';

  let { snapshot }: { snapshot: NetworkSnapshot } = $props();

  const providerNames: Record<string, string> = {
    netlink: 'rtnetlink (primary)',
    sysfs: 'sysfs / procfs (fallback)',
  };

  function sectionLabel(section: SectionReport['section']): string {
    switch (section) {
      case 'interfaces':
        return 'Interfaces';
      case 'addressesV4':
        return 'IPv4 addresses';
      case 'addressesV6':
        return 'IPv6 addresses';
      case 'routesV4':
        return 'IPv4 routes';
      case 'routesV6':
        return 'IPv6 routes';
    }
  }

  function outcomeSummary(outcome: SectionReport['outcome']): { text: string; tone: string; title: string } {
    switch (outcome.kind) {
      case 'observed':
        return {
          text: `${outcome.count} observed`,
          tone: 'ok',
          title: 'observed (zero is a valid observation)',
        };
      case 'failed':
        return { text: 'failed', tone: 'bad', title: outcome.error };
      case 'unsupported':
        return {
          text: 'not supported by this source',
          tone: 'muted',
          title: 'this provider has no way to observe the section in principle',
        };
      default:
        return { text: 'not observed', tone: 'muted', title: 'not attempted in this run' };
    }
  }
</script>

<div class="providers-view">
  <header class="view-head">
    <div>
      <h1>Provider status</h1>
      <p class="sub">
        What each observation source provided, what failed, and what it
        cannot provide — no invented health facts.
      </p>
    </div>
    <MockBadge />
  </header>

  {#each snapshot.providers as report (report.provider)}
    {@const state = providerStateLabel(report.state)}
    <section class="provider" data-testid="provider-{report.provider}">
      <header class="provider-head">
        <h2>{providerNames[report.provider] ?? report.provider}</h2>
        <span class="state {state.tone}" title="provider state">{state.label}</span>
      </header>

      <table>
        <thead>
          <tr><th>Section</th><th>Outcome</th></tr>
        </thead>
        <tbody>
          {#each report.sections as section (section.section)}
            {@const outcome = outcomeSummary(section.outcome)}
            <tr>
              <td>{sectionLabel(section.section)}</td>
              <td class="outcome {outcome.tone}" title={outcome.title}>
                {outcome.text}
                {#if section.outcome.kind === 'failed'}
                  <span class="error mono">{section.outcome.error}</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>

      {#if report.notes.length > 0}
        <ul class="notes">
          {#each report.notes as note (note)}
            <li>{note}</li>
          {/each}
        </ul>
      {/if}
    </section>
  {/each}

  <p class="footnote">
    A provider failure never removes other providers' observations: the
    merged snapshot keeps every surviving observation and every failure
    report. States: available · degraded (limited fidelity) · partial
    failure · unavailable · not supported here · not yet observed.
  </p>
</div>

<style>
  .providers-view {
    padding: 22px 26px 32px;
    max-width: 900px;
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
    font-size: 15px;
    font-weight: 600;
  }

  .sub {
    margin: 4px 0 0;
    color: var(--text-secondary);
    font-size: 12.5px;
  }

  .provider {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-m);
    background: var(--bg-raised);
    padding: 14px 16px;
    margin-bottom: 14px;
  }

  .provider-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 10px;
  }

  .state {
    padding: 2px 10px;
    border-radius: 999px;
    font-size: 11.5px;
    border: 1px solid;
    white-space: nowrap;
  }

  .state.up {
    color: var(--state-up);
    border-color: color-mix(in srgb, var(--state-up) 55%, transparent);
    background: color-mix(in srgb, var(--state-up) 10%, transparent);
  }

  .state.down {
    color: var(--state-down);
    border-color: color-mix(in srgb, var(--state-down) 55%, transparent);
    background: color-mix(in srgb, var(--state-down) 10%, transparent);
  }

  .state.unknown {
    color: var(--state-unknown);
    border-color: color-mix(in srgb, var(--state-unknown) 55%, transparent);
    background: color-mix(in srgb, var(--state-unknown) 10%, transparent);
  }

  .state.unavailable {
    color: var(--state-unavailable);
    border-color: color-mix(in srgb, var(--state-unavailable) 55%, transparent);
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
    padding-bottom: 6px;
    border-bottom: 1px solid var(--border-subtle);
  }

  td {
    padding: 6px 8px 6px 0;
    border-bottom: 1px solid color-mix(in srgb, var(--border-subtle) 50%, transparent);
  }

  .outcome.ok {
    color: var(--state-up);
  }

  .outcome.bad {
    color: var(--state-down);
  }

  .outcome.muted {
    color: var(--text-muted);
  }

  .error {
    display: block;
    color: var(--text-secondary);
    font-size: 11px;
    margin-top: 2px;
    overflow-wrap: anywhere;
  }

  .notes {
    margin: 10px 0 0;
    padding-left: 16px;
    color: var(--state-unknown);
    font-size: 12px;
  }

  .notes li {
    margin-bottom: 4px;
  }

  .footnote {
    color: var(--text-muted);
    font-size: 11.5px;
  }
</style>
