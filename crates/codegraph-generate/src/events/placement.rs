//! Worker-topology event placement (issue #505): which domain worker
//! drains which pgmq queue, and which workers host which typed
//! consumers.
//!
//! The soundness invariant this module enforces: a pgmq delete is
//! queue-global while the typed consumer registry is process-local, so
//! every queue gets EXACTLY ONE draining worker and that worker hosts
//! every consumer subscribed to events reachable on it. Competing
//! drains with partial registries would silently drop events (the
//! failure mode #505 exists to kill).
//!
//! Ownership resolves through a deterministic tier chain over graph
//! data, then hard-errors when nothing can own a queue:
//!
//! 1. **Queue-name tier** — a channel named like a worker domain
//!    collapses into that domain's trigger queue (the
//!    [`EventArchitecture::queue_inventory`] collision rule); the
//!    domain's own worker owns it.
//! 2. **Payload-domain tier** — the channel's declared events' derived
//!    domains ([`EventDefinition::domain`]) all agree on one worker
//!    domain.
//! 3. **Deploy tier** — a `.deploy` `queue` component named like the
//!    channel, joined to a domain worker's component by a `connects`
//!    edge (the component-name bridge `build_worker_domains` already
//!    uses).
//! 4. **Miss** — a generation error naming the channel, the observed
//!    candidate domains, and the `.deploy` fix.
//!
//! Consumers ride queue owners: a consumer is placed on every worker
//! that owns a queue carrying one of its subscribed events (dispatch
//! filters by event name, so each copy only fires for its own queues).
//! Inert without `.evt` input — an empty architecture yields empty
//! placement and every consumer keeps its byte-identical output.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use codegraph_config::DomainConfig;

use crate::error::{Error, Result};
use crate::events::{EventArchitecture, PublicationSource, Transport};
use crate::scaffold::worker::DeployShape;

/// The placement decision for one workers-topology run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventPlacement {
    /// Per worker domain: the names whose queues this worker drains —
    /// the arguments the emitted dispatch cycle passes through
    /// (`events_{name}` is derived at both ends). The worker's own
    /// domain name comes first (its trigger queue), then every channel
    /// queue assigned to it, sorted. A worker absent from the map owns
    /// no queue.
    pub worker_drains: BTreeMap<String, Vec<String>>,
    /// Per worker domain: the consumer names placed on it, sorted.
    /// Every consumer subscribed to an event reachable on one of the
    /// worker's queues appears here.
    pub worker_consumers: BTreeMap<String, Vec<String>>,
}

impl EventPlacement {
    /// Whether `domain`'s worker owns at least one queue (the
    /// `has_events` gate on the emitted worker scaffold).
    pub fn drains_any(&self, domain: &str) -> bool {
        self.worker_drains
            .get(domain)
            .is_some_and(|queues| !queues.is_empty())
    }

    /// The channel queues assigned to `domain` beyond its own trigger
    /// queue (the extra dispatch-cycle drain targets).
    pub fn extra_drains(&self, domain: &str) -> Vec<String> {
        self.worker_drains
            .get(domain)
            .map(|queues| {
                queues
                    .iter()
                    .filter(|name| name.as_str() != domain)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The consumer names placed on `domain`'s worker.
    pub fn consumers_for(&self, domain: &str) -> Vec<String> {
        self.worker_consumers
            .get(domain)
            .cloned()
            .unwrap_or_default()
    }
}

/// Resolve queue ownership + consumer placement for the workers
/// topology. `worker_domains` is the set of scaffold domain keys (the
/// worker crates that exist); `deploy` is the resolved `.deploy` shape
/// (`None` = no deploy file). Empty architecture ⇒ empty placement.
pub fn compute_placement(
    arch: &EventArchitecture,
    config: &DomainConfig,
    worker_domains: &HashSet<String>,
    deploy: Option<&DeployShape>,
) -> Result<EventPlacement> {
    let mut placement = EventPlacement::default();
    if arch.is_empty() {
        return Ok(placement);
    }

    // Every queue with its owning channel (channels only — the
    // per-domain trigger queues are owned by their domain trivially).
    // The map is channel name -> queue name for owner resolution.
    let mut channel_queue: BTreeMap<&str, &str> = BTreeMap::new();
    for channel in arch.bound_channels() {
        if let Transport::Pgmq(pgmq) = &channel.transport {
            channel_queue.insert(channel.name.as_str(), pgmq.queue.as_str());
        }
    }

    // Events → the bound channels publishing them (for consumer
    // placement).
    let mut event_channels: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for channel in arch.bound_channels() {
        for publication in &channel.publications {
            event_channels
                .entry(publication.event.as_str())
                .or_default()
                .insert(channel.name.as_str());
        }
    }

    // ── queue ownership ─────────────────────────────────────────────
    // Tier 0: a domain's own trigger queue is owned by its worker.
    for domain_channel in &arch.domain_channels {
        if matches!(domain_channel.transport, Transport::Pgmq(_))
            && worker_domains.contains(domain_channel.name.as_str())
        {
            placement
                .worker_drains
                .entry(domain_channel.name.clone())
                .or_default()
                .push(domain_channel.name.clone());
        }
    }

    for (channel, queue) in &channel_queue {
        let owner = resolve_channel_owner(arch, config, worker_domains, deploy, channel, queue)?;
        // A channel named like its own domain collapses into the
        // trigger queue — the drain entry already exists.
        if owner.as_str() != *channel {
            placement
                .worker_drains
                .entry(owner)
                .or_default()
                .push((*channel).to_string());
        }
    }
    for queues in placement.worker_drains.values_mut() {
        queues.sort();
        queues.dedup();
    }

    // ── consumer placement: consumers ride queue owners ─────────────
    for subscription in &arch.subscriptions {
        let mut owners: BTreeSet<String> = BTreeSet::new();
        for event_name in &subscription.events {
            // Undeclared events are skipped with a warning by the
            // consumers context; placement mirrors that set exactly.
            let Some(event) = arch.event(event_name) else {
                continue;
            };
            for channel in event_channels
                .get(event.name.as_str())
                .into_iter()
                .flatten()
                .copied()
            {
                let queue = channel_queue[channel];
                if let Some(owner) = owner_of_drain(&placement, queue) {
                    owners.insert(owner.to_string());
                }
            }
        }
        for owner in owners {
            placement
                .worker_consumers
                .entry(owner)
                .or_default()
                .push(subscription.consumer.clone());
        }
    }
    for consumers in placement.worker_consumers.values_mut() {
        consumers.sort();
        consumers.dedup();
    }

    Ok(placement)
}

/// The worker that drains `queue`, read back from the drains map (the
/// inverse of the ownership assignment above).
fn owner_of_drain<'a>(placement: &'a EventPlacement, queue: &str) -> Option<&'a str> {
    let name = queue.strip_prefix("events_")?;
    placement
        .worker_drains
        .iter()
        .find(|(_, drains)| drains.iter().any(|d| d == name))
        .map(|(worker, _)| worker.as_str())
}

/// The tier chain for one bound channel queue. `channel` is the channel
/// name, `queue` its pgmq queue (`events_{channel}`).
fn resolve_channel_owner(
    arch: &EventArchitecture,
    config: &DomainConfig,
    worker_domains: &HashSet<String>,
    deploy: Option<&DeployShape>,
    channel: &str,
    queue: &str,
) -> Result<String> {
    let mut candidates: BTreeSet<String> = BTreeSet::new();

    // Tier 1: the channel is named like a worker domain — its queue is
    // that domain's trigger queue (the collapse rule).
    if worker_domains.contains(channel) {
        return Ok(channel.to_string());
    }

    // Tier 2: the channel's declared events' derived domains all agree.
    // All-primitive events (domain `None`) don't vote — they carry no
    // domain ownership to violate; ambiguity is two DIFFERENT resolved
    // domains on one channel.
    let mut event_domains: BTreeSet<&str> = BTreeSet::new();
    for publication in arch
        .channels
        .iter()
        .filter(|c| c.name == channel)
        .flat_map(|c| &c.publications)
    {
        if !matches!(publication.source, PublicationSource::Application) {
            continue;
        }
        if let Some(event) = arch.event(&publication.event)
            && let Some(domain) = event.domain.as_deref()
        {
            event_domains.insert(domain);
        }
    }
    if event_domains.len() == 1 {
        let domain = event_domains.iter().next().unwrap();
        if worker_domains.contains(*domain) {
            return Ok((*domain).to_string());
        }
        candidates.insert((*domain).to_string());
    } else {
        for domain in &event_domains {
            candidates.insert((*domain).to_string());
        }
    }

    // Tier 3: a `.deploy` `queue` component named like the channel,
    // connected to a domain worker's component by a `connects` edge
    // (worker → queue preferred; either direction accepted).
    if let Some(shape) = deploy {
        let is_worker_component = |name: &str| {
            shape.components.iter().any(|c| {
                c.name == name
                    && (c.kind == "api" || c.kind == "worker")
                    && worker_domains.contains(name)
            })
        };
        let mut direct: Option<String> = None;
        let mut reversed: Option<String> = None;
        for edge in &shape.connections {
            if edge.to == channel && is_worker_component(&edge.from) {
                direct = Some(edge.from.clone());
            }
            if edge.from == channel && is_worker_component(&edge.to) {
                reversed = Some(edge.to.clone());
            }
        }
        if let Some(owner) = direct.or(reversed) {
            return Ok(owner);
        }
        // Record queue components named for this channel as candidate
        // owners-miss context is unnecessary — the edge is what's
        // missing; fall through to the error with the deploy hint.
    }

    // Tier 4: nothing owns the queue. Hard error naming the channel,
    // the candidates, and the deploy fix.
    let candidates: Vec<String> = candidates.into_iter().collect();
    let candidate_note = if candidates.is_empty() {
        "its events resolve to no worker domain".to_string()
    } else {
        format!(
            "its events' domains are ambiguous: {}",
            candidates.join(", ")
        )
    };
    Err(Error::Config(format!(
        "channel `{channel}' (pgmq queue `{queue}') has no drain under the workers topology: \
         not a domain name, {candidate_note}, and no `.deploy` queue component owns it. \
         Add a queue component and a connects edge to the owning worker's application, e.g. \
         `component \"{channel}\" {{ kind = \"queue\" }}` + `connects <worker-component> -> \"{channel}\"` \
         (domains.toml keys: {}).",
        config
            .domains
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

#[cfg(test)]
mod tests;
