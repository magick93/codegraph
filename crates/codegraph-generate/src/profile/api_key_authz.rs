//! API-key authorization modes (issue #635).
//!
//! The #635 decision brief asks "what is an API key to the actor policy?".
//! Instead of carving one answer into every generated app, each open
//! question is a strict `[features]` key, and every accepted value is a
//! complete generated mode with its own gates:
//!
//! ```toml
//! api_key_mode = "bypass"            # "bypass" | "agent"
//! unmodified_grants = "any"          # "any" | "human" | "agent"
//! session_kind_planes = ["handler"]  # "handler"
//! ```
//!
//! The defaults reproduce the pre-#635 output byte for byte. `delegated` /
//! `per_key` and the `rls` plane are reserved: they parse to a NAMED
//! "not yet supported" error (a mode is selectable only once its gates are
//! green — never a silently partial mode).

use std::fmt;

use codegraph_core::types::GrantSessionKind;

use crate::error::{Error, Result};

/// What an API-key session is to the actor-policy gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyMode {
    /// Today's documented v1 semantics: key sessions skip the capability,
    /// separation-of-duty and grant `when` checks (scope + tenancy RLS and
    /// the op-level `when` still apply).
    #[default]
    Bypass,
    /// A key session is an `agent` principal: grants admitting agent
    /// sessions authorize it, human-only grants deny it, and separation of
    /// duty + grant `when`s evaluate.
    Agent,
}

impl ApiKeyMode {
    /// Parse the `api_key_mode` feature value.
    pub fn from_config(s: &str) -> Result<Self> {
        match s {
            "bypass" => Ok(Self::Bypass),
            "agent" => Ok(Self::Agent),
            "delegated" | "per_key" => Err(Error::Config(format!(
                "api_key_mode \"{s}\" is not yet supported (issue #635 slice 2: it \
                 needs the api_keys minter/kind stamp); supported values are \
                 \"bypass\" (default) and \"agent\""
            ))),
            other => Err(Error::Config(format!(
                "unknown api_key_mode \"{other}\" in profile features; expected \
                 \"bypass\" (default) or \"agent\""
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bypass => "bypass",
            Self::Agent => "agent",
        }
    }
}

impl fmt::Display for ApiKeyMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The resolved API-key authorization configuration of a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct ApiKeyAuthz {
    /// `api_key_mode`.
    pub mode: ApiKeyMode,
    /// `unmodified_grants`: which session kind a grant WITHOUT a
    /// `human`/`agent` modifier admits. `Any` (default) = every kind.
    pub unmodified_grants: GrantSessionKind,
}

impl ApiKeyAuthz {
    /// Parse the three `[features]` keys. Absent keys take their defaults.
    pub fn from_features(features: &toml::Table) -> Result<Self> {
        let mode = match features.get("api_key_mode") {
            Some(v) => ApiKeyMode::from_config(v.as_str().ok_or_else(|| {
                Error::Config("feature \"api_key_mode\" must be a string".to_string())
            })?)?,
            None => ApiKeyMode::default(),
        };
        let unmodified_grants = match features.get("unmodified_grants") {
            Some(v) => parse_unmodified(v.as_str().ok_or_else(|| {
                Error::Config("feature \"unmodified_grants\" must be a string".to_string())
            })?)?,
            None => GrantSessionKind::Any,
        };
        if let Some(v) = features.get("session_kind_planes") {
            parse_planes(v)?;
        }
        Ok(Self {
            mode,
            unmodified_grants,
        })
    }

    /// The session kind a grant admits once `unmodified_grants` is applied
    /// (the ONLY place the default is applied).
    pub fn effective_kind(&self, authored: GrantSessionKind) -> GrantSessionKind {
        if authored.is_any() {
            self.unmodified_grants
        } else {
            authored
        }
    }

    /// Whether a grant authored with `authored` admits a session of
    /// `session` kind (`Human` or `Agent`).
    pub fn admits(&self, authored: GrantSessionKind, session: GrantSessionKind) -> bool {
        let kind = self.effective_kind(authored);
        kind.is_any() || kind == session
    }

    /// Whether this is the pre-#635 configuration (byte-identical output).
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

fn parse_unmodified(s: &str) -> Result<GrantSessionKind> {
    match s {
        "any" => Ok(GrantSessionKind::Any),
        "human" => Ok(GrantSessionKind::Human),
        "agent" => Ok(GrantSessionKind::Agent),
        other => Err(Error::Config(format!(
            "unknown unmodified_grants \"{other}\" in profile features; expected \
             \"any\" (default), \"human\" or \"agent\""
        ))),
    }
}

fn parse_planes(v: &toml::Value) -> Result<()> {
    let arr = v.as_array().ok_or_else(|| {
        Error::Config("feature \"session_kind_planes\" must be an array of strings".to_string())
    })?;
    let mut handler = false;
    for item in arr {
        match item.as_str() {
            Some("handler") => handler = true,
            Some("rls") => {
                return Err(Error::Config(
                    "session_kind_planes \"rls\" is not yet supported (issue #635 slice 3: \
                     one policy per (capability, kind)); supported: [\"handler\"]"
                        .to_string(),
                ));
            }
            Some(other) => {
                return Err(Error::Config(format!(
                    "unknown session_kind_planes entry \"{other}\"; expected \"handler\""
                )));
            }
            None => {
                return Err(Error::Config(
                    "feature \"session_kind_planes\" must be an array of strings".to_string(),
                ));
            }
        }
    }
    if !handler {
        return Err(Error::Config(
            "session_kind_planes must include \"handler\" (the HTTP handler is the \
             session-kind enforcement plane)"
                .to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Result<ApiKeyAuthz> {
        ApiKeyAuthz::from_features(&src.parse::<toml::Table>().unwrap())
    }

    fn err(src: &str) -> String {
        parse(src).unwrap_err().to_string()
    }

    #[test]
    fn defaults_are_bypass_and_any() {
        let a = parse("").unwrap();
        assert!(a.is_default());
        assert_eq!(a.mode, ApiKeyMode::Bypass);
        assert_eq!(a.unmodified_grants, GrantSessionKind::Any);
    }

    #[test]
    fn accepted_values_parse() {
        let a = parse(
            "api_key_mode = \"agent\"\nunmodified_grants = \"human\"\nsession_kind_planes = [\"handler\"]",
        )
        .unwrap();
        assert_eq!(a.mode, ApiKeyMode::Agent);
        assert_eq!(a.unmodified_grants, GrantSessionKind::Human);
        assert!(!a.is_default());
        assert_eq!(
            parse("unmodified_grants = \"agent\"")
                .unwrap()
                .unmodified_grants,
            GrantSessionKind::Agent
        );
    }

    #[test]
    fn reserved_values_name_their_slice() {
        for mode in ["delegated", "per_key"] {
            let e = err(&format!("api_key_mode = \"{mode}\""));
            assert!(e.contains("not yet supported") && e.contains(mode), "{e}");
        }
        let e = err("session_kind_planes = [\"handler\", \"rls\"]");
        assert!(e.contains("not yet supported") && e.contains("rls"), "{e}");
    }

    #[test]
    fn unknown_and_mistyped_values_are_hard_errors() {
        assert!(err("api_key_mode = \"admin\"").contains("unknown api_key_mode"));
        assert!(err("api_key_mode = true").contains("must be a string"));
        assert!(err("unmodified_grants = \"robot\"").contains("unknown unmodified_grants"));
        assert!(err("unmodified_grants = 1").contains("must be a string"));
        assert!(err("session_kind_planes = \"handler\"").contains("must be an array"));
        assert!(err("session_kind_planes = [\"db\"]").contains("unknown session_kind_planes"));
        assert!(err("session_kind_planes = []").contains("must include"));
    }

    #[test]
    fn admission_applies_the_unmodified_default_only_to_unmodified_grants() {
        use GrantSessionKind::{Agent, Any, Human};
        let any = ApiKeyAuthz::default();
        assert!(any.admits(Any, Human) && any.admits(Any, Agent));
        assert!(any.admits(Human, Human) && !any.admits(Human, Agent));
        assert!(any.admits(Agent, Agent) && !any.admits(Agent, Human));

        let human = ApiKeyAuthz {
            mode: ApiKeyMode::Agent,
            unmodified_grants: Human,
        };
        assert!(human.admits(Any, Human) && !human.admits(Any, Agent));
        // An authored modifier always wins over the default.
        assert!(human.admits(Agent, Agent) && !human.admits(Agent, Human));
    }
}
