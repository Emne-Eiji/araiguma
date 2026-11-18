<script lang="ts">
  import type { Interface, Route, TopologyNode } from './types';
  import { fixtures } from './lib/fixtures';
  import { defaultTopology, routeHighlightTargets } from './lib/topology';
  import NavRail from './components/NavRail.svelte';
  import OverviewView from './components/OverviewView.svelte';
  import TopologyView from './components/TopologyView.svelte';
  import TopologyDetails from './components/TopologyDetails.svelte';
  import RoutesView from './components/RoutesView.svelte';
  import RouteDetails from './components/RouteDetails.svelte';
  import ProvidersView from './components/ProvidersView.svelte';
  import DetailsPanel from './components/DetailsPanel.svelte';

  // WO-01R foundation + WO-02 views. The app renders explicitly-labeled
  // mock fixtures; live data will arrive as the same documents the CLI
  // emits. The MOCK DATA badge stays visible whenever a mock is shown.

  type View = 'overview' | 'topology' | 'routes' | 'providers';

  const snapshot = $derived(fixtures.workstation.snapshot);
  const topologyDocument = $derived(defaultTopology());

  let view = $state<View>('overview');
  let selectedInterface: Interface | null = $state(null);
  let selectedNode: TopologyNode | null = $state(null);
  let selectedRoute: Route | null = $state(null);

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
  }
</script>

<div class="app">
  <NavRail active={view} onnavigate={navigate} />
  <main class="workspace" data-testid="workspace">
    {#if view === 'overview'}
      <OverviewView
        {snapshot}
        onselect={(iface) => (selectedInterface = iface)}
      />
    {:else if view === 'topology'}
      <TopologyView
        document={topologyDocument}
        highlight={routeHighlight}
        onselect={(node) => (selectedNode = node)}
      />
    {:else if view === 'routes'}
      <RoutesView
        {snapshot}
        {selectedRoute}
        onselect={(route) => (selectedRoute = route)}
      />
    {:else}
      <ProvidersView {snapshot} />
    {/if}
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
  }
</style>
