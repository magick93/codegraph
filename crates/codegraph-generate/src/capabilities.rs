use std::collections::HashSet;

/// One feature-gated generator family enabled for the current run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Capability {
    AdminCli,
    Atproto,
    AuthRateLimit,
    Cli,
    EmDash,
    Fern,
    Grpc,
    Labels,
    Reports,
    Seed,
    TestGen,
    Ui,
    Webhooks,
}

/// The set of [`Capability`] values derived from the build plan for one
/// generator run.
#[derive(Debug, Clone, Default)]
pub(crate) struct CapabilitySet(HashSet<Capability>);

impl CapabilitySet {
    pub(crate) fn has(&self, cap: Capability) -> bool {
        self.0.contains(&cap)
    }

    pub(crate) fn insert(&mut self, cap: Capability) {
        self.0.insert(cap);
    }
}
