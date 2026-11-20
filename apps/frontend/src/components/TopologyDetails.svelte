<script lang="ts">
  // Topology-node details content (rendered inside the shared
  // AppSidePanel, WO-04 Stage A). Everything shown comes from the projected
  // document — relations only where evidence exists.

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
  }: {
    node: TopologyNode;
    document: TopologyDocument;
  } = $props();

  const connectedEdges = $derived(
    document.edges.filter(
      (edge) =>
        nodeIdKey(edge.from) === nodeIdKey(node.id) ||
        nodeIdKey(edge.to) === nodeIdKey(node.id),
    ),
  );
  // WO-03-F: legacy documents predate linkNetnsid; treat an absent field as
  // not reported (never as "in another namespace").
  const peerNetnsid = $derived(node.detail.linkNetnsid ?? null);
</script>

<div class="details-content">
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
        <dd
          class="mono"
          title={node.detail.peerIfindex !== null
            ? peerNetnsid !== null
              ? 'the kernel named this veth peer AND its network namespace (IFLA_LINK_NETNSID): the peer lives in another namespace and is not part of this observation — no relationship is drawn'
              : 'the kernel named this veth peer; see the veth peer relation below'
            : undefined}
        >
          {#if node.detail.peerIfindex === null}
            —
          {:else if peerNetnsid !== null}
            #{node.detail.peerIfindex} in another netns (netnsid {peerNetnsid === -1 ? 'not assigned' : peerNetnsid})
          {:else}
            #{node.detail.peerIfindex}
          {/if}
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
</div>

<style>
  /* The panel chrome (positioning, header, close) lives in
     AppSidePanel; this styles only the content. */
  h3 {
    margin: 18px 0 8px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--text-secondary);
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
