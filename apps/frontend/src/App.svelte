<script lang="ts">
  import type { Interface } from './types';
  import { fixtures } from './lib/fixtures';
  import NavRail from './components/NavRail.svelte';
  import OverviewView from './components/OverviewView.svelte';
  import DetailsPanel from './components/DetailsPanel.svelte';

  // WO-01 foundation: the app renders explicitly-labeled mock fixtures.
  // Live data arrives later through the same NetworkSnapshot shape (CLI
  // output loading); the MOCK DATA badge stays whenever a mock is shown.
  const snapshot = $derived(fixtures.workstation.snapshot);

  let selected: Interface | null = $state(null);

  function handleSelect(iface: Interface) {
    selected = iface;
  }
</script>

<div class="app">
  <NavRail active="overview" onnavigate={() => {}} />
  <main class="workspace" data-testid="workspace">
    <OverviewView {snapshot} onselect={handleSelect} />
  </main>
  {#if selected}
    <DetailsPanel iface={selected} {snapshot} onclose={() => (selected = null)} />
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
