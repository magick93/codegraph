//! `?include=` hydration lowering (issue #781, wave 2 / T2.1).
//!
//! The handler template used to branch inline on
//! `project.persistence_provider` to decide whether `?include=` hydration is
//! available (`handler.tera`'s two provider branches: GET-by-id and LIST).
//! This module makes that decision in Rust and pre-renders the provider's
//! hydration block so the template renders one already-computed fragment.
//!
//! The SeaORM blocks are a verbatim port of the template's SeaORM branches
//! (including comments and the dot-notation accumulation merge).
//!
//! ## Cornucopia hydration (issue #781 P1, default ON)
//!
//! The cornucopia SQL-first repositories now DRIVE include hydration
//! ([`IncludeSupport::CornucopiaHydration`]) for a cornucopia project, with
//! no preview switch. Its verdict is **per path**:
//! [`CornucopiaIncludeShape::classify`] accepts the shapes the
//! SQL/adapter/query planes implement (a single-segment scalar `refers`, a
//! single-segment reverse-FK collection, and one hop of dot notation) and
//! names the construct for every other shape, so an unimplemented shape still
//! fails closed (a guarded 501 naming the alias) instead of silently
//! returning empty data. The rendered block calls the entity's cornucopia
//! QUERY handler (the provider's session/transaction owner), mirroring the
//! tree handler's call shape — never the adapter directly.
//!
//! [`IncludeSupport::Unsupported`] remains for a persistence provider that
//! genuinely cannot hydrate includes; no production provider does (both
//! variants are covered), so it is reachable only through
//! [`build_include_plan_with_support`].
//!
//! The design record is `docs/tdd/781-cornucopia-includes.tdd.md`.
//!
//! ## What the blocks do NOT contain
//!
//! The template keeps the parts that are provider-independent and must run
//! for every provider:
//!
//! * GET — the `let include_paths: Vec<String> = params.include…` parsing,
//!   the `ALLOWED_INCLUDE_KEYS` / max-depth validation loops, and the
//!   `let mut included = {Entity}IncludedData { … };` initialisation.
//! * LIST — the `let include_paths: Vec<String> = params.include…` parsing
//!   and the `ALLOWED_INCLUDE_KEYS` validation loop (hoisted out of the
//!   provider branch when the block is wired).
//!
//! ## Whitespace contract
//!
//! The blocks are inserted into a generated Rust file, so they are
//! whitespace-exact:
//!
//! * the first line carries the handler-fn-body indentation (4 spaces), and
//!   nested lines carry the indentation the template branch emitted;
//! * a block has NO trailing newline (the template supplies the line break,
//!   matching the `GuardBlocks` / invariant-block convention);
//! * the Tera tag artifacts of the old inline branches (the blank lines the
//!   `{% if %}` / `{% for %}` lines produced in the rendered output) are not
//!   reproduced — the wiring task deliberately emits clean Rust, so its
//!   fixture re-bless is enumerated.

use crate::api::include_path::ResolvedIncludePath;
use crate::project_config::ProjectConfig;

/// Which persistence provider backed the pre-rendered hydration blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncludeSupport {
    /// The SeaORM repository plane: per-id fetch on GET, batch fetch on LIST.
    SeaOrm,
    /// The cornucopia SQL-first plane (issue #781 P1, default ON).
    ///
    /// Support is PER PATH: [`CornucopiaIncludeShape::classify`] decides each
    /// modelled path independently, so the shapes the SQL/adapter/query planes
    /// implement hydrate while every other shape renders a guarded 501 naming
    /// the alias. Never a silent empty include.
    CornucopiaHydration,
    /// The provider cannot hydrate `?include=` at all: a request that asks
    /// for includes fails closed with a 501 naming the provider and issue
    /// #781. A request WITHOUT includes proceeds normally.
    Unsupported { reason: String },
}

/// The per-path verdict for one modelled include path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncludePathSupport {
    /// The provider renders real hydration code for this path.
    Supported,
    /// This path's shape is not implemented on the provider plane — the
    /// handler must answer a guarded 501 naming the alias and this reason.
    Unsupported { reason: String },
}

impl IncludePathSupport {
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Supported)
    }
}

/// The cornucopia include shapes the SQL/adapter/query planes implement
/// (issue #781 P1). Adding a variant is how a shape becomes hydratable: the
/// classifier, the three emitters, and the KATs move together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CornucopiaIncludeShape {
    /// GET: one row on the target table through the source's own FK column.
    /// LIST: the same relationship keyed by `source_ids` in one query.
    SingleScalar,
    /// GET: every target row whose reverse FK points at the source
    /// (one-to-many — the source holds a collection). LIST: the same rows for
    /// many sources in one query, grouped by source id in the adapter.
    ///
    /// The reverse FK is the column the resolver already carries
    /// ([`crate::api::include_path::IncludeSegment::reverse_fk_column`]); the
    /// resolver refuses a junction array before it reaches this plane, so an
    /// array that survives carries a real back-FK column.
    SingleArray,
    /// One hop of dot notation — `customer.org`: the target's own scalar FK to
    /// a leaf entity.
    ///
    /// GET: the leaf row addressed through the INTERMEDIATE's FK to it
    /// (`WHERE intermediate.{leaf_fk} = leaf.id`, joined back to the source
    /// relationship), so the combined response carries
    /// `Option<{Leaf}Response>`. LIST: the same hop for many source ids in one
    /// query, keyed by SOURCE id (the `included.<first>` envelope's map key),
    /// exactly like the SeaORM dot batch.
    ///
    /// Depth is exactly two. The first hop must be scalar (the combined
    /// response carries ONE leaf field — a collection first hop would need a
    /// different contract) and the leaf must be a scalar FK hop whose column
    /// the resolver already verified. Everything else keeps its named refusal.
    Dot,
}

impl CornucopiaIncludeShape {
    /// Classify one resolved include path. `Err` carries the human reason the
    /// handler puts in its 501 and the emitters put in their WARN/omission.
    pub fn classify(path: &ResolvedIncludePath) -> Result<Self, String> {
        let Some(first) = path.segments.first() else {
            return Err("the path resolves to no segment".to_string());
        };
        if path.segments.len() > 2 {
            return Err(
                "dot-notation includes deeper than one hop (a target's child's child) are \
                 not implemented on the cornucopia SQL-first plane yet"
                    .to_string(),
            );
        }
        if path.segments.len() == 2 {
            let leaf = &path.segments[1];
            if first.child_table_override.is_some() || leaf.child_table_override.is_some() {
                return Err(
                    "VO→entity child-table includes are not implemented on the cornucopia \
                     SQL-first plane yet"
                        .to_string(),
                );
            }
            if first.is_array {
                return Err(
                    "dot-notation includes whose first hop is a collection are not \
                     implemented on the cornucopia SQL-first plane yet"
                        .to_string(),
                );
            }
            if leaf.is_array {
                return Err(
                    "dot-notation includes whose leaf is a collection are not implemented \
                     on the cornucopia SQL-first plane yet"
                        .to_string(),
                );
            }
            if leaf.fk_column.is_empty() {
                return Err(
                    "dot-notation include carries no FK column on the first hop to address \
                     its leaf by"
                        .to_string(),
                );
            }
            return Ok(Self::Dot);
        }
        if first.child_table_override.is_some() {
            return Err(
                "VO→entity child-table includes are not implemented on the cornucopia \
                 SQL-first plane yet"
                    .to_string(),
            );
        }
        if first.is_array {
            // The array query scans the TARGET by its reverse FK; a path with
            // no resolvable back-FK column is unfetchable (the resolver drops
            // junction arrays before this point, so this arm is defensive).
            if first.reverse_fk_column.is_empty() {
                return Err(
                    "array / reverse-FK include carries no back-FK column to scan the \
                     target by"
                        .to_string(),
                );
            }
            return Ok(Self::SingleArray);
        }
        Ok(Self::SingleScalar)
    }

    /// GET: does the hydrated field hold a collection? The DTO's include field
    /// is `Option<Vec<{Target}Response>>` for an array path and
    /// `Option<{Target}Response>` for a scalar one (`ddd::dto`), so the
    /// assignment differs by exactly this bit.
    pub fn is_array(self) -> bool {
        matches!(self, Self::SingleArray)
    }
}

impl IncludeSupport {
    /// True when the provider renders real hydration code (for at least the
    /// shapes it supports — see [`Self::path_support`]).
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::SeaOrm | Self::CornucopiaHydration)
    }

    /// The per-path verdict this provider gives `path`. The ONE predicate the
    /// handler plan, the three cornucopia emitters, and the generated hurl
    /// contract all read, so a contract can never probe a surface the
    /// handler refuses.
    pub fn path_support(&self, path: &ResolvedIncludePath) -> IncludePathSupport {
        match self {
            // SeaORM hydrates every resolved path.
            Self::SeaOrm => IncludePathSupport::Supported,
            Self::CornucopiaHydration => match CornucopiaIncludeShape::classify(path) {
                Ok(_) => IncludePathSupport::Supported,
                Err(reason) => IncludePathSupport::Unsupported { reason },
            },
            Self::Unsupported { reason } => IncludePathSupport::Unsupported {
                reason: reason.clone(),
            },
        }
    }

    /// The aliases that must fail closed (with their reasons), in declaration
    /// order.
    pub fn unsupported_paths(&self, paths: &[ResolvedIncludePath]) -> Vec<UnsupportedInclude> {
        paths
            .iter()
            .filter_map(|path| match self.path_support(path) {
                IncludePathSupport::Supported => None,
                IncludePathSupport::Unsupported { reason } => Some(UnsupportedInclude {
                    alias: path.alias.clone(),
                    reason,
                }),
            })
            .collect()
    }

    /// The paths the provider hydrates, in declaration order.
    pub fn supported_paths<'a>(
        &self,
        paths: &'a [ResolvedIncludePath],
    ) -> Vec<&'a ResolvedIncludePath> {
        paths
            .iter()
            .filter(|path| self.path_support(path).is_supported())
            .collect()
    }
}

/// A modelled include path whose SHAPE the provider plane does not implement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedInclude {
    /// The dot-notation alias exactly as the request spells it.
    pub alias: String,
    /// Why the shape is not hydratable (rendered into the 501 message).
    pub reason: String,
}

/// The pre-rendered `?include=` hydration blocks for one entity handler.
#[derive(Debug, Clone)]
pub struct IncludePlan {
    /// Which provider's blocks were rendered.
    pub support: IncludeSupport,
    /// The exact Rust lines the GET-by-id handler renders for include
    /// hydration (the provider-selected block, no surrounding template
    /// conditionals, no trailing newline). Empty when no include paths are
    /// modelled — the handler needs no hydration.
    pub get_block: String,
    /// The exact Rust lines the LIST handler renders for include hydration.
    /// Empty when no include paths are modelled.
    pub list_block: String,
    /// The Rust doc-comment LINE (leading `    /// ` included) that replaces
    /// the `?include=` description on the shared query-params struct. Empty
    /// when no include paths are modelled. SeaORM keeps the historical
    /// "eager-load …" line byte-for-byte; an unsupported provider states
    /// that includes are modelled but answer 501 (issue #781).
    pub include_doc: String,
}

/// The handler context the hydration blocks are rendered against.
pub struct IncludePlanCtx<'a> {
    /// The entity's Rust type stem, e.g. `Worker` (the template's
    /// `entity_name`).
    pub entity_name: &'a str,
    /// The bounded-context domain key, e.g. `common`.
    pub domain: &'a str,
    /// The entity's Rust module name, e.g. `worker`.
    pub module_name: &'a str,
    /// The entity role (`root` / `child` / …). Carried for parity with the
    /// handler context; the current blocks do not branch on it.
    pub role: &'a str,
    /// Whether the entity is auditable. Carried for parity with the handler
    /// context; the current blocks do not branch on it.
    pub is_auditable: bool,
}

/// The provider's `?include=` support verdict, independent of any entity —
/// the ONE predicate every consumer reads (the handler's hydration plan and
/// the generated hurl contract's include legs), so a contract can never probe
/// a surface the handler refuses.
///
/// Cornucopia returns [`IncludeSupport::CornucopiaHydration`] (issue #781 P1
/// landed — includes hydrate by default), whose support is refined per path by
/// [`IncludeSupport::path_support`]; SeaORM hydrates every resolved path.
pub fn include_support_for(project: &ProjectConfig) -> IncludeSupport {
    if project.is_cornucopia() {
        IncludeSupport::CornucopiaHydration
    } else {
        IncludeSupport::SeaOrm
    }
}

/// Build the include plan for one entity, deriving provider support from
/// `project` (`PersistenceProvider` has exactly two variants today).
///
/// Both providers hydrate: SeaORM renders transaction-scoped repository
/// fetches, cornucopia renders per-path query-handler calls (issue #781 P1).
/// A cornucopia shape the SQL/adapter planes do not implement still fails
/// closed with a guarded 501 naming the alias — never silent empty data.
pub fn build_include_plan(
    paths: &[ResolvedIncludePath],
    ctx: &IncludePlanCtx<'_>,
    project: &ProjectConfig,
) -> IncludePlan {
    build_include_plan_with_support(include_support_for(project), paths, ctx)
}

/// [`build_include_plan`] with an explicit support verdict — the entry point
/// for a provider the caller cannot lower (see [`IncludeSupport::Unsupported`])
/// and for tests that inject a per-path verdict.
pub fn build_include_plan_with_support(
    support: IncludeSupport,
    paths: &[ResolvedIncludePath],
    ctx: &IncludePlanCtx<'_>,
) -> IncludePlan {
    if paths.is_empty() {
        // No modelled include paths ⇒ no hydration code at all. The provider
        // verdict is still reported so callers can assert the dependency.
        return IncludePlan {
            support,
            get_block: String::new(),
            list_block: String::new(),
            include_doc: String::new(),
        };
    }
    IncludePlan {
        get_block: render_get_block(paths, ctx, &support),
        list_block: render_list_block(paths, ctx, &support),
        include_doc: include_doc_for(&support).to_string(),
        support,
    }
}

// =============================================================================
// GET-by-id
// =============================================================================

fn render_get_block(
    paths: &[ResolvedIncludePath],
    ctx: &IncludePlanCtx<'_>,
    support: &IncludeSupport,
) -> String {
    match support {
        IncludeSupport::SeaOrm => render_get_sea_orm(paths, ctx),
        IncludeSupport::CornucopiaHydration => render_get_cornucopia(paths, ctx, support),
        IncludeSupport::Unsupported { reason } => render_unsupported_get_block(reason),
    }
}

/// The cornucopia GET block (issue #781 P1).
///
/// Per path: an unimplemented SHAPE answers a 501 naming the alias and the
/// construct; an implemented shape calls the entity's cornucopia QUERY
/// handler — the provider's session/transaction owner — instead of opening a
/// transaction in the handler the way the SeaORM arm does. The RLS session
/// vars therefore come from the query layer's `set_rls_session_vars`, exactly
/// like every other cornucopia read (the tree handler's call shape).
///
/// The array shape wraps the returned `Vec<{Target}Response>` in `Some(…)`
/// exactly like the SeaORM array branch: the DTO field is
/// `Option<Vec<{Target}Response>>`, and an empty collection is still a
/// PRESENT value (`[]`, not `null`) — which is what the generated hurl
/// contract asserts.
fn render_get_cornucopia(
    paths: &[ResolvedIncludePath],
    ctx: &IncludePlanCtx<'_>,
    support: &IncludeSupport,
) -> String {
    let supported = support.supported_paths(paths);
    let unsupported = support.unsupported_paths(paths);
    let error_type = cornucopia_error_type(ctx.domain);
    let queries = cornucopia_queries_handle(ctx);

    let mut out = String::new();
    out.push_str("    for path in &include_paths {\n");
    push_cornucopia_unsupported_guard(&mut out, &unsupported, "        ");
    if !supported.is_empty() {
        out.push_str("        match path.as_str() {\n");
        for ipath in &supported {
            let (Some(first), Ok(shape)) = (
                ipath.segments.first(),
                CornucopiaIncludeShape::classify(ipath),
            ) else {
                continue;
            };
            out.push_str(&format!(
                "            {} => {{\n",
                rust_string_literal(&ipath.alias)
            ));
            match shape {
                CornucopiaIncludeShape::SingleArray => {
                    out.push_str(&format!(
                        "                included.{} = Some({queries}.{}(id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await\n",
                        first.module_name, ipath.fetch_method
                    ));
                    push_cornucopia_map_err(&mut out, 20, &ipath.alias, &error_type);
                    out.push_str("                    })?);\n");
                }
                CornucopiaIncludeShape::SingleScalar => {
                    out.push_str(&format!(
                        "                included.{} = {queries}.{}(id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await\n",
                        first.module_name, ipath.fetch_method
                    ));
                    push_cornucopia_map_err(&mut out, 20, &ipath.alias, &error_type);
                    out.push_str("                    })?;\n");
                }
                CornucopiaIncludeShape::Dot => {
                    // Same merge the SeaORM dot arm performs: the combined
                    // response type carries one `Option<{Leaf}Response>` field
                    // per leaf, and sibling dot paths sharing the first hop
                    // accumulate instead of clobbering.
                    let leaf = &ipath.segments[1];
                    out.push_str(
                        "                // Dot-notation path: merge into combined response\n",
                    );
                    out.push_str(&format!(
                        "                let leaf_resp = {queries}.{}(id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await\n",
                        ipath.fetch_method
                    ));
                    push_cornucopia_map_err(&mut out, 20, &ipath.alias, &error_type);
                    out.push_str("                    })?;\n");
                    out.push_str("                if let Some(val) = leaf_resp {\n");
                    out.push_str(&format!(
                        "                    if let Some(ref mut combined) = included.{} {{\n",
                        first.module_name
                    ));
                    out.push_str(&format!(
                        "                        combined.{} = val.{};\n",
                        leaf.module_name, leaf.module_name
                    ));
                    out.push_str("                    } else {\n");
                    out.push_str(&format!(
                        "                        included.{} = Some({} {{\n",
                        first.module_name, ipath.response_rust_type
                    ));
                    out.push_str(&format!(
                        "                            {}: val.{},\n",
                        leaf.module_name, leaf.module_name
                    ));
                    out.push_str("                            ..Default::default()\n");
                    out.push_str("                        });\n");
                    out.push_str("                    }\n");
                    out.push_str("                }\n");
                }
            }
            out.push_str("            }\n");
        }
        out.push_str("            _ => {}\n");
        out.push_str("        }\n");
    }
    out.push_str("    }");
    out
}

/// The cornucopia LIST block (issue #781 P1): one batch query per
/// implemented path, keyed by source id in the same `HashMap` JSON shape the
/// SeaORM arm emits (so the DTO/included envelope is provider-identical).
fn render_list_cornucopia(
    paths: &[ResolvedIncludePath],
    ctx: &IncludePlanCtx<'_>,
    support: &IncludeSupport,
) -> String {
    let supported = support.supported_paths(paths);
    let unsupported = support.unsupported_paths(paths);
    let error_type = cornucopia_error_type(ctx.domain);
    let queries = cornucopia_queries_handle(ctx);

    let mut out = String::new();
    out.push_str("    for path in &include_paths {\n");
    push_cornucopia_unsupported_guard(&mut out, &unsupported, "        ");
    out.push_str("    }\n");

    out.push_str("    let included = if !include_paths.is_empty() {\n");
    out.push_str("        let ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();\n");
    out.push_str(
        "        let mut included_map = std::collections::HashMap::<String, serde_json::Value>::new();\n",
    );
    for ipath in &supported {
        let Some(first) = ipath.segments.first() else {
            continue;
        };
        let Ok(shape) = CornucopiaIncludeShape::classify(ipath) else {
            continue;
        };
        if shape == CornucopiaIncludeShape::Dot {
            // Same accumulation the SeaORM LIST dot arm performs: key the
            // first-hop entry by SOURCE id and insert each leaf under its
            // segment name, so sibling dot paths sharing the first hop merge
            // instead of clobbering each other.
            let leaf = &ipath.segments[1];
            out.push_str(&format!(
                "        if include_paths.contains(&\"{}\".to_string()) {{\n",
                ipath.alias
            ));
            out.push_str(&format!(
                "            let batch = {queries}.{}(&ids, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await\n",
                ipath.batch_fetch_method
            ));
            push_cornucopia_map_err(&mut out, 16, &ipath.alias, &error_type);
            out.push_str("                })?;\n");
            out.push_str("            let entry = included_map\n");
            out.push_str(&format!(
                "                .entry(\"{}\".to_string())\n",
                first.module_name
            ));
            out.push_str("                .or_insert_with(|| serde_json::json!({}));\n");
            out.push_str("            if let Some(by_source) = entry.as_object_mut() {\n");
            out.push_str("                for (source_id, combined) in batch {\n");
            out.push_str(&format!(
                "                    let leaf = serde_json::json!(combined.{});\n",
                leaf.module_name
            ));
            out.push_str("                    let leaf_entry = by_source\n");
            out.push_str("                        .entry(source_id.to_string())\n");
            out.push_str("                        .or_insert_with(|| serde_json::json!({}));\n");
            out.push_str(
                "                    if let Some(leaf_obj) = leaf_entry.as_object_mut() {\n",
            );
            out.push_str("                        leaf_obj.insert(\n");
            out.push_str(&format!(
                "                            \"{}\".to_string(),\n",
                leaf.module_name
            ));
            out.push_str("                            leaf,\n");
            out.push_str("                        );\n");
            out.push_str("                    }\n");
            out.push_str("                }\n");
            out.push_str("            }\n");
            out.push_str("        }\n");
            continue;
        }
        out.push_str(&format!(
            "        if include_paths.contains(&\"{}\".to_string()) {{\n",
            ipath.alias
        ));
        out.push_str("            included_map.insert(\n");
        out.push_str(&format!(
            "                \"{}\".to_string(),\n",
            first.module_name
        ));
        out.push_str(&format!(
            "                serde_json::to_value({queries}.{}(&ids, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await\n",
            ipath.batch_fetch_method
        ));
        push_cornucopia_map_err(&mut out, 20, &ipath.alias, &error_type);
        out.push_str("                    })?).unwrap_or_default(),\n");
        out.push_str("            );\n");
        out.push_str("        }\n");
    }
    out.push_str("        included_map\n");
    out.push_str("    } else {\n");
    out.push_str("        std::collections::HashMap::new()\n");
    out.push_str("    };");
    out
}

/// `state.{domain}_{module}_queries` — the entity's cornucopia query handler.
fn cornucopia_queries_handle(ctx: &IncludePlanCtx<'_>) -> String {
    format!("state.{}_{}_queries", ctx.domain, ctx.module_name)
}

/// `{Domain}Error` — the generated domain error type the query layer returns.
fn cornucopia_error_type(domain: &str) -> String {
    format!("{}Error", codegraph_naming::to_pascal_case(domain))
}

/// The `.map_err(|e: {Domain}Error| AppError::from_domain_error(…))` block the
/// query-layer call maps through (the provider's `from_repo_err` classifier
/// turns a scope/RLS denial into 403). Ends with the `.with_correlation_id(…)`
/// line; the caller closes the closure block with its own trailing
/// punctuation (`})?;` / `}).unwrap_or_default(),`).
fn push_cornucopia_map_err(out: &mut String, indent: usize, alias: &str, error_type: &str) {
    let pad = " ".repeat(indent);
    out.push_str(&format!("{pad}.map_err(|e: {error_type}| {{\n"));
    out.push_str(&format!(
        "{pad}    AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to include {alias}\")\n"
    ));
    out.push_str(&format!(
        "{pad}        .with_correlation_id(correlation_id)\n"
    ));
}

/// The per-path fail-closed guard for shapes the cornucopia plane does not
/// implement: only the REQUESTED alias trips it, so a request that asks for an
/// implemented path is unaffected. Emitted only when some modelled path is
/// unsupported.
fn push_cornucopia_unsupported_guard(
    out: &mut String,
    unsupported: &[UnsupportedInclude],
    pad: &str,
) {
    if unsupported.is_empty() {
        return;
    }
    out.push_str(&format!("{pad}let unsupported = match path.as_str() {{\n"));
    for entry in unsupported {
        out.push_str(&format!(
            "{pad}    {} => Some({}),\n",
            rust_string_literal(&entry.alias),
            rust_string_literal(&entry.reason)
        ));
    }
    out.push_str(&format!("{pad}    _ => None,\n"));
    out.push_str(&format!("{pad}}};\n"));
    out.push_str(&format!("{pad}if let Some(reason) = unsupported {{\n"));
    out.push_str(&format!(
        "{pad}    // This include's SHAPE is not implemented on the cornucopia plane\n"
    ));
    out.push_str(&format!(
        "{pad}    // (issue #781 P1): fail closed instead of returning empty data.\n"
    ));
    out.push_str(&format!("{pad}    return Err(AppError {{\n"));
    out.push_str(&format!(
        "{pad}        status: axum::http::StatusCode::NOT_IMPLEMENTED,\n"
    ));
    out.push_str(&format!("{pad}        code: \"NOT_IMPLEMENTED\",\n"));
    out.push_str(&format!(
        "{pad}        message: format!(\"?include={{path}} is not supported: {{reason}} (issue #781)\"),\n"
    ));
    out.push_str(&format!("{pad}        details: None,\n"));
    out.push_str(&format!(
        "{pad}        correlation_id: Some(correlation_id),\n"
    ));
    out.push_str(&format!("{pad}    }});\n"));
    out.push_str(&format!("{pad}}}\n"));
}

fn render_get_sea_orm(paths: &[ResolvedIncludePath], ctx: &IncludePlanCtx<'_>) -> String {
    let mut out = String::new();
    out.push_str("    use sea_orm::TransactionTrait;\n");
    out.push_str("    let tx = state.db.begin().await\n");
    out.push_str(
        "        .map_err(|e| AppError::internal(format!(\"Failed to begin transaction: {e}\"))\n",
    );
    out.push_str("            .with_correlation_id(correlation_id))?;\n");
    out.push_str(&format!(
        "    let repo = crate::domain::{}::{}::repository_impl::{}RepositoryImpl;\n",
        ctx.domain, ctx.module_name, ctx.entity_name
    ));
    out.push_str("    for path in &include_paths {\n");
    out.push_str("        match path.as_str() {\n");
    for ipath in paths {
        let Some(first) = ipath.segments.first() else {
            continue;
        };
        out.push_str(&format!("            \"{}\" => {{\n", ipath.alias));
        if ipath.segments.len() == 1 {
            if first.is_array {
                out.push_str(&format!(
                    "                included.{} = Some(repo.{}(&tx, id).await\n",
                    first.module_name, ipath.fetch_method
                ));
                out.push_str(&map_err_line(20, &ipath.alias));
                out.push_str("                        .with_correlation_id(correlation_id))?);\n");
            } else {
                out.push_str(&format!(
                    "                included.{} = repo.{}(&tx, id).await\n",
                    first.module_name, ipath.fetch_method
                ));
                out.push_str(&map_err_line(20, &ipath.alias));
                out.push_str("                        .with_correlation_id(correlation_id))?;\n");
            }
        } else {
            // The template merges only the first two segments (depth > 2
            // paths keep the same leaf); ported verbatim.
            let leaf = &ipath.segments[1];
            out.push_str("                // Dot-notation path: merge into combined response\n");
            out.push_str(&format!(
                "                let leaf_resp = repo.{}(&tx, id).await\n",
                ipath.fetch_method
            ));
            out.push_str(&map_err_line(20, &ipath.alias));
            out.push_str("                        .with_correlation_id(correlation_id))?;\n");
            out.push_str("                if let Some(val) = leaf_resp {\n");
            out.push_str(&format!(
                "                    if let Some(ref mut combined) = included.{} {{\n",
                first.module_name
            ));
            out.push_str(&format!(
                "                        combined.{} = val.{};\n",
                leaf.module_name, leaf.module_name
            ));
            out.push_str("                    } else {\n");
            out.push_str(&format!(
                "                        included.{} = Some({} {{\n",
                first.module_name, ipath.response_rust_type
            ));
            out.push_str(&format!(
                "                            {}: val.{},\n",
                leaf.module_name, leaf.module_name
            ));
            out.push_str("                            ..Default::default()\n");
            out.push_str("                        });\n");
            out.push_str("                    }\n");
            out.push_str("                }\n");
        }
        out.push_str("            }\n");
    }
    out.push_str("            _ => {}\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("    tx.commit().await\n");
    out.push_str(
        "        .map_err(|e| AppError::internal(format!(\"Failed to commit transaction: {e}\"))\n",
    );
    out.push_str("            .with_correlation_id(correlation_id))?;");
    out
}

// =============================================================================
// LIST
// =============================================================================

fn render_list_block(
    paths: &[ResolvedIncludePath],
    ctx: &IncludePlanCtx<'_>,
    support: &IncludeSupport,
) -> String {
    match support {
        IncludeSupport::SeaOrm => render_list_sea_orm(paths, ctx),
        IncludeSupport::CornucopiaHydration => render_list_cornucopia(paths, ctx, support),
        IncludeSupport::Unsupported { reason } => render_unsupported_list_block(reason),
    }
}

fn render_list_sea_orm(paths: &[ResolvedIncludePath], ctx: &IncludePlanCtx<'_>) -> String {
    let mut out = String::new();
    out.push_str("    let included = if !include_paths.is_empty() {\n");
    out.push_str("        use sea_orm::TransactionTrait;\n");
    out.push_str("        let tx = state.db.begin().await\n");
    out.push_str(
        "            .map_err(|e| AppError::internal(format!(\"Failed to begin transaction for includes: {e}\"))\n",
    );
    out.push_str("                .with_correlation_id(correlation_id))?;\n");
    out.push_str(&format!(
        "        let repo = crate::domain::{}::{}::repository_impl::{}RepositoryImpl;\n",
        ctx.domain, ctx.module_name, ctx.entity_name
    ));
    out.push_str("        let ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();\n");
    out.push_str(
        "        let mut included_map = std::collections::HashMap::<String, serde_json::Value>::new();\n",
    );
    // NOTE: the template's `ALLOWED_INCLUDE_KEYS` loop is NOT part of the
    // block — the wiring task keeps it (provider-independent validation).
    for ipath in paths {
        let Some(first) = ipath.segments.first() else {
            continue;
        };
        if ipath.segments.len() == 1 {
            out.push_str(&format!(
                "        if include_paths.contains(&\"{}\".to_string()) {{\n",
                ipath.alias
            ));
            out.push_str("            included_map.insert(\n");
            out.push_str(&format!(
                "                \"{}\".to_string(),\n",
                first.module_name
            ));
            out.push_str(&format!(
                "                serde_json::to_value(repo.{}(&tx, &ids).await\n",
                ipath.batch_fetch_method
            ));
            out.push_str(&map_err_line(20, &ipath.alias));
            out.push_str(
                "                        .with_correlation_id(correlation_id))?).unwrap_or_default(),\n",
            );
            out.push_str("            );\n");
            out.push_str("        }\n");
        } else {
            let leaf = &ipath.segments[1];
            out.push_str(
                "        // Dot-notation path: batch-fetch the combined struct per source id and\n",
            );
            out.push_str(
                "        // merge each leaf field into `included.<segments[0]>[<source_id>]` so\n",
            );
            out.push_str(
                "        // sibling dot paths sharing the first segment accumulate instead of\n",
            );
            out.push_str("        // clobbering each other.\n");
            out.push_str(&format!(
                "        if include_paths.contains(&\"{}\".to_string()) {{\n",
                ipath.alias
            ));
            out.push_str(&format!(
                "            let batch = repo.{}(&tx, &ids).await\n",
                ipath.batch_fetch_method
            ));
            out.push_str(&map_err_line(16, &ipath.alias));
            out.push_str("                    .with_correlation_id(correlation_id))?;\n");
            out.push_str("            let entry = included_map\n");
            out.push_str(&format!(
                "                .entry(\"{}\".to_string())\n",
                first.module_name
            ));
            out.push_str("                .or_insert_with(|| serde_json::json!({}));\n");
            out.push_str("            if let Some(by_source) = entry.as_object_mut() {\n");
            out.push_str("                for (source_id, combined) in batch {\n");
            out.push_str(&format!(
                "                    let leaf = serde_json::json!(combined.{});\n",
                leaf.module_name
            ));
            out.push_str("                    let leaf_entry = by_source\n");
            out.push_str("                        .entry(source_id.to_string())\n");
            out.push_str("                        .or_insert_with(|| serde_json::json!({}));\n");
            out.push_str(
                "                    if let Some(leaf_obj) = leaf_entry.as_object_mut() {\n",
            );
            out.push_str("                        leaf_obj.insert(\n");
            out.push_str(&format!(
                "                            \"{}\".to_string(),\n",
                leaf.module_name
            ));
            out.push_str("                            leaf,\n");
            out.push_str("                        );\n");
            out.push_str("                    }\n");
            out.push_str("                }\n");
            out.push_str("            }\n");
            out.push_str("        }\n");
        }
    }
    out.push_str("        tx.commit().await\n");
    out.push_str(
        "            .map_err(|e| AppError::internal(format!(\"Failed to commit transaction: {e}\"))\n",
    );
    out.push_str("                .with_correlation_id(correlation_id))?;\n");
    out.push_str("        included_map\n");
    out.push_str("    } else {\n");
    out.push_str("        std::collections::HashMap::new()\n");
    out.push_str("    };");
    out
}

// =============================================================================
// Provider branches without hydration / without support
// =============================================================================

/// A fail-closed 501 (issue #781): a provider that cannot hydrate includes
/// must never answer with silently-empty include data.
///
/// The block lives in the handler's `has_include` region, which is emitted
/// whenever the ENTITY models include paths — so it must be GUARDED on the
/// request actually asking for some: a request without `?include=` proceeds
/// normally.
fn render_unsupported_get_block(reason: &str) -> String {
    let mut out = render_unsupported_guard(reason);
    out.push_str("\n    }");
    out
}

/// The LIST arm: same guard, then the empty map the Ok envelope always
/// references (a request with includes never reaches it — it returned 501).
fn render_unsupported_list_block(reason: &str) -> String {
    let mut out = render_unsupported_guard(reason);
    out.push_str("\n    }");
    out.push_str(
        "\n    let included = std::collections::HashMap::<String, serde_json::Value>::new();",
    );
    out
}

/// The shared guarded 501 body: `if !include_paths.is_empty() { … return
/// Err(501) … }` at the handler-fn-body indent, WITHOUT the closing brace
/// (the caller appends it, plus any provider-specific tail).
fn render_unsupported_guard(reason: &str) -> String {
    // The reason names issue #781 already for the cornucopia verdict; keep
    // the reference exactly once for both paths.
    let message = if reason.contains("#781") {
        format!("?include= hydration is not supported: {reason}")
    } else {
        format!("?include= hydration is not supported: {reason} (issue #781)")
    };
    let mut out = String::new();
    out.push_str("    if !include_paths.is_empty() {\n");
    out.push_str(
        "        // `?include=` hydration is not supported by this persistence provider\n",
    );
    out.push_str(
        "        // (issue #781): fail closed instead of silently returning empty data.\n",
    );
    out.push_str("        // A request without includes proceeds normally.\n");
    out.push_str("        return Err(AppError {\n");
    out.push_str("            status: axum::http::StatusCode::NOT_IMPLEMENTED,\n");
    out.push_str("            code: \"NOT_IMPLEMENTED\",\n");
    out.push_str(&format!(
        "            message: {}.to_string(),\n",
        rust_string_literal(&message)
    ));
    out.push_str("            details: None,\n");
    out.push_str("            correlation_id: Some(correlation_id),\n");
    out.push_str("        });");
    out
}

/// The `?include=` doc-comment line rendered into the query-params struct.
///
/// * SeaORM: the pre-#781 line, byte-exact.
/// * Cornucopia: eager-loading works for the implemented shapes; the wording
///   names the residual limit honestly.
/// * Unsupported: honest — includes are modelled but the deployment's
///   persistence provider cannot serve them, so requests answer 501.
fn include_doc_for(support: &IncludeSupport) -> &'static str {
    match support {
        IncludeSupport::SeaOrm => {
            "    /// Eager-load related entities, e.g. `?include=person,deployment.position`."
        }
        IncludeSupport::CornucopiaHydration => {
            "    /// Eager-load related entities. The cornucopia persistence provider hydrates scalar `refers` includes, reverse-FK collections, and one-hop dot-notation includes; any other modelled shape answers 501 (issue #781)."
        }
        IncludeSupport::Unsupported { .. } => {
            "    /// Includes are modelled for this entity but are not supported by this deployment's persistence provider; a request that sends `?include=` answers 501 (issue #781)."
        }
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// The shared `.map_err(|e| AppError::internal(format!("Failed to include
/// {alias}: {e}"))` line at `indent` spaces.
fn map_err_line(indent: usize, alias: &str) -> String {
    format!(
        "{}.map_err(|e| AppError::internal(format!(\"Failed to include {}: {{e}}\"))\n",
        " ".repeat(indent),
        alias
    )
}

/// Escape a value for embedding in a generated Rust string literal.
fn rust_string_literal(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::include_path::IncludeSegment;

    fn segment(module_name: &str, is_array: bool) -> IncludeSegment {
        IncludeSegment {
            entity_name: module_name.to_string(),
            schema_title: format!("{module_name}Type"),
            module_name: module_name.to_string(),
            domain: "common".to_string(),
            table: format!("\"common\".\"{module_name}\""),
            fk_column: format!("{module_name}_id"),
            reverse_fk_column: "worker_id".to_string(),
            fk_is_required: false,
            reverse_fk_is_required: false,
            is_array,
            child_table_override: None,
        }
    }

    fn path(alias: &str, segments: Vec<IncludeSegment>) -> ResolvedIncludePath {
        let snake = alias.replace('.', "_");
        ResolvedIncludePath {
            alias: alias.to_string(),
            segments,
            response_rust_type: format!("{}CombinedResponse", pascal(alias)),
            fetch_method: format!("fetch_{snake}_for_worker"),
            batch_fetch_method: format!("fetch_{snake}_batch_for_worker"),
        }
    }

    fn pascal(alias: &str) -> String {
        alias
            .split('.')
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect()
    }

    fn ctx() -> IncludePlanCtx<'static> {
        IncludePlanCtx {
            entity_name: "Worker",
            domain: "common",
            module_name: "worker",
            role: "root",
            is_auditable: true,
        }
    }

    fn cornucopia_project() -> ProjectConfig {
        use crate::profile::PersistenceProvider;

        ProjectConfig {
            database: crate::project_config::DatabaseConfig {
                persistence_provider: PersistenceProvider::Cornucopia,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn sea_orm_plan(paths: &[ResolvedIncludePath]) -> IncludePlan {
        build_include_plan(paths, &ctx(), &ProjectConfig::default())
    }

    // ── GET: one-segment scalar ──────────────────────────────────────────

    #[test]
    fn get_single_scalar_path_uses_the_per_id_fetch() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let plan = sea_orm_plan(&paths);
        assert!(plan.support.is_supported());
        assert!(
            plan.get_block
                .contains("included.person = repo.fetch_person_for_worker(&tx, id).await"),
            "{}",
            plan.get_block
        );
        assert!(plan.get_block.contains(
            ".map_err(|e| AppError::internal(format!(\"Failed to include person: {e}\"))"
        ));
        // No batch fetch / map on the GET arm.
        assert!(!plan.get_block.contains("included_map"));
        assert!(!plan.get_block.contains("batch_for_worker"));
        // No trailing newline (the template supplies the line break).
        assert!(!plan.get_block.ends_with('\n'));
    }

    #[test]
    fn get_single_scalar_path_block_is_exact() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let plan = sea_orm_plan(&paths);
        let expected = [
            "    use sea_orm::TransactionTrait;",
            "    let tx = state.db.begin().await",
            "        .map_err(|e| AppError::internal(format!(\"Failed to begin transaction: {e}\"))",
            "            .with_correlation_id(correlation_id))?;",
            "    let repo = crate::domain::common::worker::repository_impl::WorkerRepositoryImpl;",
            "    for path in &include_paths {",
            "        match path.as_str() {",
            "            \"person\" => {",
            "                included.person = repo.fetch_person_for_worker(&tx, id).await",
            "                    .map_err(|e| AppError::internal(format!(\"Failed to include person: {e}\"))",
            "                        .with_correlation_id(correlation_id))?;",
            "            }",
            "            _ => {}",
            "        }",
            "    }",
            "    tx.commit().await",
            "        .map_err(|e| AppError::internal(format!(\"Failed to commit transaction: {e}\"))",
            "            .with_correlation_id(correlation_id))?;",
        ]
        .join("\n");
        assert_eq!(plan.get_block, expected);
    }

    // ── GET: one-segment array ──────────────────────────────────────────

    #[test]
    fn get_single_array_path_wraps_the_fetch_in_some() {
        let paths = vec![path("deployment", vec![segment("deployment", true)])];
        let plan = sea_orm_plan(&paths);
        assert!(
            plan.get_block.contains(
                "included.deployment = Some(repo.fetch_deployment_for_worker(&tx, id).await"
            ),
            "{}",
            plan.get_block
        );
        assert!(
            plan.get_block
                .contains("                        .with_correlation_id(correlation_id))?);")
        );
        assert!(!plan.get_block.contains("batch_for_worker"));
    }

    // ── GET: dot-notation merge ─────────────────────────────────────────

    #[test]
    fn get_dot_path_merges_into_the_combined_struct() {
        let paths = vec![path(
            "deployment.position",
            vec![segment("deployment", false), segment("position", false)],
        )];
        let plan = sea_orm_plan(&paths);
        assert!(
            plan.get_block
                .contains("// Dot-notation path: merge into combined response")
        );
        assert!(
            plan.get_block.contains(
                "let leaf_resp = repo.fetch_deployment_position_for_worker(&tx, id).await"
            )
        );
        assert!(
            plan.get_block
                .contains("if let Some(ref mut combined) = included.deployment {")
        );
        assert!(plan.get_block.contains("combined.position = val.position;"));
        assert!(
            plan.get_block
                .contains("included.deployment = Some(DeploymentPositionCombinedResponse {")
        );
        assert!(plan.get_block.contains("position: val.position,"));
        assert!(plan.get_block.contains("..Default::default()"));
    }

    #[test]
    fn get_two_dot_paths_sharing_a_segment_accumulate_on_the_same_entry() {
        let paths = vec![
            path(
                "deployment.position",
                vec![segment("deployment", false), segment("position", false)],
            ),
            path(
                "deployment.assignment",
                vec![segment("deployment", false), segment("assignment", false)],
            ),
        ];
        let plan = sea_orm_plan(&paths);
        assert!(plan.get_block.contains("combined.position = val.position;"));
        assert!(
            plan.get_block
                .contains("combined.assignment = val.assignment;")
        );
        // Both arms bind `included.deployment` (accumulation, not clobbering).
        assert_eq!(
            plan.get_block
                .matches("if let Some(ref mut combined) = included.deployment {")
                .count(),
            2
        );
        assert_eq!(
            plan.get_block
                .matches("included.deployment = Some(")
                .count(),
            2
        );
    }

    // ── LIST ────────────────────────────────────────────────────────────

    #[test]
    fn list_single_path_uses_the_batch_fetch() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let plan = sea_orm_plan(&paths);
        assert!(
            plan.list_block
                .contains("let included = if !include_paths.is_empty() {")
        );
        assert!(
            plan.list_block
                .contains("let ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();")
        );
        assert!(
            plan.list_block
                .contains("if include_paths.contains(&\"person\".to_string()) {")
        );
        assert!(
            plan.list_block.contains(
                "serde_json::to_value(repo.fetch_person_batch_for_worker(&tx, &ids).await"
            )
        );
        assert!(plan.list_block.contains(".unwrap_or_default(),"));
        assert!(
            plan.list_block
                .contains("        std::collections::HashMap::new()")
        );
        // The template keeps the ALLOWED_INCLUDE_KEYS loop — never emitted here.
        assert!(!plan.list_block.contains("ALLOWED_INCLUDE_KEYS"));
        assert!(!plan.get_block.contains("ALLOWED_INCLUDE_KEYS"));
        assert!(!plan.list_block.ends_with('\n'));
    }

    #[test]
    fn list_single_path_block_is_exact() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let plan = sea_orm_plan(&paths);
        let expected = [
            "    let included = if !include_paths.is_empty() {",
            "        use sea_orm::TransactionTrait;",
            "        let tx = state.db.begin().await",
            "            .map_err(|e| AppError::internal(format!(\"Failed to begin transaction for includes: {e}\"))",
            "                .with_correlation_id(correlation_id))?;",
            "        let repo = crate::domain::common::worker::repository_impl::WorkerRepositoryImpl;",
            "        let ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();",
            "        let mut included_map = std::collections::HashMap::<String, serde_json::Value>::new();",
            "        if include_paths.contains(&\"person\".to_string()) {",
            "            included_map.insert(",
            "                \"person\".to_string(),",
            "                serde_json::to_value(repo.fetch_person_batch_for_worker(&tx, &ids).await",
            "                    .map_err(|e| AppError::internal(format!(\"Failed to include person: {e}\"))",
            "                        .with_correlation_id(correlation_id))?).unwrap_or_default(),",
            "            );",
            "        }",
            "        tx.commit().await",
            "            .map_err(|e| AppError::internal(format!(\"Failed to commit transaction: {e}\"))",
            "                .with_correlation_id(correlation_id))?;",
            "        included_map",
            "    } else {",
            "        std::collections::HashMap::new()",
            "    };",
        ]
        .join("\n");
        assert_eq!(plan.list_block, expected);
    }

    #[test]
    fn list_dot_path_accumulates_into_included_map() {
        let paths = vec![
            path(
                "deployment.position",
                vec![segment("deployment", false), segment("position", false)],
            ),
            path(
                "deployment.assignment",
                vec![segment("deployment", false), segment("assignment", false)],
            ),
        ];
        let plan = sea_orm_plan(&paths);
        // Both leaf inserts key the same first-segment entry.
        assert_eq!(
            plan.list_block
                .matches(".entry(\"deployment\".to_string())")
                .count(),
            2
        );
        assert!(
            plan.list_block
                .contains("let leaf = serde_json::json!(combined.position);")
        );
        assert!(
            plan.list_block
                .contains("let leaf = serde_json::json!(combined.assignment);")
        );
        assert!(
            plan.list_block
                .contains("                            \"position\".to_string(),")
        );
        assert!(
            plan.list_block
                .contains("                            \"assignment\".to_string(),")
        );
        assert!(plan.list_block.contains(
            "        // sibling dot paths sharing the first segment accumulate instead of"
        ));
    }

    // ── Empty / cornucopia / unsupported ────────────────────────────────

    #[test]
    fn empty_paths_produce_empty_blocks() {
        let plan = sea_orm_plan(&[]);
        assert!(plan.get_block.is_empty());
        assert!(plan.list_block.is_empty());
        assert!(plan.include_doc.is_empty());
        assert_eq!(plan.support, IncludeSupport::SeaOrm);
    }

    /// Issue #781 P1 landed: a default cornucopia project HYDRATES — the
    /// preview switch is gone, so this is what every cornucopia run emits.
    #[test]
    fn cornucopia_project_hydrates_by_default() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let plan = build_include_plan(&paths, &ctx(), &cornucopia_project());
        assert_eq!(plan.support, IncludeSupport::CornucopiaHydration);
        assert!(plan.support.is_supported());
        assert!(
            plan.get_block.contains(
                "included.person = state.common_worker_queries.fetch_person_for_worker(id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await"
            ),
            "{}",
            plan.get_block
        );
        assert!(
            plan.list_block.contains(
                "serde_json::to_value(state.common_worker_queries.fetch_person_batch_for_worker(&ids, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await"
            ),
            "{}",
            plan.list_block
        );
        // The scalar shape is implemented, so no per-path guard is emitted.
        assert!(!plan.get_block.contains("let unsupported = match"));
        assert!(!plan.list_block.contains("let unsupported = match"));
    }

    /// Unsupported SHAPES still fail closed per path (the retained 501 arm):
    /// a cornucopia project's depth-3 dot path 501s while the implemented
    /// scalar on the same handler hydrates.
    #[test]
    fn cornucopia_unsupported_shapes_still_fail_closed() {
        let paths = vec![
            path("person", vec![segment("person", false)]),
            path(
                "person.address.city",
                vec![
                    segment("person", false),
                    segment("address", false),
                    segment("city", false),
                ],
            ),
        ];
        let plan = build_include_plan(&paths, &ctx(), &cornucopia_project());
        let block = &plan.get_block;
        assert!(
            block.contains("        let unsupported = match path.as_str() {"),
            "{block}"
        );
        assert!(
            block.contains("            \"person.address.city\" => Some(\"dot-notation"),
            "{block}"
        );
        assert!(
            block.contains("status: axum::http::StatusCode::NOT_IMPLEMENTED,"),
            "{block}"
        );
        assert!(
            block.contains(
                "message: format!(\"?include={path} is not supported: {reason} (issue #781)\")"
            ),
            "{block}"
        );
        // The implemented shape on the same handler still hydrates.
        assert!(
            block.contains("included.person = state.common_worker_queries"),
            "{block}"
        );
    }

    #[test]
    fn unsupported_renders_a_guarded_fail_closed_501() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let support = IncludeSupport::Unsupported {
            reason: "persistence provider `diesel` has no include fetch plane".to_string(),
        };
        let plan = build_include_plan_with_support(support.clone(), &paths, &ctx());
        assert_eq!(plan.support, support);
        for block in [&plan.get_block, &plan.list_block] {
            assert!(block.contains("    if !include_paths.is_empty() {"));
            assert!(block.contains("status: axum::http::StatusCode::NOT_IMPLEMENTED,"));
            assert!(block.contains("code: \"NOT_IMPLEMENTED\","));
            // An injected reason that does not name the issue gets the
            // reference appended exactly once (the comment above is separate).
            assert!(block.contains("(issue #781)"));
            let message_line = block
                .lines()
                .find(|line| line.trim_start().starts_with("message:"))
                .expect("message line");
            assert_eq!(message_line.matches("#781").count(), 1, "{message_line}");
            assert!(block.contains("persistence provider `diesel` has no include fetch plane"));
            assert!(block.contains("correlation_id: Some(correlation_id),"));
        }
        assert!(plan.list_block.ends_with(
            "let included = std::collections::HashMap::<String, serde_json::Value>::new();"
        ));
    }

    // ── Doc line (issue #781) ───────────────────────────────────────────

    #[test]
    fn sea_orm_doc_line_is_the_historical_text() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let plan = sea_orm_plan(&paths);
        assert_eq!(
            plan.include_doc,
            "    /// Eager-load related entities, e.g. `?include=person,deployment.position`."
        );
    }

    #[test]
    fn unsupported_doc_line_is_honest_about_the_501() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let support = IncludeSupport::Unsupported {
            reason: "persistence provider `diesel` has no include fetch plane".to_string(),
        };
        let plan = build_include_plan_with_support(support, &paths, &ctx());
        assert!(
            plan.include_doc
                .starts_with("    /// Includes are modelled for this entity")
        );
        assert!(
            plan.include_doc
                .contains("not supported by this deployment's persistence provider")
        );
        assert!(plan.include_doc.contains("answers 501 (issue #781)"));
        // The historical "eager-load" claim is gone.
        assert!(!plan.include_doc.contains("Eager-load related entities"));
    }

    // ── Cornucopia hydration (issue #781 P1, default ON) ───────────────

    /// A default cornucopia project is the hydrating verdict, and the per-path
    /// classifier decides support.
    #[test]
    fn cornucopia_reports_per_path_support() {
        let paths = vec![
            path("person", vec![segment("person", false)]),
            path("deployment", vec![segment("deployment", true)]),
            path(
                "deployment.position",
                vec![segment("deployment", false), segment("position", false)],
            ),
            path(
                "deployment.position.building",
                vec![
                    segment("deployment", false),
                    segment("position", false),
                    segment("building", false),
                ],
            ),
        ];
        let plan = build_include_plan(&paths, &ctx(), &cornucopia_project());
        assert_eq!(plan.support, IncludeSupport::CornucopiaHydration);
        assert!(plan.support.is_supported());
        let supported = plan.support.supported_paths(&paths);
        assert_eq!(
            supported
                .iter()
                .map(|p| p.alias.as_str())
                .collect::<Vec<_>>(),
            vec!["person", "deployment", "deployment.position"]
        );
        let unsupported = plan.support.unsupported_paths(&paths);
        assert_eq!(unsupported.len(), 1);
        assert_eq!(unsupported[0].alias, "deployment.position.building");
        assert!(
            unsupported[0].reason.contains("dot-notation"),
            "{}",
            unsupported[0].reason
        );
    }

    /// The classifier names the construct for every unimplemented shape, and
    /// accepts exactly the shapes the SQL/adapter/query planes implement —
    /// including one hop of dot notation.
    #[test]
    fn cornucopia_shape_classifier_names_the_construct() {
        let two_hop = path(
            "deployment.position.building",
            vec![
                segment("deployment", false),
                segment("position", false),
                segment("building", false),
            ],
        );
        let reason = CornucopiaIncludeShape::classify(&two_hop).unwrap_err();
        assert!(reason.contains("dot-notation"), "{reason}");

        let dot = path(
            "deployment.position",
            vec![segment("deployment", false), segment("position", false)],
        );
        assert_eq!(
            CornucopiaIncludeShape::classify(&dot),
            Ok(CornucopiaIncludeShape::Dot)
        );

        // A dot whose first hop is a collection has no single combined leaf
        // slot to merge into.
        let array_first = path(
            "deployment.position",
            vec![segment("deployment", true), segment("position", false)],
        );
        let reason = CornucopiaIncludeShape::classify(&array_first).unwrap_err();
        assert!(reason.contains("first hop is a collection"), "{reason}");

        // A dot whose leaf is a collection needs a Vec slot the combined
        // response does not model.
        let array_leaf = path(
            "deployment.position",
            vec![segment("deployment", false), segment("position", true)],
        );
        let reason = CornucopiaIncludeShape::classify(&array_leaf).unwrap_err();
        assert!(reason.contains("leaf is a collection"), "{reason}");

        // A dot leaf with no FK on the intermediate cannot be addressed.
        let mut no_leaf_fk = segment("position", false);
        no_leaf_fk.fk_column = String::new();
        let reason = CornucopiaIncludeShape::classify(&path(
            "deployment.position",
            vec![segment("deployment", false), no_leaf_fk],
        ))
        .unwrap_err();
        assert!(reason.contains("no FK column"), "{reason}");

        // An array whose path carries no back-FK column cannot be scanned.
        let mut no_back_fk = segment("deployment", true);
        no_back_fk.reverse_fk_column = String::new();
        let reason =
            CornucopiaIncludeShape::classify(&path("deployment", vec![no_back_fk])).unwrap_err();
        assert!(reason.contains("no back-FK column"), "{reason}");

        let mut vo = segment("person", false);
        vo.child_table_override = Some(crate::api::include_path::ChildTableOverride {
            vo_title: "PersonLegalType".to_string(),
            child_table_name: "worker_person".to_string(),
            child_module: "common_worker_person".to_string(),
            child_schema: "common".to_string(),
            parent_fk_column: "worker_id".to_string(),
            response_type: "WorkerPersonLegalResponse".to_string(),
        });
        let reason = CornucopiaIncludeShape::classify(&path("person", vec![vo])).unwrap_err();
        assert!(reason.contains("VO→entity child-table"), "{reason}");

        let scalar = path("person", vec![segment("person", false)]);
        assert_eq!(
            CornucopiaIncludeShape::classify(&scalar),
            Ok(CornucopiaIncludeShape::SingleScalar)
        );

        let array = path("deployment", vec![segment("deployment", true)]);
        assert_eq!(
            CornucopiaIncludeShape::classify(&array),
            Ok(CornucopiaIncludeShape::SingleArray)
        );
    }

    /// GET: the supported arms call the entity's cornucopia QUERY handler
    /// (which owns the client/transaction/RLS session vars) — never the
    /// adapter directly — the array shape wraps the collection in `Some(…)`,
    /// and the unsupported arm 501s naming the alias.
    #[test]
    fn cornucopia_hydration_get_block_calls_the_query_handler() {
        let paths = vec![
            path("person", vec![segment("person", false)]),
            path("deployment", vec![segment("deployment", true)]),
            path(
                "person.address.city",
                vec![
                    segment("person", false),
                    segment("address", false),
                    segment("city", false),
                ],
            ),
        ];
        let plan = build_include_plan(&paths, &ctx(), &cornucopia_project());
        let block = &plan.get_block;

        assert!(
            block.starts_with("    for path in &include_paths {\n"),
            "{block}"
        );
        // Per-path fail-closed guard names the requested alias.
        assert!(
            block.contains("        let unsupported = match path.as_str() {"),
            "{block}"
        );
        assert!(
            block.contains("            \"person.address.city\" => Some(\"dot-notation"),
            "{block}"
        );
        assert!(
            block.contains(
                "message: format!(\"?include={path} is not supported: {reason} (issue #781)\")"
            ),
            "{block}"
        );
        assert!(
            block.contains("status: axum::http::StatusCode::NOT_IMPLEMENTED,"),
            "{block}"
        );
        // The implemented scalar arm hydrates through the query handler with
        // the session args in the provider's positional order.
        assert!(
            block.contains(
                "included.person = state.common_worker_queries.fetch_person_for_worker(id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await"
            ),
            "{block}"
        );
        // The array arm assigns `Some(collection)` — the DTO field is
        // `Option<Vec<…>>`, and an empty vec is still a present value.
        assert!(
            block.contains(
                "included.deployment = Some(state.common_worker_queries.fetch_deployment_for_worker(id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await"
            ),
            "{block}"
        );
        assert!(block.contains("                    })?);"), "{block}");
        assert!(block.contains(".map_err(|e: CommonError| {"), "{block}");
        assert!(
            block.contains(
                "AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to include person\")"
            ),
            "{block}"
        );
        assert!(
            block.contains(
                "AppError::from_domain_error(e.http_status(), e.to_string(), \"Failed to include deployment\")"
            ),
            "{block}"
        );
        assert!(block.contains("            _ => {}"), "{block}");
        // No SeaORM transaction opened in the handler.
        assert!(!block.contains("state.db.begin()"), "{block}");
        assert!(
            !block.ends_with('\n'),
            "the template supplies the line break"
        );
    }

    /// LIST: one batch call per path, keyed by source id in the same
    /// `HashMap<Uuid, …>` JSON shape the SeaORM arm emits. The array shape
    /// serialises the SAME map (its values are `Vec<{Target}Response>` — the
    /// batch contract the query wrapper declares).
    #[test]
    fn cornucopia_hydration_list_block_batches_by_source_id() {
        let paths = vec![
            path("person", vec![segment("person", false)]),
            path("deployment", vec![segment("deployment", true)]),
        ];
        let plan = build_include_plan(&paths, &ctx(), &cornucopia_project());
        let block = &plan.list_block;

        assert!(
            block.contains("    let included = if !include_paths.is_empty() {"),
            "{block}"
        );
        assert!(
            block.contains("let ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();"),
            "{block}"
        );
        assert!(
            block.contains("if include_paths.contains(&\"person\".to_string()) {"),
            "{block}"
        );
        assert!(
            block.contains(
                "serde_json::to_value(state.common_worker_queries.fetch_person_batch_for_worker(&ids, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await"
            ),
            "{block}"
        );
        assert!(
            block.contains("if include_paths.contains(&\"deployment\".to_string()) {"),
            "{block}"
        );
        assert!(
            block.contains(
                "serde_json::to_value(state.common_worker_queries.fetch_deployment_batch_for_worker(&ids, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await"
            ),
            "the array batch rides the same envelope:\n{block}"
        );
        assert!(block.contains(".unwrap_or_default(),"), "{block}");
        assert!(
            block.contains("        std::collections::HashMap::new()"),
            "{block}"
        );
        // No per-path guard when every modelled path is implemented.
        assert!(!block.contains("let unsupported = match"), "{block}");
        assert!(!block.contains("state.db.begin()"), "{block}");
    }

    /// A dict with ONLY unsupported shapes still 501s — the block is the
    /// guard plus the empty map the Ok envelope reads.
    #[test]
    fn cornucopia_hydration_all_unsupported_still_fails_closed() {
        let paths = vec![path(
            "deployment.position.building",
            vec![
                segment("deployment", false),
                segment("position", false),
                segment("building", false),
            ],
        )];
        let plan = build_include_plan(&paths, &ctx(), &cornucopia_project());
        assert!(
            plan.get_block
                .contains("\"deployment.position.building\" => Some(\"dot-notation"),
            "{}",
            plan.get_block
        );
        assert!(
            !plan.get_block.contains("state.common_worker_queries"),
            "{}",
            plan.get_block
        );
        assert!(
            plan.list_block.contains(
                "let included = std::collections::HashMap::<String, serde_json::Value>::new();"
            ) || plan
                .list_block
                .contains("let included = if !include_paths.is_empty() {"),
            "{}",
            plan.list_block
        );
    }

    /// GET: the dot arm merges the leaf into the combined response — the
    /// `included.<first>` entry is created when absent and updated in place
    /// otherwise, exactly like the SeaORM dot arm.
    #[test]
    fn cornucopia_hydration_get_block_merges_the_dot_leaf() {
        let paths = vec![path(
            "customer.org",
            vec![segment("customer", false), segment("organization", false)],
        )];
        let plan = build_include_plan(&paths, &ctx(), &cornucopia_project());
        let block = &plan.get_block;

        assert!(
            block.contains("            \"customer.org\" => {"),
            "{block}"
        );
        assert!(
            block.contains("                // Dot-notation path: merge into combined response"),
            "{block}"
        );
        assert!(
            block.contains(
                "let leaf_resp = state.common_worker_queries.fetch_customer_org_for_worker(id, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await"
            ),
            "{block}"
        );
        assert!(
            block.contains("if let Some(ref mut combined) = included.customer {"),
            "{block}"
        );
        assert!(
            block.contains("combined.organization = val.organization;"),
            "{block}"
        );
        assert!(
            block.contains("included.customer = Some(CustomerOrgCombinedResponse {"),
            "{block}"
        );
        assert!(block.contains("organization: val.organization,"), "{block}");
        assert!(block.contains("..Default::default()"), "{block}");
        // No per-path guard when the only modelled path is implemented.
        assert!(!block.contains("let unsupported = match"), "{block}");
    }

    /// LIST: the dot arm batches the leaf and accumulates it into
    /// `included.<first>[<source_id>].<leaf>` — sibling dot paths sharing the
    /// first hop merge, exactly like the SeaORM dot arm.
    #[test]
    fn cornucopia_hydration_list_block_accumulates_the_dot_leaf() {
        let paths = vec![
            path(
                "customer.org",
                vec![segment("customer", false), segment("organization", false)],
            ),
            path(
                "customer.billing",
                vec![segment("customer", false), segment("billing", false)],
            ),
        ];
        let plan = build_include_plan(&paths, &ctx(), &cornucopia_project());
        let block = &plan.list_block;

        assert!(
            block.contains("if include_paths.contains(&\"customer.org\".to_string()) {"),
            "{block}"
        );
        assert!(
            block.contains(
                "let batch = state.common_worker_queries.fetch_customer_org_batch_for_worker(&ids, api_key_info.api_key_id, api_key_info.organization_id, api_key_info.user_id, api_key_info.role.clone()).await"
            ),
            "{block}"
        );
        assert!(
            block.contains("let leaf = serde_json::json!(combined.organization);"),
            "{block}"
        );
        assert!(
            block.contains("let leaf = serde_json::json!(combined.billing);"),
            "{block}"
        );
        assert_eq!(
            block.matches(".entry(\"customer\".to_string())").count(),
            2,
            "sibling dot paths accumulate on the same first-hop entry:\n{block}"
        );
        assert!(
            block.contains("                            \"organization\".to_string(),"),
            "{block}"
        );
        assert!(
            block.contains("                            \"billing\".to_string(),"),
            "{block}"
        );
    }

    /// The hydrated arm's doc line is honest about the residual limits.
    #[test]
    fn cornucopia_hydration_doc_line_names_the_residual_limit() {
        let paths = vec![path("person", vec![segment("person", false)])];
        let plan = build_include_plan(&paths, &ctx(), &cornucopia_project());
        assert!(
            plan.include_doc.contains("Eager-load related entities"),
            "{}",
            plan.include_doc
        );
        assert!(
            plan.include_doc.contains("scalar `refers`"),
            "{}",
            plan.include_doc
        );
        assert!(plan.include_doc.contains("501"), "{}", plan.include_doc);
    }
}
