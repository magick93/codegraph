//! Media storage plane (issue #625, PR-A): provider-neutral types for the
//! `[domains.X.media]` config surface and the emitted `MediaStorage` seam.
//!
//! Layering: the TOML surface lives in `codegraph_config::config::media`
//! (mirroring `MediaProvider` — the ChartKind precedent, string sets pinned
//! equal by a KAT there); the capability matrix and the object-key template
//! live here so generators can consult them without a config dependency.
//! Provider differences are *declared* (the matrix), never branched at runtime.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// The closed provider vocabulary. Mirrored by
/// `codegraph_config::config::media::MediaProvider` (the TOML surface); a
/// KAT pins the two string sets identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaProvider {
    Supabase,
    R2,
    S3,
}

impl MediaProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Supabase => "supabase",
            Self::R2 => "r2",
            Self::S3 => "s3",
        }
    }

    /// Inverse of [`Self::as_str`]; `None` for unknown vocabulary.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "supabase" => Some(Self::Supabase),
            "r2" => Some(Self::R2),
            "s3" => Some(Self::S3),
            _ => None,
        }
    }

    /// All known variants (deterministic order).
    pub fn all() -> &'static [MediaProvider] {
        &[MediaProvider::Supabase, MediaProvider::R2, MediaProvider::S3]
    }

    /// Emitted adapter module stem under `src/media/` in generated apps.
    pub fn adapter_module(&self) -> &'static str {
        match self {
            Self::Supabase => "supabase",
            Self::R2 => "r2",
            Self::S3 => "s3",
        }
    }
}

/// What a storage provider can do. The generation-time validators consult
/// this matrix; emitted code never branches on provider identity — invalid
/// combos are refused before any file is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProviderCapability {
    /// Browser uploads directly to the provider; the provider enforces
    /// identity on the bytes (e.g. Supabase storage RLS on `auth.uid()`).
    ClientDirectUpload,
    /// Bytes flow through the generated API/worker (binding put or SDK put).
    ThroughAppUpload,
    /// The app mints a short-lived upload URL; the browser PUTs directly.
    PresignPut,
    /// A public-read bucket/CDN posture is available.
    PublicRead,
    /// Short-TTL signed GET URLs for display.
    SignedDisplay,
    /// The provider enforces end-user identity on stored objects
    /// (defense-in-depth; the app-plane gate remains authoritative).
    StorageSideRls,
}

/// The provider capability matrix (issue #625 §2). Total and exhaustive:
/// adding a provider forces a row here — the compiler keeps the matrix honest.
pub fn provider_capabilities(p: MediaProvider) -> BTreeSet<ProviderCapability> {
    use ProviderCapability::*;
    match p {
        MediaProvider::Supabase => [
            ClientDirectUpload,
            ThroughAppUpload,
            PresignPut,
            PublicRead,
            SignedDisplay,
            StorageSideRls,
        ]
        .into(),
        // No end-user identity on objects: client-direct and storage-side RLS
        // are impossible; authority is issued by the app (presign or proxy).
        MediaProvider::R2 | MediaProvider::S3 => [
            ThroughAppUpload,
            PresignPut,
            PublicRead,
            SignedDisplay,
        ]
        .into(),
    }
}

/// The upload data path (config-declared, validated against the matrix).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UploadMode {
    /// Browser → provider; provider enforces identity.
    ClientDirect,
    /// Browser → generated route → provider.
    ThroughApp,
    /// App mints a short-lived PUT; browser uploads direct.
    Presigned,
}

impl UploadMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ClientDirect => "client-direct",
            Self::ThroughApp => "through-app",
            Self::Presigned => "presigned",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "client-direct" => Some(Self::ClientDirect),
            "through-app" => Some(Self::ThroughApp),
            "presigned" => Some(Self::Presigned),
            _ => None,
        }
    }

    /// The matrix capability this mode requires.
    pub fn as_capability(&self) -> ProviderCapability {
        match self {
            Self::ClientDirect => ProviderCapability::ClientDirectUpload,
            Self::ThroughApp => ProviderCapability::ThroughAppUpload,
            Self::Presigned => ProviderCapability::PresignPut,
        }
    }
}

/// Display posture, derived from the flat `public_read` TOML knob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaVisibility {
    /// Signed display URLs minted per request (default — private bucket).
    Signed,
    /// Public bucket: the stored URL renders directly.
    Public,
}

/// One placeholder in a rendered object key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyPlaceholder {
    Org,
    Domain,
    Entity,
    Id,
    Field,
}

impl KeyPlaceholder {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Org => "org",
            Self::Domain => "domain",
            Self::Entity => "entity",
            Self::Id => "id",
            Self::Field => "field",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "org" => Some(Self::Org),
            "domain" => Some(Self::Domain),
            "entity" => Some(Self::Entity),
            "id" => Some(Self::Id),
            "field" => Some(Self::Field),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum KeySegment {
    Literal(String),
    Placeholder(KeyPlaceholder),
}

/// Object-key shape: a `/`-separated template of literals and
/// `{placeholder}` segments (default `{org}/{domain}/{entity}/{id}/{field}`).
/// Overwrite-in-place semantics (no orphan churn on re-upload) require the
/// `{id}` and `{field}` placeholders — parse rejects templates without them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyTemplate {
    segments: Vec<KeySegment>,
}

/// Everything a template needs at render time; resolved by the caller
/// (command/route layer), never by the adapter.
#[derive(Debug, Clone)]
pub struct MediaObjectRef {
    pub org: String,
    pub domain: String,
    pub entity: String,
    pub id: uuid::Uuid,
    pub field: String,
}

impl KeyTemplate {
    /// The default overwrite-in-place template: `{org}/{domain}/{entity}/{id}/{field}`.
    pub fn default_template() -> Self {
        const PLACEHOLDERS: [KeyPlaceholder; 5] = [
            KeyPlaceholder::Org,
            KeyPlaceholder::Domain,
            KeyPlaceholder::Entity,
            KeyPlaceholder::Id,
            KeyPlaceholder::Field,
        ];
        let mut segments = Vec::new();
        for (idx, ph) in PLACEHOLDERS.iter().enumerate() {
            if idx > 0 {
                segments.push(KeySegment::Literal("/".to_string()));
            }
            segments.push(KeySegment::Placeholder(*ph));
        }
        Self { segments }
    }

    /// Parse `{a}/{b}/literal` syntax. Unknown placeholders and templates
    /// without `{id}` or `{field}` are errors (naming the problem) — keys
    /// would otherwise collide across objects or fields.
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut segments = Vec::new();
        let mut placeholders = BTreeSet::new();
        let chunks: Vec<&str> = s.split('/').collect();
        let last = chunks.len().saturating_sub(1);
        for (idx, raw) in chunks.iter().enumerate() {
            if raw.is_empty() {
                return Err(format!(
                    "key template segment {idx} is empty (check for stray slashes): \"{s}\""
                ));
            }
            let mut rest = *raw;
            let mut literal = String::new();
            loop {
                match (rest.find('{'), rest.find('}')) {
                    (Some(open), Some(close)) if close > open => {
                        literal.push_str(&rest[..open]);
                        if !literal.is_empty() {
                            segments.push(KeySegment::Literal(std::mem::take(&mut literal)));
                        }
                        let name = &rest[open + 1..close];
                        let ph = KeyPlaceholder::parse(name).ok_or_else(|| {
                            format!(
                                "unknown key placeholder \"{{{name}}}\" (known: org, domain, entity, id, field)"
                            )
                        })?;
                        placeholders.insert(ph);
                        segments.push(KeySegment::Placeholder(ph));
                        rest = &rest[close + 1..];
                    }
                    (Some(_), _) | (_, Some(_)) => {
                        return Err(format!("unclosed placeholder braces in key template: \"{s}\""));
                    }
                    _ => {
                        literal.push_str(rest);
                        break;
                    }
                }
            }
            if !literal.is_empty() {
                segments.push(KeySegment::Literal(literal));
            }
            if idx != last {
                segments.push(KeySegment::Literal("/".to_string()));
            }
        }
        if segments.is_empty() {
            return Err("key template is empty".to_string());
        }
        for required in [KeyPlaceholder::Id, KeyPlaceholder::Field] {
            if !placeholders.contains(&required) {
                return Err(format!(
                    "key template must contain the {{{}}} placeholder — keys without it collide across {}s",
                    required.as_str(),
                    required.as_str()
                ));
            }
        }
        Ok(Self { segments })
    }

    /// Render the key. Segment values may not contain `/` (rejected — a
    /// path-traversal-shaped key would escape the bucket prefix).
    pub fn render(&self, r: &MediaObjectRef) -> Result<String, String> {
        let mut out = String::new();
        for seg in &self.segments {
            match seg {
                KeySegment::Literal(lit) => out.push_str(lit),
                KeySegment::Placeholder(ph) => {
                    let piece = match ph {
                        KeyPlaceholder::Org => r.org.clone(),
                        KeyPlaceholder::Domain => r.domain.clone(),
                        KeyPlaceholder::Entity => r.entity.clone(),
                        KeyPlaceholder::Id => r.id.to_string(),
                        KeyPlaceholder::Field => r.field.clone(),
                    };
                    if piece.contains('/') || piece.is_empty() {
                        return Err(format!(
                            "key segment {{{}}} renders to \"{}\" — empty values and '/' are rejected (path-shaped keys would escape the bucket prefix)",
                            ph.as_str(),
                            piece
                        ));
                    }
                    out.push_str(&piece);
                }
            }
        }
        Ok(out)
    }
}

/// Provider-specific settings carried from the config surface to the
/// emitted adapter construction (PR-B/PR-C consume the variants).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaProviderSettings {
    /// Env-driven (SUPABASE_URL / service key) — nothing to carry in PR-A.
    Supabase,
    R2 {
        binding: String,
        presign_ttl_secs: u64,
    },
    S3 {
        region: Option<String>,
        endpoint: Option<String>,
    },
}

/// One contained-value media field resolved against the graph: the child
/// table + columns the media surface reads and writes. All names go through
/// the `codegraph-naming` single-source helpers (#460).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaFieldPlan {
    /// Schema title of the owning entity (e.g. "PersonType").
    pub entity_title: String,
    /// Postgres table of the owning entity (e.g. "person").
    pub entity_table: String,
    /// The contained-value feature name as authored (e.g. "avatar").
    pub field_name: String,
    /// snake_case of the feature (e.g. "avatar").
    pub field_snake: String,
    /// `{parent}_{feature}` (e.g. "person_avatar").
    pub child_table: String,
    /// `{parent}_id` (e.g. "person_id").
    pub parent_fk_column: String,
    /// The url column on the child table (e.g. "url").
    pub url_column: String,
    /// The mime-type column on the child table (e.g. "mime_type").
    pub mime_column: String,
}

/// One domain's resolved media plane (issue #625 §3b L2): built
/// post-ingest/pre-generation from `[domains.X.media]` + the graph;
/// consumed by the `media_storage` generator. Deterministic — BTreeMap
/// keying upstream, sorted fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaPlan {
    pub domain: String,
    pub provider: MediaProvider,
    pub upload_mode: UploadMode,
    pub bucket: String,
    pub key_template: KeyTemplate,
    /// Public if `public_read`, else Signed (private bucket + signed
    /// display URLs — the privacy default).
    pub visibility: MediaVisibility,
    pub max_bytes: u64,
    pub accept: Vec<String>,
    pub display_ttl_secs: u64,
    /// Resolved media class titles (exact or `+type_suffix`).
    pub media_classes: Vec<String>,
    /// Contained-value media fields, sorted by (entity_title, field_snake).
    pub fields: Vec<MediaFieldPlan>,
    pub provider_settings: MediaProviderSettings,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── M1: the capability matrix ────────────────────────────────────────

    #[test]
    fn supabase_capabilities_are_the_full_set() {
        let caps = provider_capabilities(MediaProvider::Supabase);
        for c in [
            ProviderCapability::ClientDirectUpload,
            ProviderCapability::ThroughAppUpload,
            ProviderCapability::PresignPut,
            ProviderCapability::PublicRead,
            ProviderCapability::SignedDisplay,
            ProviderCapability::StorageSideRls,
        ] {
            assert!(caps.contains(&c), "supabase missing {c:?}");
        }
        assert_eq!(caps.len(), 6);
    }

    #[test]
    fn r2_lacks_client_direct_and_rls() {
        let caps = provider_capabilities(MediaProvider::R2);
        assert!(!caps.contains(&ProviderCapability::ClientDirectUpload));
        assert!(!caps.contains(&ProviderCapability::StorageSideRls));
        for c in [
            ProviderCapability::ThroughAppUpload,
            ProviderCapability::PresignPut,
            ProviderCapability::PublicRead,
            ProviderCapability::SignedDisplay,
        ] {
            assert!(caps.contains(&c), "r2 missing {c:?}");
        }
        assert_eq!(caps.len(), 4);
    }

    #[test]
    fn s3_matches_r2_capabilities() {
        assert_eq!(
            provider_capabilities(MediaProvider::S3),
            provider_capabilities(MediaProvider::R2)
        );
    }

    #[test]
    fn upload_mode_maps_to_capability() {
        assert_eq!(
            UploadMode::ClientDirect.as_capability(),
            ProviderCapability::ClientDirectUpload
        );
        assert_eq!(
            UploadMode::ThroughApp.as_capability(),
            ProviderCapability::ThroughAppUpload
        );
        assert_eq!(
            UploadMode::Presigned.as_capability(),
            ProviderCapability::PresignPut
        );
    }

    #[test]
    fn provider_as_str_round_trip() {
        for p in MediaProvider::all() {
            assert_eq!(MediaProvider::parse(p.as_str()), Some(*p));
            assert_eq!(MediaProvider::parse(p.adapter_module()), Some(*p));
        }
        assert_eq!(MediaProvider::parse("minio"), None);
        assert_eq!(UploadMode::parse(UploadMode::Presigned.as_str()), Some(UploadMode::Presigned));
        assert_eq!(UploadMode::parse("direct"), None);
    }

    // ── M2: the key template ─────────────────────────────────────────────

    #[test]
    fn default_template_round_trips() {
        let t = KeyTemplate::default_template();
        let r = MediaObjectRef {
            org: "11111111-1111-1111-1111-111111111111".into(),
            domain: "core".into(),
            entity: "person".into(),
            id: uuid::uuid!("22222222-2222-2222-2222-222222222222"),
            field: "avatar".into(),
        };
        assert_eq!(
            t.render(&r).unwrap(),
            "11111111-1111-1111-1111-111111111111/core/person/22222222-2222-2222-2222-222222222222/avatar"
        );
    }

    #[test]
    fn parse_accepts_custom_template_with_literals() {
        let t = KeyTemplate::parse("media/{org}/fixed/{id}/{field}").unwrap();
        let r = MediaObjectRef {
            org: "o".into(),
            domain: "d".into(),
            entity: "e".into(),
            id: uuid::uuid!("22222222-2222-2222-2222-222222222222"),
            field: "f".into(),
        };
        assert_eq!(t.render(&r).unwrap(), "media/o/fixed/22222222-2222-2222-2222-222222222222/f");
    }

    #[test]
    fn unknown_placeholder_is_parse_error() {
        let err = KeyTemplate::parse("{org}/{bucket}/{field}").unwrap_err();
        assert!(err.contains("bucket"), "error names the placeholder: {err}");
    }

    #[test]
    fn missing_id_or_field_placeholder_is_parse_error() {
        let err = KeyTemplate::parse("{org}/{domain}/{entity}").unwrap_err();
        assert!(err.contains("id") || err.contains("field"), "error names the gap: {err}");
        assert!(KeyTemplate::parse("{field}/{id}").is_ok());
    }

    #[test]
    fn unclosed_brace_is_parse_error() {
        assert!(KeyTemplate::parse("{org}/{field").is_err());
    }

    #[test]
    fn render_rejects_separator_in_values() {
        let t = KeyTemplate::parse("{org}/{id}/{field}").unwrap();
        let r = MediaObjectRef {
            org: "org/sub".into(),
            domain: "d".into(),
            entity: "e".into(),
            id: uuid::uuid!("22222222-2222-2222-2222-222222222222"),
            field: "avatar".into(),
        };
        assert!(t.render(&r).is_err());
    }
}
