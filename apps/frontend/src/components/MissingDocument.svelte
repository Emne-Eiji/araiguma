<script lang="ts">
  // Shown when the current view needs a document kind that has not been
  // imported (e.g. a topology view with only a snapshot imported). The
  // message names what is missing and how to produce it — never a blank
  // screen and never silently substituted mock data.
  let { kind }: { kind: 'snapshot' | 'topology' } = $props();
</script>

<div class="missing" data-testid="missing-{kind}-notice">
  <h1>{kind === 'snapshot' ? 'No snapshot imported' : 'No topology document imported'}</h1>
  <p>
    {kind === 'snapshot'
      ? 'The Overview, Routes, and Provider status views render an observation snapshot. Import one produced by the Araiguma CLI (araiguma snapshot > snapshot.json), or return to the demonstration data.'
      : 'The Topology view renders a topology document projected from a snapshot. Produce one with the Araiguma CLI (araiguma topology --input snapshot.json > topology.json) and import it, or return to the demonstration data.'}
  </p>
  <p class="hint">
    Imported documents are validated in full before anything is displayed.
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
