<script lang="ts">
  // Shown when the current view needs a document kind that is unavailable
  // in the active mode. The message names what is missing — never a blank
  // screen and never silently substituted mock or imported data.
  let {
    kind,
    mode,
  }: {
    kind: 'snapshot' | 'topology';
    mode: 'imported' | 'live';
  } = $props();

  const title = $derived(
    mode === 'live'
      ? kind === 'snapshot'
        ? 'No live snapshot available'
        : 'No live topology available'
      : kind === 'snapshot'
        ? 'No snapshot imported'
        : 'No topology document imported',
  );

  const body = $derived(
    mode === 'live'
      ? kind === 'snapshot'
        ? 'The live engine has not delivered a validated observation snapshot. No imported or synthetic document is substituted for live data.'
        : 'The live engine has not delivered a validated topology projection. No imported or synthetic document is substituted for live data.'
      : kind === 'snapshot'
        ? 'The Overview, Routes, and Provider status views render an observation snapshot. Import one produced by the Araiguma CLI (araiguma snapshot > snapshot.json), or return to the demonstration data.'
        : 'The Topology view renders a topology document projected from a snapshot. Produce one with the Araiguma CLI (araiguma topology --input snapshot.json > topology.json) and import it, or return to the demonstration data.',
  );
</script>

<div class="missing" data-testid="missing-{kind}-notice">
  <h1>{title}</h1>
  <p>{body}</p>
  <p class="hint">
    {#if mode === 'live'}
      Live documents are validated in full before anything is displayed.
    {:else}
      Imported documents are validated in full before anything is displayed.
    {/if}
  </p>
</div>

<style>
  .missing {
    margin: 48px auto;
    max-width: 560px;
    padding: 28px 32px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-m);
    text-align: center;
  }

  h1 {
    margin: 0 0 12px;
    font-size: 17px;
    font-weight: 600;
  }

  p {
    margin: 0 0 8px;
    color: var(--text-secondary);
    font-size: 13px;
    line-height: 1.55;
  }

  .hint {
    color: var(--text-muted);
    font-size: 11.5px;
  }
</style>
