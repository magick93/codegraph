//! Media storage configuration (issue #625, PR-A): per-domain
//! `[domains.X.media]` — the storage-target knob (the crons/observability
//! precedent). Absent on every domain ⇒ no media plane ⇒ byte-identical
//! output.
//!
//! Validation split by crate boundary: THIS module enforces everything
//! expressible without the core mirror (subsection/provider match, bounds,
//! accept entries) at parse time — `parse_domain_config_str` runs it, so
//! doctor and run both fail on a broken config before anything else. The
//! provider CAPABILITY matrix (`upload_mode` vs what the provider can do)
//! and the `{...}` key-template syntax live in `codegraph_core::types::
//! media` and are enforced generation-side (`codegraph-generate::media`,
//! consumed by the driver pre-generation and by doctor) — the config crate
//! deliberately has no production dependency on core (the charts/KAT
//! convention).

use serde::Deserialize;

use crate::error::DomainConfigError;

/// The closed provider vocabulary (TOML surface). Mirrored by
/// `codegraph_core::types::MediaProvider`; the string sets are pinned equal
/// by a KAT in this module (the ChartKind precedent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
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

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "supabase" => Some(Self::Supabase),
            "r2" => Some(Self::R2),
            "s3" => Some(Self::S3),
            _ => None,
        }
    }

    /// All known variants (deterministic order) — KAT/audit use.
    pub fn all() -> &'static [MediaProvider] {
        &[MediaProvider::Supabase, MediaProvider::R2, MediaProvider::S3]
    }
}

/// The upload data path (TOML surface; mirrored by the core enum — the KAT
/// covers modes too). Validated against the provider capability matrix
/// generation-side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MediaUploadMode {
    ClientDirect,
    ThroughApp,
    Presigned,
}

impl MediaUploadMode {
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
}

/// Provider-specific subsections. A table is only legal under its own
/// provider — `[media.r2]` with `provider = "supabase"` is a parse error
/// naming the stray table (strictness per issue #625 §2).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupabaseMediaConfig {}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct R2MediaConfig {
    /// Wrangler `[[r2_buckets]]` binding name (consumed by the workers
    /// scaffold in the R2 slice — carried from day one so the config shape
    /// never migrates).
    pub binding: String,
    #[serde(default = "default_presign_ttl")]
    pub presign_ttl_secs: u64,
}

fn default_presign_ttl() -> u64 {
    900
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S3MediaConfig {
    /// None = the AWS_REGION env decides at runtime.
    #[serde(default)]
    pub region: Option<String>,
    /// Self-host / MinIO endpoint. None = the AWS SDK default resolution.
    #[serde(default)]
    pub endpoint: Option<String>,
}

fn default_key_template() -> String {
    "{org}/{domain}/{entity}/{id}/{field}".to_string()
}

fn default_display_ttl() -> u64 {
    300
}

fn default_url_field() -> String {
    "url".to_string()
}

fn default_mime_field() -> String {
    "mime_type".to_string()
}

/// One domain's media storage declaration. STRICT: unknown keys are parse
/// errors (new surface — no back-compat burden; the `deny_unknown_fields`
/// class). The `upload_mode`-vs-provider matrix and the `{...}` key
/// template syntax are validated generation-side against the core mirror
/// (see the module docs).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaConfig {
    pub provider: MediaProvider,
    pub upload_mode: MediaUploadMode,
    pub bucket: String,
    pub max_bytes: u64,
    pub accept: Vec<String>,
    #[serde(default = "default_key_template")]
    pub key_template: String,
    /// Display posture sugar: true ⇒ public bucket (URL renders directly);
    /// false (default) ⇒ signed display URLs — personal data defaults
    /// private (crewbase#79's privacy caveat).
    #[serde(default)]
    pub public_read: bool,
    #[serde(default = "default_display_ttl")]
    pub display_ttl_secs: u64,
    /// Contained-value feature names counted as media references within
    /// this domain's entities (the mox-side marker, open question #5 in
    /// #625 — config-declared for PR-A so the DSL stays clean).
    #[serde(default = "default_media_classes")]
    pub media_classes: Vec<String>,
    /// Column name (snake-lowered compare) of the url field on the media
    /// value class.
    #[serde(default = "default_url_field")]
    pub url_field: String,
    /// Column name (snake-lowered compare) of the mime-type field.
    #[serde(default = "default_mime_field")]
    pub mime_field: String,
    #[serde(default)]
    pub supabase: Option<SupabaseMediaConfig>,
    #[serde(default)]
    pub r2: Option<R2MediaConfig>,
    #[serde(default)]
    pub s3: Option<S3MediaConfig>,
}

fn default_media_classes() -> Vec<String> {
    vec!["MediaReference".to_string()]
}

impl MediaConfig {
    /// Structural validation (core-free): provider-subsection agreement,
    /// bounds, accept entries. Errors name the offending key — the
    /// `charts.rs` strictness ("validation is hard, never silent").
    pub fn validate(&self) -> Result<(), DomainConfigError> {
        todo!("M3 GREEN: structural validation")
    }
}

/// Validate every domain's `[media]` declaration (no-op when none — the
/// zero-media parse path must stay byte-identical for existing configs).
pub fn validate_media_config(config: &crate::config::DomainConfig) -> Result<(), DomainConfigError> {
    for (name, entry) in &config.domains {
        if let Some(media) = &entry.media {
            media
                .validate()
                .map_err(|e| DomainConfigError::Invalid(format!("domain \"{name}\" media: {e}")))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse_domain_config_str;

    fn base_toml(provider: &str, mode: &str) -> String {
        format!(
            "[domains.core]\nlabel = \"Core\"\nschema_dir = \"schemas\"\npostgres_schema = \"core\"\n\n[domains.core.media]\nprovider = \"{provider}\"\nupload_mode = \"{mode}\"\nbucket = \"person-media\"\nmax_bytes = 5242880\naccept = [\"image/png\"]\n"
        )
    }

    // ── parse surface ────────────────────────────────────────────────────

    #[test]
    fn minimal_media_config_parses_with_defaults() {
        let cfg = parse_domain_config_str(&base_toml("supabase", "client-direct")).unwrap();
        let m = cfg.domains["core"].media.as_ref().unwrap();
        assert_eq!(m.provider, MediaProvider::Supabase);
        assert_eq!(m.key_template, "{org}/{domain}/{entity}/{id}/{field}");
        assert!(!m.public_read);
        assert_eq!(m.display_ttl_secs, 300);
        assert_eq!(m.url_field, "url");
        assert_eq!(m.mime_field, "mime_type");
        assert_eq!(m.media_classes, vec!["MediaReference".to_string()]);
    }

    #[test]
    fn absent_media_stays_none() {
        let cfg = parse_domain_config_str(
            "[domains.core]\nlabel = \"Core\"\nschema_dir = \"schemas\"\npostgres_schema = \"core\"\n",
        )
        .unwrap();
        assert!(cfg.domains["core"].media.is_none());
    }

    #[test]
    fn unknown_media_keys_are_parse_errors() {
        let toml = base_toml("supabase", "client-direct")
            + "\n[domains.core.media.bogus]\nkey = 1\n";
        let err = parse_domain_config_str(&toml).unwrap_err().to_string();
        assert!(err.contains("bogus"), "{err}");
    }

    #[test]
    fn unknown_provider_value_is_parse_error() {
        let err = parse_domain_config_str(&base_toml("minio", "presigned"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("minio"), "{err}");
    }

    #[test]
    fn unknown_upload_mode_is_parse_error() {
        let err = parse_domain_config_str(&base_toml("supabase", "direct"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("direct"), "{err}");
    }

    // ── structural validation (validate()) ───────────────────────────────

    #[test]
    fn stray_provider_subsection_is_error() {
        let toml = base_toml("supabase", "client-direct")
            + "\n[domains.core.media.r2]\nbinding = \"MEDIA\"\n";
        let err = parse_domain_config_str(&toml).unwrap_err().to_string();
        assert!(err.contains("media.r2"), "{err}");
        assert!(err.contains("supabase"), "{err}");
    }

    #[test]
    fn r2_provider_requires_the_r2_subsection() {
        let err = parse_domain_config_str(&base_toml("r2", "through-app"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("r2"), "{err}");
    }

    #[test]
    fn r2_requires_non_empty_binding() {
        let toml = base_toml("r2", "through-app")
            + "\n[domains.core.media.r2]\nbinding = \"\"\n";
        let err = parse_domain_config_str(&toml).unwrap_err().to_string();
        assert!(err.contains("binding"), "{err}");
    }

    #[test]
    fn matching_subsection_is_accepted() {
        let toml = base_toml("r2", "through-app")
            + "\n[domains.core.media.r2]\nbinding = \"MEDIA_BUCKET\"\npresign_ttl_secs = 60\n";
        let cfg = parse_domain_config_str(&toml).unwrap();
        assert_eq!(cfg.domains["core"].media.as_ref().unwrap().provider, MediaProvider::R2);
    }

    #[test]
    fn zero_max_bytes_is_error() {
        let toml = base_toml("supabase", "client-direct").replace("max_bytes = 5242880", "max_bytes = 0");
        let err = parse_domain_config_str(&toml).unwrap_err().to_string();
        assert!(err.contains("max_bytes"), "{err}");
    }

    #[test]
    fn empty_accept_list_is_error() {
        let toml = base_toml("supabase", "client-direct").replace("accept = [\"image/png\"]", "accept = []");
        let err = parse_domain_config_str(&toml).unwrap_err().to_string();
        assert!(err.contains("accept"), "{err}");
    }

    #[test]
    fn accept_entries_must_look_like_mime_types() {
        let toml = base_toml("supabase", "client-direct").replace("accept = [\"image/png\"]", "accept = [\"png\"]");
        let err = parse_domain_config_str(&toml).unwrap_err().to_string();
        assert!(err.contains("png"), "{err}");
    }

    #[test]
    fn empty_bucket_is_error() {
        let toml = base_toml("supabase", "client-direct").replace("bucket = \"person-media\"", "bucket = \"\"");
        let err = parse_domain_config_str(&toml).unwrap_err().to_string();
        assert!(err.contains("bucket"), "{err}");
    }

    #[test]
    fn empty_key_template_is_error() {
        let toml = base_toml("supabase", "client-direct")
            + "\nkey_template = \"\"\n";
        let err = parse_domain_config_str(&toml).unwrap_err().to_string();
        assert!(err.contains("key_template"), "{err}");
    }

    // ── the vocabulary KAT (ChartKind precedent) ─────────────────────────

    #[test]
    fn provider_string_sets_match_core_mirror() {
        for p in MediaProvider::all() {
            assert_eq!(
                codegraph_core::types::MediaProvider::parse(p.as_str()),
                Some(match p {
                    MediaProvider::Supabase => codegraph_core::types::MediaProvider::Supabase,
                    MediaProvider::R2 => codegraph_core::types::MediaProvider::R2,
                    MediaProvider::S3 => codegraph_core::types::MediaProvider::S3,
                }),
                "config provider '{}' missing from the core mirror",
                p.as_str()
            );
        }
    }

    #[test]
    fn upload_mode_string_sets_match_core_mirror() {
        for m in [MediaUploadMode::ClientDirect, MediaUploadMode::ThroughApp, MediaUploadMode::Presigned] {
            assert_eq!(
                codegraph_core::types::UploadMode::parse(m.as_str()),
                Some(match m {
                    MediaUploadMode::ClientDirect => codegraph_core::types::UploadMode::ClientDirect,
                    MediaUploadMode::ThroughApp => codegraph_core::types::UploadMode::ThroughApp,
                    MediaUploadMode::Presigned => codegraph_core::types::UploadMode::Presigned,
                }),
                "config upload mode '{}' missing from the core mirror",
                m.as_str()
            );
        }
    }
}
