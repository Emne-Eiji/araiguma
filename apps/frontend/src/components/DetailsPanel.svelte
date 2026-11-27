<script lang="ts">
  // Interface details content (rendered inside the shared AppSidePanel).
  // WO-08 redesign: an honest "observed signal path" — addresses → routes →
  // gateways → link DNS → connections — plus identity and provenance. Every
  // fact comes from the current snapshot; a missing optional section is
  // labeled, never shown as zero; nothing is inferred.

  import type { Interface, NetworkSnapshot, Route } from '../types';
  import type { SectionId } from '../lib/navigation.svelte';
  import { routeKey } from '../lib/routes';
  import { formatIpNet, formatMacObservation, interfaceStateChip, kindLabel, macObservationKind } from '../lib/format';
  import {
    connectionsAvailability,
    connectionsForInterface,
    connectionStateLabel,
    dnsAvailability,
    dnsForInterface,
    dnsServerText,
    formatCount,
    gatewaysForInterface,
    isDefaultRoute,
    routesForInterface,
  } from '../lib/observability';

  let {
    iface,
    snapshot,
    onopenSection,
  }: {
    iface: Interface;
    snapshot: NetworkSnapshot;
    onopenSection?: (section: SectionId) => void;
  } = $props();

  const chip = $derived(interfaceStateChip(iface));
  const addresses = $derived(
    snapshot.addresses
      .filter((a) => a.ifindex === iface.ifindex)
      .sort((a, b) => a.address.localeCompare(b.address)),
  );
  const master = $derived(
    iface.masterIfindex !== null
      ? snapshot.interfaces.find((i) => i.ifindex === iface.masterIfindex)
      : undefined,
  );
  const parent = $derived(
    iface.parentIfindex !== null
      ? snapshot.interfaces.find((i) => i.ifindex === iface.parentIfindex)
      : undefined,
  );
  const peer = $derived(
    iface.peerIfindex !== null
      ? snapshot.interfaces.find((i) => i.ifindex === iface.peerIfindex)
      : undefined,
  );

  const routes = $derived(routesForInterface(snapshot, iface.ifindex));
  const gateways = $derived(gatewaysForInterface(snapshot, iface.ifindex));
  const linkDns = $derived(dnsForInterface(snapshot, iface.ifindex));
  const connections = $derived(connectionsForInterface(snapshot, iface.ifindex));

  const dns = $derived(dnsAvailability(snapshot));
  const connectionAvailability = $derived(connectionsAvailability(snapshot));
  const dnsKnown = $derived(snapshot.dns !== undefined);
  const connectionsKnown = $derived(snapshot.connections !== undefined);
  const globalDnsCount = $derived(
    (snapshot.dns ?? []).filter((observation) => observation.scope.kind === 'global').length,
  );

  function routeLine(route: Route): string {
    const destination = formatIpNet(route.destination);
    const next =
      typeof route.nextHops === 'object' && 'multi' in route.nextHops
        ? `multipath (${route.nextHops.multi.length} next hops)`
        : route.gateway
          ? `via ${route.gateway}`
          : 'on-link';
    const metric = route.metric !== null ? `metric ${route.metric}` : 'metric not observed';
    return `${route.family} ${destination} · ${next} · ${metric}`;
  }

  function openSection(section: SectionId): void {
    onopenSection?.(section);
  }
</script>

<div class="details-content">
  <section>
    <h3>Identity</h3>
    <dl>
      <dt>Kind</dt>
      <dd>{kindLabel(iface.kind)}{#if iface.kindDetail} <span class="mono detail">({iface.kindDetail})</span>{/if}</dd>
      <dt>State</dt>
      <dd><span class="chip {chip.tone}">{chip.label}</span></dd>
      <dt>Admin state</dt>
      <dd>{iface.adminState ?? '—'}<span class="hint"> (IFF_UP)</span></dd>
      <dt>Oper state</dt>
      <dd>{iface.operState ?? '—'}</dd>
      <dt>ifindex</dt>
      <dd class="mono">{iface.ifindex}</dd>
      <dt>MAC address</dt>
      <dd
        class="mono"
        class:dim={macObservationKind(iface.macAddress) !== 'present'}
        title={
          macObservationKind(iface.macAddress) === 'absent'
            ? 'observed: this interface has no hardware address (e.g. wireguard)'
            : macObservationKind(iface.macAddress) === 'notObserved'
              ? 'not observed'
              : undefined
        }
      >
        {formatMacObservation(iface.macAddress)}
      </dd>
      <dt>MTU</dt>
      <dd class="mono">{iface.mtu ?? '—'}</dd>
      <dt>Master</dt>
      <dd class="mono">{master ? master.name : iface.masterIfindex !== null ? iface.masterIfindex : '—'}</dd>
      <dt>Parent link</dt>
      <dd class="mono">{parent ? parent.name : iface.parentIfindex !== null ? iface.parentIfindex : '—'}</dd>
      <dt>veth peer</dt>
      <dd class="mono" title={iface.peerIfindex !== null && !peer
        ? 'the named peer is not part of this observation (it may live in another network namespace)'
        : undefined}
      >
        {peer ? peer.name : iface.peerIfindex !== null ? `#${iface.peerIfindex} (not in this observation)` : '—'}
      </dd>
      <dt>Observed by</dt>
      <dd>{iface.sources.join(', ')}</dd>
    </dl>
  </section>

  <section class="signal" data-testid="signal-path" aria-label="Observed signal path">
    <h3>Observed signal path</h3>
    <p class="signal-note">
      Follow the evidence: addresses → routes → gateways → link DNS →
      connections. Absent optional observations are labeled, not zeroed.
    </p>

    <ol class="steps">
      <li class="step">
        <span class="step-marker" aria-hidden="true">1</span>
        <div class="step-body">
          <header class="step-head">
            <h4>Addresses</h4>
            <span class="count">{formatCount(addresses.length, 'address', 'addresses')}</span>
          </header>
          {#if addresses.length === 0}
            <p class="empty">no addresses observed on this interface</p>
          {:else}
            <ul class="dense">
              {#each addresses as address, i (i)}
                <li class="mono">
                  {address.address}/{address.prefixLen}
                  <span class="muted"> · {address.scope ?? 'scope not observed'}</span>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      </li>

      <li class="step">
        <span class="step-marker" aria-hidden="true">2</span>
        <div class="step-body">
          <header class="step-head">
            <h4>Routes</h4>
            <span class="count">{formatCount(routes.length, 'route')}</span>
          </header>
          <p class="muted step-sub">routes whose output is this interface, including multipath hops</p>
          {#if routes.length === 0}
            <p class="empty">no routes observed via this interface</p>
          {:else}
            <ul class="dense">
              {#each routes as route, index (routeKey(route, index))}
                <li class="mono route-line" class:default={isDefaultRoute(route)}>
                  {routeLine(route)}
                  {#if isDefaultRoute(route)}<span class="muted"> · default</span>{/if}
                </li>
              {/each}
            </ul>
          {/if}
          {#if onopenSection}
            <button class="step-link" onclick={() => openSection('routes')}>Open Routes &amp; Gateways →</button>
          {/if}
        </div>
      </li>

      <li class="step">
        <span class="step-marker" aria-hidden="true">3</span>
        <div class="step-body">
          <header class="step-head">
            <h4>Gateways</h4>
            <span class="count">{formatCount(gateways.length, 'gateway')}</span>
          </header>
          <p class="muted step-sub">distinct next hops named by those routes; reachability is not tested</p>
          {#if gateways.length === 0}
            <p class="empty">no gateway named — observed routes are on-link or no routes are observed</p>
          {:else}
            <ul class="dense">
              {#each gateways as gateway, i (i)}<li class="mono">{gateway}</li>{/each}
            </ul>
          {/if}
        </div>
      </li>

      <li class="step">
        <span class="step-marker" aria-hidden="true">4</span>
        <div class="step-body">
          <header class="step-head">
            <h4>Link DNS</h4>
            {#if dnsKnown}
              <span class="count">{formatCount(linkDns.length, 'link observation')}</span>
            {:else}
              <span class="count muted">not observed</span>
            {/if}
          </header>
          {#if !dnsKnown}
            <p class="empty">{dns.detail}</p>
          {:else if linkDns.length === 0}
            <p class="empty">
              no link-scoped DNS observation for this interface;
              {formatCount(globalDnsCount, 'global observation')} in this document.
              Configuration is not proof of name resolution.
            </p>
          {:else}
            {#each linkDns as observation, index (index)}
              <div class="dns-block">
                <p class="mono dns-servers">
                  {#if observation.servers.length === 0}
                    no servers observed
                  {:else}
                    {observation.servers.map(dnsServerText).join(' · ')}
                  {/if}
                </p>
                <p class="muted dns-domains">
                  search: {observation.searchDomains.join(', ') || 'none observed'} ·
                  routing: {observation.routingDomains.join(', ') || 'none observed'}
                  {#if observation.mode}· mode {observation.mode}{/if}
                </p>
              </div>
            {/each}
          {/if}
          {#if onopenSection}
            <button class="step-link" onclick={() => openSection('dns')}>Open DNS configuration →</button>
          {/if}
        </div>
      </li>

      <li class="step">
        <span class="step-marker" aria-hidden="true">5</span>
        <div class="step-body">
          <header class="step-head">
            <h4>Connections</h4>
            {#if connectionsKnown}
              <span class="count">{formatCount(connections.length, 'connection')}</span>
            {:else}
              <span class="count muted">not observed</span>
            {/if}
          </header>
          <p class="muted step-sub">NetworkManager objects naming this interface; not traffic flow</p>
          {#if !connectionsKnown}
            <p class="empty">{connectionAvailability.detail}</p>
          {:else if connections.length === 0}
            <p class="empty">observed: no connection object names this interface</p>
          {:else}
            <ul class="dense">
              {#each connections as connection, index (index)}
                {@const state = connectionStateLabel(connection.state)}
                <li>
                  <span class="mono">{connection.connectionType}</span>
                  <span class="muted"> · {state.label}{connection.vpn ? ' · VPN' : ''}</span>
                </li>
              {/each}
            </ul>
          {/if}
          {#if onopenSection}
            <button class="step-link" onclick={() => openSection('connections')}>Open Connections →</button>
          {/if}
        </div>
      </li>
    </ol>
  </section>

  {#if iface.notes.length > 0}
    <section>
      <h3>Observation notes</h3>
      <ul class="notes">
        {#each iface.notes as note, i (i)}<li>{note}</li>{/each}
      </ul>
    </section>
  {/if}
</div>

<style>
  h3 {
    margin: 18px 0 8px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--text-secondary);
  }

  h4 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
  }

  dl {
    display: grid;
    grid-template-columns: 96px 1fr;
    row-gap: 6px;
    margin: 0;
  }

  dt {
    color: var(--text-muted);
    font-size: 12px;
    padding-top: 1px;
  }

  dd {
    margin: 0;
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  .detail {
    color: var(--text-muted);
    font-size: 11px;
  }

  .hint {
    color: var(--text-muted);
    font-size: 11px;
  }

  .chip {
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 11px;
    border: 1px solid;
  }

  .chip.up {
    color: var(--state-up);
    border-color: color-mix(in srgb, var(--state-up) 55%, transparent);
  }

  .chip.down {
    color: var(--state-down);
    border-color: color-mix(in srgb, var(--state-down) 55%, transparent);
  }

  .chip.unknown {
    color: var(--state-unknown);
    border-color: color-mix(in srgb, var(--state-unknown) 55%, transparent);
  }

  .mono {
    font-family: var(--font-mono);
    font-size: 0.93em;
  }

  .dim {
    color: var(--text-secondary);
  }

  .muted {
    color: var(--text-muted);
  }

  /* ── Observed signal path ──────────────────────────────────────────── */
  .signal {
    margin-top: 4px;
  }

  .signal-note {
    margin: -2px 0 12px;
    color: var(--text-muted);
    font-size: 11px;
    line-height: 1.45;
  }

  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .step {
    position: relative;
    padding: 0 0 14px 30px;
  }

  .step::before {
    content: '';
    position: absolute;
    left: 9px;
    top: 18px;
    bottom: 0;
    width: 2px;
    background: color-mix(in srgb, var(--border-strong) 80%, transparent);
  }

  .step:last-child {
    padding-bottom: 0;
  }

  .step:last-child::before {
    display: none;
  }

  .step-marker {
    position: absolute;
    left: 0;
    top: 0;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 1px solid var(--border-strong);
    background: var(--bg-inset, var(--bg-raised));
    color: var(--accent);
    font-size: 10px;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .step-body {
    min-width: 0;
  }

  .step-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 5px;
  }

  .count {
    font-size: 10.5px;
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    padding: 0 7px;
    white-space: nowrap;
  }

  .count.muted {
    color: var(--state-unknown);
    border-color: color-mix(in srgb, var(--state-unknown) 45%, transparent);
  }

  .step-sub {
    margin: -2px 0 6px;
    font-size: 10.5px;
    line-height: 1.4;
  }

  .dense {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 11.5px;
    max-height: 180px;
    overflow-y: auto;
  }

  .route-line {
    color: var(--text-secondary);
    font-size: 11px;
    overflow-wrap: anywhere;
  }

  .route-line.default {
    color: var(--accent);
  }

  .dns-block {
    margin: 4px 0;
  }

  .dns-servers {
    margin: 0;
    font-size: 11.5px;
    overflow-wrap: anywhere;
  }

  .dns-domains {
    margin: 2px 0 0;
    font-size: 10.5px;
  }

  .step-link {
    margin-top: 6px;
    color: var(--accent);
    font-size: 11px;
  }

  .step-link:hover {
    text-decoration: underline;
  }

  .empty {
    margin: 0;
    color: var(--text-muted);
    font-size: 11.5px;
  }

  .notes {
    color: var(--state-unknown);
    font-size: 12px;
    margin: 0;
    padding-left: 16px;
  }
</style>
