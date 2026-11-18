<script lang="ts">
  // Persistent, explicit marker that the current data is synthetic.
  // Showing mock data without this badge would misrepresent fixture data
  // as live observation — it must stay visible whenever a mock is shown.
</script>

<span
  class="mock-badge"
  data-testid="mock-badge"
  role="status"
  title="This view shows synthetic fixture data — not a live observation of this machine"
>
  MOCK DATA
</span>

<style>
  .mock-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px;
    border: 1px solid color-mix(in srgb, var(--state-unknown) 65%, transparent);
    border-radius: 999px;
    color: var(--state-unknown);
    background: color-mix(in srgb, var(--state-unknown) 12%, transparent);
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 0.08em;
    white-space: nowrap;
  }

  .mock-badge::before {
    content: '';
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--state-unknown);
  }
</style>
