<script lang="ts">
  // Narrow icon-led navigation rail. 56px wide, expandable later; today it
  // carries the views that exist (Overview) — no speculative placeholders.

  let {
    active = 'overview',
    onnavigate = (_view: string) => {},
  }: {
    active?: string;
    onnavigate?: (view: string) => void;
  } = $props();
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

  <button
    class="item"
    class:active={active === 'overview'}
    data-testid="nav-overview"
    title="Overview"
    aria-label="Overview"
    aria-current={active === 'overview' ? 'page' : undefined}
    onclick={() => onnavigate('overview')}
  >
    <svg viewBox="0 0 24 24" width="20" height="20" fill="none"
      stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
      <rect x="4" y="4" width="7" height="7" rx="1.5" />
      <rect x="13" y="4" width="7" height="7" rx="1.5" />
      <rect x="4" y="13" width="7" height="7" rx="1.5" />
      <rect x="13" y="13" width="7" height="7" rx="1.5" />
    </svg>
  </button>
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

  .item.active {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    color: var(--accent);
  }
</style>
