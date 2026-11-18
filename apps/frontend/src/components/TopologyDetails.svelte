<script lang="ts">
  // Contextual details for a selected topology node. Overlays the workspace
  // (same discipline as the overview details panel).

  import type { TopologyDocument, TopologyNode } from '../types';
  import {
    edgeKindLabel,
    evidenceLabel,
    nodeKindLabel,
    nodeIdKey,
  } from '../lib/topology';
  import { formatMacObservation, kindLabel, macObservationKind } from '../lib/format';

  let {
    node,
    document,
    onclose,
  }: {
    node: TopologyNode;
    document: TopologyDocument;
    onclose: () => void;
  } = $props();

  const connectedEdges = $derived(
    document.edges.filter(
      (edge) =>
        nodeIdKey(edge.from) === nodeIdKey(node.id) ||
        nodeIdKey(edge.to) === nodeIdKey(node.id),
    ),
  );
</script>

<aside class="panel" data-testid="topology-details" aria-label="Topology node details">
  <header>
    <h2 class="mono">{node.label}</h2>
    <button class="close" onclick={onclose} aria-label="Close details">✕</button>
  </header>

  <section>
    <h3>Kind</h3>
    <p class="value">
      {nodeKindLabel(node)}{#if node.kindDetail}
        <span class="mono detail">({node.kindDetail})</span>
      {/if}
    </p>
    {#if node.nodeKind === 'interface' && node.interfaceKind}
      <p class="value dim">{kindLabel(node.interfaceKind)}</p>
    {/if}
  </section>

  {#if node.nodeKind === 'interface'}
    <section>
      <h3>State</h3>
      <p class="value">
        {node.state}
        <span class="dim">
          · admin {node.detail.adminState ?? '—'} · oper {node.detail.operState ?? '—'}
        </span>
      </p>
    </section>

    <section>
      <h3>Identity</h3>
      <dl>
        <dt>ifindex</dt>
        <dd class="mono">{node.detail.ifindex ?? '—'}</dd>
        <dt>MAC</dt>
        <dd
          class="mono"
          class:dim={macObservationKind(node.detail.macAddress) !== 'present'}
          title={
            macObservationKind(node.detail.macAddress) === 'absent'
              ? 'observed: this interface has no hardware address'
              : macObservationKind(node.detail.macAddress) === 'notObserved'
                ? 'not observed'
                : undefined
          }
        >
          {formatMacObservation(node.detail.macAddress)}
        </dd>
        <dt>MTU</dt>
        <dd class="mono">{node.detail.mtu ?? '—'}</dd>
        <dt>veth peer</dt>
        <dd class="mono" title={node.detail.peerIfindex !== null
          ? 'the kernel named this veth peer; see the veth peer relation below'
          : undefined}
        >
          {node.detail.peerIfindex ?? '—'}
        </dd>
        <dt>Routes via</dt>
        <dd class="mono">{node.detail.routesUsing} ({node.detail.defaultRoutes} default)</dd>
        <dt>Observed by</dt>
        <dd>{node.detail.sources.join(', ') || '—'}</dd>
      </dl>
    </section>

    <section>
      <h3>Addresses ({node.detail.addresses.length})</h3>
      {#if node.detail.addresses.length === 0}
        <p class="empty">no addresses observed</p>
      {:else}
        <table>
          <thead><tr><th>Address</th><th>Scope</th></tr></thead>
          <tbody>
            {#each node.detail.addresses as address (address.address + '/' + address.prefixLen)}
              <tr>
                <td class="mono">{address.address}/{address.prefixLen}</td>
                <td>{address.scope ?? '—'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </section>
  {:else}
    <section>
      <h3>What this node is</h3>
      <p class="value">
        A next-hop address named by an observed routing-table entry. It is a
        routing fact — <strong>not</strong> a claim that the host is reachable
        or connected to the internet.
      </p>
    </section>
  {/if}

  {#if connectedEdges.length > 0}
    <section>
      <h3>Relations ({connectedEdges.length})</h3>
      <ul class="relations">
        {#each connectedEdges as edge, index (index)}
          <li>
            <span class="edge-kind">{edgeKindLabel(edge.edgeKind)}</span>
            <span class="mono path">
              {#if nodeIdKey(edge.from) === nodeIdKey(node.id)}→{:else}←{/if}
              {#if nodeIdKey(edge.from) === nodeIdKey(node.id)}
                {document.nodes.find((n) => nodeIdKey(n.id) === nodeIdKey(edge.to))?.label ?? '?'}
              {:else}
                {document.nodes.find((n) => nodeIdKey(n.id) === nodeIdKey(edge.from))?.label ?? '?'}
              {/if}
            </span>
            <span class="evidence">{evidenceLabel(edge.evidence)}</span>
            {#if edge.detail}<span class="edge-detail">{edge.detail}</span>{/if}
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if node.annotations.length > 0 || node.detail.notes.length > 0}
    <section>
      <h3>Observation notes</h3>
      <ul class="notes">
        {#each [...node.annotations, ...node.detail.notes] as note, index (index)}
          <li>{note}</li>
        {/each}
      </ul>
    </section>
  {/if}
</aside>

<style>
  .panel {
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

  h3 {
    margin: 18px 0 8px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--text-secondary);
  }

  .close {
    color: var(--text-muted);
    width: 28px;
    height: 28px;
    border-radius: var(--radius-s);
  }

  .close:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .value {
    margin: 0;
    font-size: 13px;
  }

  .dim {
    color: var(--text-secondary);
    font-size: 12px;
  }

  .detail {
    color: var(--text-muted);
    font-size: 11px;
  }

  dl {
    display: grid;
    grid-template-columns: 88px 1fr;
    row-gap: 6px;
    margin: 0;
  }

  dt {
    color: var(--text-muted);
    font-size: 12px;
  }

  dd {
    margin: 0;
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
  }

  th {
    text-align: left;
    color: var(--text-muted);
    font-weight: 500;
    font-size: 11px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border-subtle);
  }

  td {
    padding: 5px 6px 5px 0;
    border-bottom: 1px solid color-mix(in srgb, var(--border-subtle) 55%, transparent);
  }

  .empty {
    color: var(--text-muted);
    font-size: 12.5px;
    margin: 0;
  }

  .relations {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .relations li {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12px;
  }

  .edge-kind {
    color: var(--accent-blue);
    font-weight: 600;
  }

  .path {
    color: var(--text-primary);
  }

  .evidence {
    color: var(--text-secondary);
  }

  .edge-detail {
    color: var(--text-muted);
    font-size: 11px;
  }

  .notes {
    color: var(--state-unknown);
    font-size: 12px;
    padding-left: 16px;
    margin: 0;
  }

  .notes li {
    margin-bottom: 4px;
  }
</style>
