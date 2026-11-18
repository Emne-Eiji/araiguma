<script lang="ts">
  // Narrow icon-led navigation rail. 56px wide. The bottom section carries
  // the data source controls (WO-03 M1): import an observation document,
  // and — when imported documents are shown — return to the synthetic
  // demonstration data.

  import { dataSource, importFile, returnToMock } from '../lib/dataMode.svelte';

  let {
    active = 'overview',
    onnavigate = (_view: string) => {},
    onimporterror = (_message: string) => {},
  }: {
    active?: string;
    onnavigate?: (view: string) => void;
    onimporterror?: (message: string) => void;
  } = $props();

  const items = [
    { id: 'overview', label: 'Overview', testid: 'nav-overview' },
    { id: 'topology', label: 'Topology', testid: 'nav-topology' },
    { id: 'routes', label: 'Routes', testid: 'nav-routes' },
    { id: 'providers', label: 'Provider status', testid: 'nav-providers' },
  ];

  let fileInput = $state<HTMLInputElement>();

  async function onFileChosen(event: Event) {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    // Reset the value so choosing the same file again re-triggers.
    input.value = '';
    if (!file) return;
    try {
      const text = await file.text();
      importFile(file.name, text);
    } catch (error) {
      onimporterror(`${file.name}: ${(error as Error).message}`);
    }
  }
</script>

<nav class="rail" aria-label="Primary navigation">
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

  {#each items as item (item.id)}
    <button
      class="item"
      class:active={active === item.id}
      data-testid={item.testid}
      title={item.label}
      aria-label={item.label}
      aria-current={active === item.id ? 'page' : undefined}
      onclick={() => onnavigate(item.id)}
    >
      {#if item.id === 'overview'}
        <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
          stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
          <rect x="4" y="4" width="7" height="7" rx="1.5" />
          <rect x="13" y="4" width="7" height="7" rx="1.5" />
          <rect x="4" y="13" width="7" height="7" rx="1.5" />
          <rect x="13" y="13" width="7" height="7" rx="1.5" />
        </svg>
      {:else if item.id === 'topology'}
        <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
          stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
          <circle cx="6" cy="18" r="2.6" />
          <circle cx="18" cy="18" r="2.6" />
          <circle cx="12" cy="6" r="2.6" />
          <path d="M10.5 8.2 7.3 15.6M13.5 8.2l3.2 7.4M8.6 18h6.8" />
        </svg>
      {:else if item.id === 'routes'}
        <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
          stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
          <path d="M5 19c0-6 3.5-9 7.5-9 3 0 5.5 2 6.5 5" />
          <circle cx="5" cy="19" r="1.9" />
          <path d="M19 15l1.6 1.4L22.4 13" />
          <path d="M5 19h14" stroke-dasharray="2 3" opacity="0.45" />
        </svg>
      {:else if item.id === 'providers'}
        <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
          stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
          <path d="M12 3.5 20 8v8l-8 4.5L4 16V8z" />
          <path d="M4 8l8 4.5L20 8M12 12.5V20.5" />
        </svg>
      {/if}
    </button>
  {/each}

  <div class="spacer"></div>

  <input
    type="file"
    accept=".json,application/json"
    class="file-input"
    data-testid="import-file-input"
    bind:this={fileInput}
    onchange={onFileChosen}
    aria-hidden="true"
    tabindex="-1"
  />
  <button
    class="item import"
    data-testid="nav-import"
    title="Import an observation document (araiguma snapshot / topology JSON)"
    aria-label="Import an observation document"
    onclick={() => fileInput?.click()}
  >
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
      stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
      <path d="M12 15V4" />
      <path d="M8 8l4-4 4 4" />
      <path d="M4 15v3a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-3" />
    </svg>
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
    gap: 6px;
    padding: 10px 0;
    background: var(--bg-panel);
    border-right: 1px solid var(--border-subtle);
  }

  .mark {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    margin-bottom: 10px;
  }

  .item {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-m);
    color: var(--text-muted);
    transition: background 120ms ease, color 120ms ease;
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

  .spacer {
    flex: 1;
  }

  .import {
    color: var(--accent-blue);
    border: 1px dashed color-mix(in srgb, var(--accent-blue) 55%, transparent);
  }

  .file-input {
    display: none;
  }
</style>
