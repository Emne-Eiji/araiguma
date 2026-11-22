<script lang="ts">
  // Application shell (WO-04 Stage A): six-section navigation (rail) +
  // central workspace + reusable right details panel + compact bottom
  // status area. WO-01R foundation + WO-02 views + WO-03 M1 document
  // import, restructured without changing any data-fidelity behavior: the
  // app renders explicitly-labeled mock scenarios, imported observation
  // documents, or a live engine pair; the status bar says which and never
  // presents recorded or synthetic data as live.
  //
  // Tray-compatible destinations (foundation only): the navigation store
  // exposes stable section ids and selected-object references; this shell
  // resolves them against the CURRENT documents and opens the shared
  // panel — a reference that does not resolve opens the panel's explicit
  // empty state, never stale details. No event bus, no notification
  // framework.

  import type { Interface, Route, TopologyNode } from './types';
  import { routeHighlightTargets, nodeIdKey } from './lib/topology';
  import { reResolveSelections } from './lib/selection';
  import { findRouteByRef } from './lib/routes';
  import { formatIpNet } from './lib/format';
  import {
    dataSource,
    importFile,
    currentSnapshot,
    currentTopology,
  } from './lib/dataMode.svelte';
  import {
    navigation,
    navigate,
    navigateTo,
    isSectionId,
    type SectionId,
    type SelectionRef,
  } from './lib/navigation.svelte';
  import { onDestroy, onMount, untrack } from 'svelte';
  import NavRail from './components/NavRail.svelte';
  import OverviewView from './components/OverviewView.svelte';
  import TopologyView from './components/TopologyView.svelte';
  import TopologyDetails from './components/TopologyDetails.svelte';
  import InterfacesView from './components/InterfacesView.svelte';
  import RoutesView from './components/RoutesView.svelte';
  import RouteDetails from './components/RouteDetails.svelte';
  import ProvidersView from './components/ProvidersView.svelte';
  import DocumentsView from './components/DocumentsView.svelte';
  import DetailsPanel from './components/DetailsPanel.svelte';
  import AppSidePanel from './components/AppSidePanel.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import MissingDocument from './components/MissingDocument.svelte';
  import {
    initLiveMode,
    teardownLiveMode,
    type LiveModeTeardown,
  } from './lib/liveMode';

  const section = $derived(navigation.section);
  const snapshot = $derived(currentSnapshot());
  const topologyDocument = $derived(currentTopology());

  let selectedInterface: Interface | null = $state(null);
  let selectedNode: TopologyNode | null = $state(null);
  let selectedRoute: Route | null = $state(null);
  let importError = $state<string | null>(null);
  // Set when a navigation destination names a selection that is NOT part of
  // the current documents: the panel opens in its explicit empty state.
  let panelEmpty = $state(false);

  // Desktop live mode is optional: the same shell runs unchanged in the
  // browser with its explicit mock/imported modes. The local teardown
  // handles the race where the component is destroyed while initialization
  // is still awaiting its first IPC response; the module-level teardown
  // covers an already-active session.
  let liveTeardown: LiveModeTeardown | null = null;
  let liveDisposed = false;

  onMount(() => {
    liveDisposed = false;
    void initLiveMode().then((teardown) => {
      if (liveDisposed) {
        void teardown();
      } else {
        liveTeardown = teardown;
      }
    });
  });

  onDestroy(() => {
    liveDisposed = true;
    void liveTeardown?.();
    liveTeardown = null;
    void teardownLiveMode();
  });

  // Linkage: the nodes a selected route refers to, highlighted when the
  // topology view shows it (highlight only — nothing is modified). With the
  // destination API this is reachable: "Show in Topology" keeps the route
  // selected across the section switch.
  const routeHighlight = $derived(
    section === 'topology' && selectedRoute && topologyDocument
      ? routeHighlightTargets(selectedRoute, topologyDocument)
      : [],
  );

  // ── Destinations ─────────────────────────────────────────────────────────
  // Every navigation lands here. Section switches clear stale selections
  // and overlays (the established discipline); a destination with a
  // selection resolves it by identity against the CURRENT documents.
  $effect(() => {
    const destination = navigation.destination;
    if (destination === null) return;
    untrack(() => {
      selectedInterface = null;
      selectedNode = null;
      selectedRoute = null;
      panelEmpty = false;
      importError = null;
      if (!destination.selection) return;
      resolveSelection(destination.selection);
    });
  });

  function resolveSelection(ref: SelectionRef): void {
    const current = currentSnapshot();
    const currentTopologyDocument = currentTopology();
    if (ref.kind === 'interface') {
      selectedInterface =
        current?.interfaces.find((iface) => iface.ifindex === ref.ifindex) ?? null;
      panelEmpty = selectedInterface === null;
    } else if (ref.kind === 'node') {
      selectedNode =
        currentTopologyDocument?.nodes.find(
          (node) => nodeIdKey(node.id) === ref.nodeKey,
        ) ?? null;
      panelEmpty = selectedNode === null;
    } else {
      selectedRoute = current ? findRouteByRef(current.routes, ref) : null;
      panelEmpty = selectedRoute === null;
    }
  }

  // WO-03-F / AUD-09: when the active documents change (import, return to
  // mock, scenario switch), every selection re-resolves against the NEW
  // documents by identity — a selection that no longer resolves is cleared,
  // never kept rendering as if it belonged to the current observation. The
  // effect tracks only the generation counter; the selection reads are
  // untracked so re-resolving cannot retrigger it.
  $effect(() => {
    const generation = dataSource.generation;
    untrack(() => {
      if (generation === 0) return; // initial mock documents: nothing selected yet
      const resolved = reResolveSelections(
        {
          interface: selectedInterface,
          node: selectedNode,
          route: selectedRoute,
        },
        currentSnapshot(),
        currentTopology(),
      );
      selectedInterface = resolved.interface;
      selectedNode = resolved.node;
      selectedRoute = resolved.route;
    });
  });

  // A view needs its document. Imported mode can lack one kind when only
  // the other was imported; live mode can lack either kind until the
  // engine delivers a coherent pair. Mock mode always has the active
  // scenario's fixture pair.
  const snapshotMissing = $derived(dataSource.mode !== 'mock' && snapshot === null);
  const topologyMissing = $derived(
    dataSource.mode !== 'mock' && topologyDocument === null,
  );
  const missingMode = $derived(dataSource.mode === 'live' ? 'live' : 'imported');

  // ── The shared details panel ─────────────────────────────────────────────
  const panelOpen = $derived(
    selectedInterface !== null || selectedNode !== null || selectedRoute !== null || panelEmpty,
  );

  const panelConfig = $derived.by(() => {
    if (selectedInterface !== null) {
      return {
        testid: 'details-panel',
        ariaLabel: `Interface details for ${selectedInterface.name}`,
        title: selectedInterface.name,
        closeLabel: 'Close details',
        emptyBody: '',
      };
    }
    if (selectedNode !== null) {
      return {
        testid: 'topology-details',
        ariaLabel: 'Topology node details',
        title: selectedNode.label,
        closeLabel: 'Close details',
        emptyBody: '',
      };
    }
    if (selectedRoute !== null) {
      return {
        testid: 'route-details',
        ariaLabel: 'Route details',
        title: `${formatIpNet(selectedRoute.destination)} · ${selectedRoute.family}`,
        closeLabel: 'Close route details',
        emptyBody: '',
      };
    }
    return {
      testid: 'app-side-panel',
      ariaLabel: 'Details',
      title: 'Details',
      closeLabel: 'Close details',
      emptyBody:
        'The requested item is not part of the current observation documents — its details cannot be shown. Nothing stale is rendered.',
    };
  });

  function closePanel(): void {
    selectedInterface = null;
    selectedNode = null;
    selectedRoute = null;
    panelEmpty = false;
  }

  // ── Import (one shared file input; the rail and the Documents view both
  //    open it — the input's test id stays unique) ─────────────────────────
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
      importError = `${file.name}: ${(error as Error).message}`;
    }
  }

  function openImport(): void {
    fileInput?.click();
  }

  function goTo(next: SectionId): void {
    navigateTo(next);
  }

  // Test/inspection hook (same gating discipline as TopologyView): exposed
  // ONLY in development or with ?araiguma-test-hooks, so the e2e suite can
  // drive the destination API exactly as a future tray would. Inputs are
  // validated at the boundary; the hook is removed when the app unmounts.
  const TEST_HOOK_PARAM = 'araiguma-test-hooks';
  const testHooksEnabled =
    import.meta.env.DEV ||
    (typeof window !== 'undefined' &&
      new URLSearchParams(window.location.search).has(TEST_HOOK_PARAM));

  function coerceSelection(raw: unknown): SelectionRef | undefined {
    if (typeof raw !== 'object' || raw === null) return undefined;
    const ref = raw as Record<string, unknown>;
    if (ref.kind === 'interface' && typeof ref.ifindex === 'number') {
      return { kind: 'interface', ifindex: ref.ifindex };
    }
    if (ref.kind === 'node' && typeof ref.nodeKey === 'string') {
      return { kind: 'node', nodeKey: ref.nodeKey };
    }
    if (
      ref.kind === 'route' &&
      (ref.family === 'ipv4' || ref.family === 'ipv6') &&
      (ref.destination === null ||
        (typeof ref.destination === 'object' &&
          ref.destination !== null &&
          typeof (ref.destination as Record<string, unknown>).address === 'string' &&
          typeof (ref.destination as Record<string, unknown>).prefixLen === 'number')) &&
      (ref.gateway === null || typeof ref.gateway === 'string') &&
      (ref.metric === null || typeof ref.metric === 'number') &&
      (ref.tos === null || typeof ref.tos === 'number')
    ) {
      return {
        kind: 'route',
        family: ref.family,
        destination:
          ref.destination === null
            ? null
            : {
                address: (ref.destination as { address: string }).address,
                prefixLen: (ref.destination as { prefixLen: number }).prefixLen,
              },
        gateway: ref.gateway as string | null,
        metric: ref.metric as number | null,
        tos: ref.tos as number | null,
      };
    }
    return undefined;
  }

  $effect(() => {
    if (!testHooksEnabled) return;
    (window as unknown as Record<string, unknown>).__araigumaNavigation = {
      navigateTo: (section: string) => {
        if (isSectionId(section)) navigateTo(section);
      },
      navigate: (destination: unknown) => {
        if (typeof destination !== 'object' || destination === null) return;
        const { section, selection } = destination as Record<string, unknown>;
        if (typeof section !== 'string' || !isSectionId(section)) return;
        navigateTo(section as SectionId);
        const coerced = coerceSelection(selection);
        if (coerced) navigate({ section: section as SectionId, selection: coerced });
      },
      currentSection: () => navigation.section,
    };
    return () => {
      delete (window as unknown as Record<string, unknown>).__araigumaNavigation;
    };
  });
</script>

<div class="app">
  <div class="app-body">
    <NavRail active={section} onnavigate={goTo} onimportclick={openImport} />
    <main class="workspace" data-testid="workspace">
      {#if importError}
        <div class="import-error" data-testid="import-error" role="alert">
          <span>Import rejected — {importError}</span>
          <button onclick={() => (importError = null)} aria-label="Dismiss import error">✕</button>
        </div>
      {/if}

      <div class="view-slot">
        {#if section === 'overview'}
          <OverviewView />
        {:else if section === 'topology'}
          {#if topologyMissing}
            <MissingDocument kind="topology" mode={missingMode} />
          {:else if topologyDocument}
            <TopologyView
              document={topologyDocument}
              highlight={routeHighlight}
              onselect={(node) => (selectedNode = node)}
            />
          {/if}
        {:else if section === 'interfaces'}
          {#if snapshotMissing}
            <MissingDocument kind="snapshot" mode={missingMode} />
          {:else if snapshot}
            <InterfacesView {snapshot} onselect={(iface) => (selectedInterface = iface)} />
          {/if}
        {:else if section === 'routes'}
          {#if snapshotMissing}
            <MissingDocument kind="snapshot" mode={missingMode} />
          {:else if snapshot}
            <RoutesView
              {snapshot}
              {selectedRoute}
              onselect={(route) => (selectedRoute = route)}
            />
          {/if}
        {:else if section === 'providers'}
          {#if snapshotMissing}
            <MissingDocument kind="snapshot" mode={missingMode} />
          {:else if snapshot}
            <ProvidersView {snapshot} />
          {/if}
        {:else}
          <DocumentsView onimportclick={openImport} />
        {/if}
      </div>
    </main>

    <AppSidePanel
      open={panelOpen}
      empty={panelEmpty}
      testid={panelConfig.testid}
      ariaLabel={panelConfig.ariaLabel}
      title={panelConfig.title}
      closeLabel={panelConfig.closeLabel}
      emptyBody={panelConfig.emptyBody}
      onclose={closePanel}
    >
      {#snippet children()}
        {#if selectedInterface && snapshot}
          <DetailsPanel iface={selectedInterface} {snapshot} />
        {:else if selectedNode && topologyDocument}
          <TopologyDetails node={selectedNode} document={topologyDocument} />
        {:else if selectedRoute}
          <RouteDetails route={selectedRoute} />
        {/if}
      {/snippet}
    </AppSidePanel>
  </div>

  <StatusBar />

  <input
    type="file"
    accept=".json,application/json"
    class="app-file-input"
    data-testid="import-file-input"
    bind:this={fileInput}
    onchange={onFileChosen}
    aria-hidden="true"
    tabindex="-1"
  />
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .app-body {
    flex: 1;
    min-height: 0;
    display: flex;
    position: relative;
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

  .app-file-input {
    display: none;
  }
</style>
