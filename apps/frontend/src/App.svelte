<script lang="ts">
  import type { Interface, Route, TopologyNode } from './types';
  import { routeHighlightTargets } from './lib/topology';
  import {
    dataSource,
    currentSnapshot,
    currentTopology,
    snapshotImported,
    topologyImported,
  } from './lib/dataMode.svelte';
  import { formatObservationTime } from './lib/import';
  import NavRail from './components/NavRail.svelte';
  import OverviewView from './components/OverviewView.svelte';
  import TopologyView from './components/TopologyView.svelte';
  import TopologyDetails from './components/TopologyDetails.svelte';
  import RoutesView from './components/RoutesView.svelte';
  import RouteDetails from './components/RouteDetails.svelte';
  import ProvidersView from './components/ProvidersView.svelte';
  import DetailsPanel from './components/DetailsPanel.svelte';
  import MissingDocument from './components/MissingDocument.svelte';

  // WO-01R foundation + WO-02 views + WO-03 M1 document import. The app
  // renders either explicitly-labeled mock fixtures or imported
  // observation documents (never presented as a live connection); the
  // badge in every view header says which.

  type View = 'overview' | 'topology' | 'routes' | 'providers';

  const snapshot = $derived(currentSnapshot());
  const topologyDocument = $derived(currentTopology());

  let view = $state<View>('overview');
  let selectedInterface: Interface | null = $state(null);
  let selectedNode: TopologyNode | null = $state(null);
  let selectedRoute: Route | null = $state(null);
  let importError = $state<string | null>(null);

  // Linkage: the nodes a selected route refers to, highlighted when the
  // topology view is opened (highlight only — nothing is modified).
  const routeHighlight = $derived(
    view === 'topology' && selectedRoute
      ? routeHighlightTargets(selectedRoute, topologyDocument)
      : [],
  );

  function navigate(next: string) {
    view = next as View;
    // Panels belong to their views; switching closes stale overlays.
    selectedInterface = null;
    selectedNode = null;
    importError = null;
  }

  // A view needs its document; in imported mode one may be missing when
  // only the other kind was imported. Mock mode always has both.
  const snapshotMissing = $derived(dataSource.mode === 'imported' && !snapshotImported());
  const topologyMissing = $derived(dataSource.mode === 'imported' && !topologyImported());

  const provenance = $derived.by(() => {
    if (dataSource.mode === 'mock') {
      return 'Synthetic demonstration data (labeled fixtures) — not an observation of this machine';
    }
    const parts: string[] = [];
    if (dataSource.snapshot) {
      parts.push(
        `snapshot “${dataSource.snapshot.fileName}” observed ${formatObservationTime(dataSource.snapshot.document.timestampMs)}`,
      );
    } else {
      parts.push('no snapshot document imported (the affected views explain what is missing)');
    }
    if (dataSource.topology) {
      parts.push(
        `topology “${dataSource.topology.fileName}” from an observation of ${formatObservationTime(dataSource.topology.document.sourceTimestampMs)}`,
      );
    } else {
      parts.push('no topology document imported (the topology view explains what is missing)');
    }
    return `Imported observation documents — ${parts.join(' · ')}. A recorded observation, not a live connection.`;
  });
</script>

<div class="app">
  <NavRail active={view} onnavigate={navigate} onimporterror={(m) => (importError = m)} />
  <main class="workspace" data-testid="workspace">
    {#if importError}
      <div class="import-error" data-testid="import-error" role="alert">
        <span>Import rejected — {importError}</span>
        <button onclick={() => (importError = null)} aria-label="Dismiss import error">✕</button>
      </div>
    {/if}

    <div class="view-slot">
      {#if view === 'overview'}
        {#if snapshotMissing}
          <MissingDocument kind="snapshot" />
        {:else}
          <OverviewView
            {snapshot}
            onselect={(iface) => (selectedInterface = iface)}
          />
        {/if}
      {:else if view === 'topology'}
        {#if topologyMissing}
          <MissingDocument kind="topology" />
        {:else}
          <TopologyView
            document={topologyDocument}
            highlight={routeHighlight}
            onselect={(node) => (selectedNode = node)}
          />
        {/if}
      {:else if view === 'routes'}
        {#if snapshotMissing}
          <MissingDocument kind="snapshot" />
        {:else}
          <RoutesView
            {snapshot}
            {selectedRoute}
            onselect={(route) => (selectedRoute = route)}
          />
        {/if}
      {:else}
        {#if snapshotMissing}
          <MissingDocument kind="snapshot" />
        {:else}
          <ProvidersView {snapshot} />
        {/if}
      {/if}
    </div>

    <p class="provenance" data-testid="data-provenance">{provenance}</p>
  </main>

  {#if view === 'overview' && selectedInterface}
    <DetailsPanel
      iface={selectedInterface}
      {snapshot}
      onclose={() => (selectedInterface = null)}
    />
  {/if}
  {#if view === 'topology' && selectedNode}
    <TopologyDetails
      node={selectedNode}
      document={topologyDocument}
      onclose={() => (selectedNode = null)}
    />
  {/if}
  {#if view === 'routes' && selectedRoute}
    <RouteDetails
      route={selectedRoute}
      onclose={() => (selectedRoute = null)}
    />
  {/if}
</div>

<style>
  .app {
    display: flex;
    height: 100%;
    overflow: hidden;
  }

  .workspace {
    flex: 1;
    min-width: 0;
    position: relative;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  .view-slot {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .import-error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin: 12px 16px 0;
    padding: 8px 12px;
    border: 1px solid color-mix(in srgb, var(--state-down) 55%, transparent);
    border-radius: var(--radius-m);
    background: color-mix(in srgb, var(--state-down) 10%, transparent);
    color: var(--state-down);
    font-size: 12.5px;
  }

  .import-error button {
    color: var(--state-down);
    width: 24px;
    height: 24px;
    border-radius: var(--radius-s);
    flex: none;
  }

  .import-error button:hover {
    background: color-mix(in srgb, var(--state-down) 15%, transparent);
  }

  .provenance {
    margin: 0;
    padding: 8px 26px 12px;
    color: var(--text-muted);
    font-size: 11px;
  }
</style>
