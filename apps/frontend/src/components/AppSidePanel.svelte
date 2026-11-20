<script lang="ts">
  // Reusable application-level right details panel (WO-04 Stage A).
  //
  // ONE panel chrome for every details surface: hidden unless `open`
  // (selections open it explicitly), close is explicit (button and Escape),
  // focus moves into the panel when it opens and back to the trigger when
  // it closes, and an explicit empty/unselected state exists for
  // destinations whose target is not part of the current documents (the
  // honest answer — never stale details). The caller provides the content
  // and the identity labels; this component owns only the shell.

  import type { Snippet } from 'svelte';

  let {
    testid,
    ariaLabel,
    title,
    closeLabel = 'Close details',
    onclose,
    open = false,
    empty = false,
    emptyBody = 'Nothing selected. Choose an item in the view to see its details.',
    children,
  }: {
    testid: string;
    ariaLabel: string;
    title: string;
    closeLabel?: string;
    onclose: () => void;
    open?: boolean;
    empty?: boolean;
    emptyBody?: string;
    children?: Snippet;
  } = $props();

  let panelEl = $state<HTMLElement>();
  let restoreFocusTo: HTMLElement | null = null;

  // Focus handling: on open, remember the trigger and focus the panel so
  // keyboard users land inside it; on close, return focus to the trigger
  // when it is still in the document. While the panel stays open and the
  // selection changes, focus is left where the user put it.
  $effect(() => {
    if (open) {
      const active = document.activeElement;
      if (active instanceof HTMLElement && active !== panelEl) {
        restoreFocusTo = active;
      }
      panelEl?.focus();
    } else if (restoreFocusTo) {
      const target = restoreFocusTo;
      restoreFocusTo = null;
      if (target.isConnected) target.focus();
    }
  });

  function onKeydown(event: KeyboardEvent) {
    if (!open) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      onclose();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
  <aside
    class="side-panel"
    bind:this={panelEl}
    data-testid={testid}
    aria-label={ariaLabel}
    tabindex="-1"
  >
    <header>
      <h2 class="mono">{title}</h2>
      <button class="close" onclick={onclose} aria-label={closeLabel}>✕</button>
    </header>
    {#if empty}
      <div class="empty-state" data-testid="panel-empty">
        <p>{emptyBody}</p>
      </div>
    {:else}
      {@render children?.()}
    {/if}
  </aside>
{/if}

<style>
  /* The panel overlays the workspace; it never shrinks it (the established
     discipline — the table/graph keeps its width). */
  .side-panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(360px, 90vw);
    overflow-y: auto;
    background: var(--bg-panel);
    border-left: 1px solid var(--border-strong);
    box-shadow: var(--shadow-panel);
    padding: 16px 18px 24px;
    z-index: 20;
  }

  .side-panel:focus-visible {
    outline: none;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 4px;
  }

  h2 {
    margin: 0;
    font-size: 17px;
    overflow-wrap: anywhere;
  }

  .close {
    color: var(--text-muted);
    width: 28px;
    height: 28px;
    border-radius: var(--radius-s);
    flex: none;
    margin-left: 8px;
  }

  .close:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .empty-state {
    margin-top: 24px;
    padding: 20px 16px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-m);
    text-align: center;
    color: var(--text-secondary);
    font-size: 13px;
  }

  .empty-state p {
    margin: 0;
    line-height: 1.55;
  }
</style>
