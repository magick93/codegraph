//! The `?include_deleted=` role gate (issue #792).
//!
//! The soft-delete read toggle is a PRIVILEGED read: honoring it requires
//! more than a session. Two layers, both required:
//!
//! 1. **Authentication** — already in place (`scaffold/middleware.tera`'s
//!    `auth_middleware` authenticates every `/api` request and inserts
//!    `AuthInfo` into the request extensions). Nothing new.
//! 2. **Role gate** — the session's JWT role must rank `manager`-or-better
//!    in the domain's role hierarchy
//!    (`codegraph_config::config::default_roles_hierarchy()` = owner,
//!    manager, member, employee; `[rbac] roles_hierarchy` overrides). Rank
//!    is the index — lower index = higher privilege. An under-privileged
//!    session sending `?include_deleted=true` answers 403 naming the
//!    requirement; absent/`false` needs no gate (today's read behaviour).
//!
//! API-key sessions bypass the role gate — the `permission_middleware.tera`
//! precedent (`AuthMode::ApiKey { .. } => true`): a key's authority is its
//! RLS scopes (#169), and API-key sessions carry `role: "api_key"`, which is
//! not a hierarchy role. Without the bypass every generated hurl contract
//! and machine consumer would 403.
//!
//! Fail closed: a hierarchy without `manager` (or an unknown role) refuses
//! everything.
//!
//! ONE spec function ([`include_deleted_role_allows`]) is consumed by the
//! generator so the emitted helper and these KATs derive from one place; the
//! emitted helper is the spec fn + the api-key bypass + the hierarchy const,
//! byte-pinned by the handler matrix.

/// The spec: does `role` rank `manager`-or-better in `hierarchy`?
///
/// Rank is the index (lower index = higher privilege). `manager` itself
/// passes; everything after it — and every role the hierarchy does not name
/// — refuses. A hierarchy without `manager` refuses everything (fail
/// closed).
pub fn include_deleted_role_allows(role: &str, hierarchy: &[String]) -> bool {
    let Some(min_rank) = hierarchy.iter().position(|r| r == "manager") else {
        return false;
    };
    hierarchy
        .iter()
        .position(|r| r == role)
        .is_some_and(|rank| rank <= min_rank)
}

/// The pre-rendered gate artifacts for one entity's handler file.
/// Empty fields keep the handler byte-identical (non-auditable entities and
/// shapes with no read surface render nothing — see
/// [`build_include_deleted_gate`]).
pub struct IncludeDeletedGate {
    /// The emitted `include_deleted_role_allows` helper fn (top level, once
    /// per handler file). Whitespace-exact; no trailing newline.
    pub helper_fn: String,
    /// The pre-rendered 403 guard block, rendered after the handler's
    /// `let correlation_id = …` line (the `#338` block convention: starts
    /// with a newline, carries NO trailing newline — the template supplies
    /// the line break).
    pub guard_block: String,
}

/// Build the gate artifacts for one entity. Active only when the entity is
/// auditable AND at least one GET/LIST/SEARCH surface exists to consult the
/// gate (`has_read or has_list or has_fts`) — otherwise the helper would be
/// dead code and the guard would reference a `params` binding no handler
/// binds. Empty fields keep the handler byte-identical.
pub fn build_include_deleted_gate(
    is_auditable: bool,
    has_read: bool,
    has_list: bool,
    has_fts: bool,
    hierarchy: &[String],
) -> IncludeDeletedGate {
    let active = is_auditable && (has_read || has_list || has_fts);
    if !active {
        return IncludeDeletedGate {
            helper_fn: String::new(),
            guard_block: String::new(),
        };
    }
    let hierarchy_list = hierarchy
        .iter()
        .map(|role| format!("\"{role}\""))
        .collect::<Vec<_>>()
        .join(", ");
    IncludeDeletedGate {
        helper_fn: format!(
            r#"/// `?include_deleted=true` is a privileged read (issue #792): JWT sessions
/// need a `manager`-or-better role in the domain's role hierarchy (earlier
/// ranks higher). API-key sessions pass — their authority is the key's RLS
/// scopes (#169). Unknown roles and hierarchies without `manager` fail closed.
fn include_deleted_role_allows(auth_info: &ApiKeyInfo) -> bool {{
    if matches!(auth_info.auth_mode, crate::middleware::AuthMode::ApiKey {{ .. }}) {{
        return true;
    }}
    const HIERARCHY: &[&str] = &[{hierarchy_list}];
    let Some(min_rank) = HIERARCHY.iter().position(|r| *r == "manager") else {{
        return false;
    }};
    HIERARCHY
        .iter()
        .position(|r| *r == auth_info.role.as_str())
        .is_some_and(|rank| rank <= min_rank)
}}"#
        ),
        guard_block: r#"
    // `?include_deleted=true` needs a `manager`-or-better role (issue #792);
    // API-key sessions pass (their scopes enforce at the DB tier). Absent or
    // `false` is the default read and needs no gate.
    if params.include_deleted == Some(true) && !include_deleted_role_allows(&api_key_info) {
        return Err(AppError::forbidden(
            "?include_deleted=true requires a manager-or-better role",
        )
        .with_correlation_id(correlation_id));
    }"#
        .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegraph_config::config::default_roles_hierarchy;

    fn hierarchy(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    /// owner/manager pass, member/employee/unknown refuse — the default
    /// hierarchy.
    #[test]
    fn default_hierarchy_ranks_the_manager_minimum() {
        let h = default_roles_hierarchy();
        assert_eq!(h, hierarchy(&["owner", "manager", "member", "employee"]));

        assert!(
            include_deleted_role_allows("owner", &h),
            "owner outranks manager"
        );
        assert!(
            include_deleted_role_allows("manager", &h),
            "manager itself passes (manager-or-better)"
        );
        assert!(
            !include_deleted_role_allows("member", &h),
            "member ranks below manager"
        );
        assert!(
            !include_deleted_role_allows("employee", &h),
            "employee ranks below manager"
        );
        assert!(
            !include_deleted_role_allows("api_key", &h),
            "unknown roles refuse (the emitted helper bypasses them explicitly, the spec refuses)"
        );
        assert!(!include_deleted_role_allows("", &h));
    }

    /// A custom hierarchy ranks by ITS order; `manager` names the minimum.
    #[test]
    fn custom_hierarchies_rank_by_their_own_order() {
        let h = hierarchy(&["admin", "manager", "clerk"]);
        assert!(include_deleted_role_allows("admin", &h));
        assert!(include_deleted_role_allows("manager", &h));
        assert!(!include_deleted_role_allows("clerk", &h));
        assert!(
            !include_deleted_role_allows("owner", &h),
            "not in this hierarchy"
        );
    }

    /// A hierarchy without `manager` refuses EVERY role — fail closed.
    #[test]
    fn hierarchy_without_manager_fails_closed() {
        let h = hierarchy(&["owner", "admin", "member"]);
        for role in ["owner", "admin", "member", "manager", "api_key"] {
            assert!(
                !include_deleted_role_allows(role, &h),
                "{role} must refuse: no `manager` in the hierarchy"
            );
        }
        let empty: Vec<String> = Vec::new();
        assert!(!include_deleted_role_allows("owner", &empty));
    }

    /// The emitted helper embeds the configured hierarchy and the same
    /// rank comparison the spec fn carries, plus the api-key bypass.
    #[test]
    fn emitted_helper_embeds_the_hierarchy_and_the_bypass() {
        let gate = build_include_deleted_gate(true, true, false, false, &default_roles_hierarchy());
        assert!(
            gate.helper_fn.contains(
                r#"const HIERARCHY: &[&str] = &["owner", "manager", "member", "employee"];"#
            ),
            "the hierarchy rides the emitted const:\n{}",
            gate.helper_fn
        );
        assert!(
            gate.helper_fn
                .contains("crate::middleware::AuthMode::ApiKey"),
            "the api-key bypass rides the emitted helper:\n{}",
            gate.helper_fn
        );
        assert!(
            gate.helper_fn
                .contains("is_some_and(|rank| rank <= min_rank)"),
            "the same rank comparison as the spec fn:\n{}",
            gate.helper_fn
        );
    }

    /// The guard block names the requirement and consults the helper.
    #[test]
    fn guard_block_names_the_manager_minimum() {
        let gate = build_include_deleted_gate(true, true, true, false, &default_roles_hierarchy());
        assert!(
            gate.guard_block
                .contains("params.include_deleted == Some(true)"),
            "fires only on an explicit true:\n{}",
            gate.guard_block
        );
        assert!(
            gate.guard_block
                .contains("include_deleted_role_allows(&api_key_info)"),
            "consults the emitted helper:\n{}",
            gate.guard_block
        );
        assert!(
            gate.guard_block
                .contains("requires a manager-or-better role"),
            "the 403 names the `manager` minimum:\n{}",
            gate.guard_block
        );
        assert!(
            gate.guard_block.starts_with('\n') && !gate.guard_block.ends_with('\n'),
            "the #338 block convention: leading newline, no trailing newline"
        );
    }

    /// Inactive shapes render empty: non-auditable, and auditable entities
    /// with no GET/LIST/SEARCH surface (the helper would be dead code and
    /// the guard would reference an unbound `params`).
    #[test]
    fn inactive_shapes_render_nothing() {
        for (is_auditable, has_read, has_list, has_fts) in
            [(false, true, true, true), (true, false, false, false)]
        {
            let gate = build_include_deleted_gate(
                is_auditable,
                has_read,
                has_list,
                has_fts,
                &default_roles_hierarchy(),
            );
            assert!(
                gate.helper_fn.is_empty() && gate.guard_block.is_empty(),
                "gate must be inactive for auditable={is_auditable} read={has_read} list={has_list} fts={has_fts}"
            );
        }
    }

    /// A custom hierarchy (including one without `manager`) renders into the
    /// emitted const verbatim.
    #[test]
    fn custom_hierarchy_renders_into_the_emitted_const() {
        let gate = build_include_deleted_gate(
            true,
            true,
            false,
            false,
            &hierarchy(&["admin", "manager", "clerk"]),
        );
        assert!(
            gate.helper_fn
                .contains(r#"const HIERARCHY: &[&str] = &["admin", "manager", "clerk"];"#),
            ":\n{}",
            gate.helper_fn
        );
        // The `manager`-less hierarchy still emits the helper — the emitted
        // `let Some(...) else` arm fails closed at runtime.
        let closed =
            build_include_deleted_gate(true, true, false, false, &hierarchy(&["owner", "admin"]));
        assert!(
            closed.helper_fn.contains("return false;"),
            "the no-manager helper keeps the fail-closed arm:\n{}",
            closed.helper_fn
        );
    }
}
