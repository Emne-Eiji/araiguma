<script lang="ts">
  // Navigation rail (WO-04 Stage A; WO-08 adds DNS and Connections): stable
  // sections. Two modes — the default compact icon-only 56px rail with a
  // hover label preview, and a pinned expanded mode with persistent labels
  // (a session-only choice).
  // The bottom section carries the data source controls (WO-03 M1): import
  // an observation document, and — when imported documents are shown —
  // return to the synthetic demonstration data. The file input itself lives
  // in App.svelte so every import affordance shares one input.

  import { dataSource, returnToMock } from '../lib/dataMode.svelte';
  import { SECTIONS, SECTION_META, type SectionId } from '../lib/navigation.svelte';

  let {
    active = 'overview',
    onnavigate = (_section: SectionId) => {},
    onimportclick = () => {},
  }: {
    active?: SectionId;
    onnavigate?: (section: SectionId) => void;
    onimportclick?: () => void;
  } = $props();

  // Session-only pinned state (nothing is persisted or claimed to be).
  let pinned = $state(false);
</script>

<nav class="rail" class:expanded={pinned} aria-label="Primary navigation">
  <div class="mark" aria-hidden="true">
    <svg viewBox="0 0 24 24" width="22" height="22">
      <rect x="3" y="3" width="18" height="18" rx="5" fill="none"
        stroke="var(--accent)" stroke-width="1.6" />
      <circle cx="9" cy="13" r="2.1" fill="var(--accent)" />
      <circle cx="15" cy="13" r="2.1" fill="var(--accent)" />
      <path d="M 8 8.5 Q 12 11 16 8.5" fill="none" stroke="var(--accent)"
        stroke-width="1.6" stroke-linecap="round" />
    </svg>
  </div>

  <button
    class="item pin-toggle"
    class:active={false}
    data-testid="nav-pin-toggle"
    aria-pressed={pinned}
    title={pinned ? 'Collapse the navigation labels' : 'Pin the navigation labels open'}
    aria-label={pinned ? 'Collapse the navigation labels' : 'Pin the navigation labels open'}
    onclick={() => (pinned = !pinned)}
  >
    {#if pinned}
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none"
        stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
        <path d="M19 5l-6 7 6 7" />
        <path d="M12 5l-6 7 6 7" />
      </svg>
      <span class="label">Collapse</span>
    {:else}
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none"
        stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
        <path d="M5 5l6 7-6 7" />
        <path d="M12 5l6 7-6 7" />
      </svg>
      <span class="flyout" aria-hidden="true">Pin labels open</span>
    {/if}
  </button>

  <div class="divider" aria-hidden="true"></div>

  {#each SECTIONS as section (section)}
    {@const meta = SECTION_META[section]}
    <button
      class="item"
      class:active={active === section}
      data-testid={meta.testid}
      title={meta.label}
      aria-label={meta.label}
      aria-current={active === section ? 'page' : undefined}
      onclick={() => onnavigate(section)}
    >
      {#if section === 'overview'}
        <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
          stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
          <rect x="4" y="4" width="7" height="7" rx="1.5" />
          <rect x="13" y="4" width="7" height="7" rx="1.5" />
          <rect x="4" y="13" width="7" height="7" rx="1.5" />
          <rect x="13" y="13" width="7" height="7" rx="1.5" />
        </svg>
      {:else if section === 'topology'}
        <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
          stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
          <circle cx="6" cy="18" r="2.6" />
          <circle cx="18" cy="18" r="2.6" />
          <circle cx="12" cy="6" r="2.6" />
          <path d="M10.5 8.2 7.3 15.6M13.5 8.2l3.2 7.4M8.6 18h6.8" />
        </svg>
      {:else if section === 'interfaces'}
        <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
          stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
          <rect x="6" y="8" width="12" height="9" rx="1.5" />
          <path d="M9 8V5.5M12 8V5.5M15 8V5.5" />
          <path d="M9.5 12.5h5" />
        </svg>
      {:else if section === 'routes'}
        <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
          stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
          <path d="M5 19c0-6 3.5-9 7.5-9 3 0 5.5 2 6.5 5" />
          <circle cx="5" cy="19" r="1.9" />
          <path d="M19 15l1.6 1.4L22.4 13" />
          <path d="M5 19h14" stroke-dasharray="2 3" opacity="0.45" />
        </svg>
      {:else if section === 'dns'}
        <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
          stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
          <rect x="4" y="4" width="16" height="6" rx="1.6" />
          <rect x="4" y="14" width="16" height="6" rx="1.6" />
          <path d="M7.5 7h.01M7.5 17h.01" />
          <path d="M11 7h5M11 17h5" />
        </svg>
      {:else if section === 'connections'}
        <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
          stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
          <circle cx="7" cy="12" r="3.2" />
          <circle cx="17" cy="12" r="3.2" />
          <path d="M10.2 12h3.6" />
          <path d="M5 5.5v3M19 15.5v3" opacity="0.55" />
        </svg>
      {:else if section === 'providers'}
        <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
          stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
          <path d="M12 3.5 20 8v8l-8 4.5L4 16V8z" />
          <path d="M4 8l8 4.5L20 8M12 12.5V20.5" />
        </svg>
      {:else if section === 'documents'}
        <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
          stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
          <path d="M6 3h8l4 4v14H6z" />
          <path d="M14 3v4h4" />
          <path d="M9 12h6M9 15.5h6" />
        </svg>
      {/if}
      {#if pinned}
        <span class="label">{meta.label}</span>
      {:else}
        <span class="flyout" aria-hidden="true">{meta.label}</span>
      {/if}
    </button>
  {/each}

  <div class="spacer"></div>

  <button
    class="item import"
    data-testid="nav-import"
    title="Import an observation document (araiguma snapshot / topology JSON)"
    aria-label="Import an observation document"
    onclick={onimportclick}
  >
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
      stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
      <path d="M12 15V4" />
      <path d="M8 8l4-4 4 4" />
      <path d="M4 15v3a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-3" />
    </svg>
    {#if pinned}
      <span class="label">Import document…</span>
    {:else}
      <span class="flyout" aria-hidden="true">Import document…</span>
    {/if}
  </button>
  {#if dataSource.mode === 'imported'}
    <button
      class="item"
      data-testid="nav-return-mock"
      title="Return to the synthetic demonstration data"
      aria-label="Return to the synthetic demonstration data"
      onclick={() => returnToMock()}
    >
      <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
        stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
        <path d="M4 12a8 8 0 1 1 2.3 5.6" />
        <path d="M4 18v-5h5" />
      </svg>
      {#if pinned}
        <span class="label">Return to demo data</span>
      {:else}
        <span class="flyout" aria-hidden="true">Return to demo data</span>
      {/if}
    </button>
  {/if}
</nav>

<style>
  .rail {
    width: 56px;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 5px;
    padding: 10px 0;
    background: var(--bg-panel);
    border-right: 1px solid var(--border-subtle);
    transition: width 140ms ease;
  }

  .rail.expanded {
    width: 232px;
    align-items: stretch;
    padding: 10px 10px;
  }

  .mark {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    margin-bottom: 6px;
    flex: none;
    align-self: center;
  }

  .divider {
    width: 26px;
    height: 1px;
    background: var(--border-subtle);
    margin: 3px 0 5px;
    flex: none;
    align-self: center;
  }

  .rail.expanded .divider {
    width: auto;
    align-self: stretch;
  }

  .item {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-m);
    color: var(--text-muted);
    transition: background 120ms ease, color 120ms ease;
    flex: none;
  }

  .rail.expanded .item {
    width: 100%;
    justify-content: flex-start;
    padding: 6px 10px;
    min-height: 40px;
    height: auto;
  }

  .item:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .item:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .item.active {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
  }

  .pin-toggle {
    width: 32px;
    height: 32px;
    color: var(--text-muted);
  }

  .rail.expanded .pin-toggle {
    width: 100%;
    height: 32px;
    padding: 0 10px;
  }

  .label {
    font-size: 12.5px;
    color: inherit;
    /* Long section names ("Providers & Observation Health") wrap to a
       second line instead of being truncated — the full name always
       stays readable in pinned mode. */
    white-space: normal;
    overflow-wrap: anywhere;
    line-height: 1.3;
  }

  .rail.expanded .label {
    flex: 1;
  }

  /* Collapsed mode: a hover/focus label preview flies out beside the item.
     Purely visual (aria-hidden) — the accessible name comes from the
     button's aria-label/title, which is always present. */
  .flyout {
    position: absolute;
    left: calc(100% + 12px);
    top: 50%;
    transform: translateY(-50%);
    padding: 4px 10px;
    border-radius: var(--radius-s);
    background: var(--bg-raised);
    border: 1px solid var(--border-strong);
    box-shadow: var(--shadow-panel);
    color: var(--text-primary);
    font-size: 12px;
    white-space: nowrap;
    opacity: 0;
    pointer-events: none;
    transition: opacity 90ms ease;
    z-index: 40;
  }

  .item:hover .flyout,
  .item:focus-visible .flyout {
    opacity: 1;
  }

  /* The active item never needs a label preview: it is highlighted and
     its view is on screen — the preview would only obscure content. */
  .item.active .flyout {
    display: none;
  }

  .spacer {
    flex: 1;
  }

  .import {
    color: var(--accent-blue);
    border: 1px dashed color-mix(in srgb, var(--accent-blue) 55%, transparent);
  }

  .rail.expanded .import {
    border-style: solid;
  }
</style>
