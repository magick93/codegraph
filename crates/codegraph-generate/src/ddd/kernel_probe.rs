//! The rendered kernel-probe fragments (issue #818, Option A) shared by
//! every machine-transition lowering — the `save`/`update` arm and the
//! statement-body arm — so the two planes cannot drift.
//!
//! These fragments serve ops that NAME their edge (the op's declared
//! targets). A body-driven op — no binding, no trigger match — resolves the
//! REQUESTED target against the current state's out-edges instead
//! (`MachineRuntime::resolve_target`, issue #831), so it never bakes a
//! candidate list.
//!
//! One execution path for machine transitions in a generated app: the
//! handler loads the entity's registry machine
//! (`{domain_types}::machines::machine_json`), builds the fail-closed
//! [`codegraph_workflow::MachineRuntime`], and PROBES its declared
//! candidate targets from the record's current state
//! ([`codegraph_workflow::MachineRuntime::transition_for`]). The
//! hand-rendered `match current_state` arms are retired; the kernel
//! decides.

use crate::ddd::service_ops::rust_string_literal;

/// The runtime construction both machine arms share: load the registry
/// embed, parse it, and build the fail-closed
/// [`codegraph_workflow::MachineRuntime`]. Every error arm is a defect
/// seam (generation validated the embed) — an internal 500, never a
/// silently skipped transition. `indent` is the leading whitespace of the
/// first line (`    ` in the update arm; deeper inside a body block).
#[must_use]
pub fn runtime_construction(domain_types_crate: &str, machine_id: &str, indent: &str) -> String {
    let id_literal = rust_string_literal(machine_id);
    let missing = rust_string_literal(&format!(
        "machine `{machine_id}` is not in the machines registry"
    ));
    let unparseable = rust_string_literal(&format!("machine `{machine_id}` does not parse: {{e}}"));
    let invalid = rust_string_literal(&format!("machine `{machine_id}` is invalid: {{e}}"));
    format!(
        concat!(
            "{i}// Kernel transition (issue #818, Option A): the machine decides —\n",
            "{i}// the hand-rendered match arms are retired. The candidate targets\n",
            "{i}// are probed through the runtime; exactly one legal edge from the\n",
            "{i}// current state fires; none or ambiguous is the 409.\n",
            "{i}let machine_embedded = {dt}::machines::machine_json({id})\n",
            "{i}    .ok_or_else(|| {{\n",
            "{i}        AppError::internal({missing}).with_correlation_id(correlation_id)\n",
            "{i}    }})?;\n",
            "{i}let machine_ir: serde_json::Value = serde_json::from_str(machine_embedded)\n",
            "{i}    .map_err(|e| {{\n",
            "{i}        AppError::internal(format!({unparseable}))\n",
            "{i}            .with_correlation_id(correlation_id)\n",
            "{i}    }})?;\n",
            "{i}let runtime = codegraph_workflow::MachineRuntime::from_json(&machine_ir)\n",
            "{i}    .map_err(|e| {{\n",
            "{i}        AppError::internal(format!({invalid}))\n",
            "{i}            .with_correlation_id(correlation_id)\n",
            "{i}    }})?;\n",
        ),
        i = indent,
        dt = domain_types_crate,
        id = id_literal,
        missing = missing,
        unparseable = unparseable,
        invalid = invalid,
    )
}

/// The `let legal: Vec<&str> = …collect();` probe for a NAMED-transition op:
/// every candidate target (baked from the op's declared pair list — deduped,
/// declaration order) is probed from the record's current state through the
/// kernel's target-addressed lookup. A body-driven op uses
/// `MachineRuntime::resolve_target` instead (issue #831) — the candidate
/// universe there is the current state's own edges, never the machine's. `indent` is the first line's leading
/// whitespace; continuation lines get the same indent (Rust does not
/// care, the contract pins the shape).
#[must_use]
pub fn probe_legal_block(candidates: &[String], indent: &str) -> String {
    let literals: Vec<String> = candidates.iter().map(|c| rust_string_literal(c)).collect();
    format!(
        concat!(
            "{i}let legal: Vec<&str> = [{cands}]\n",
            "{i}    .iter()\n",
            "{i}    .copied()\n",
            "{i}    .filter(|candidate| runtime.transition_for(&current_state, candidate).is_some())\n",
            "{i}    .collect();\n",
        ),
        i = indent,
        cands = literals.join(", "),
    )
}

/// The named-transition verdict: exactly one legal target fires (its
/// value is bound to `target_state`), none is the contract 409, and an
/// ambiguous fork names its targets (the shape the old first-match
/// `match` arm silently collapsed).
#[must_use]
pub fn named_verdict_block(op_name: &str, indent: &str) -> String {
    format!(
        concat!(
            "{i}let target_state: &str = match legal.as_slice() {{\n",
            "{i}    [only] => *only,\n",
            "{i}    [] => {{\n",
            "{i}        return Err(AppError::conflict(format!(\n",
            "{i}            \"operation `{op}` has no transition from the current state `{{}}`\",\n",
            "{i}            current_state\n",
            "{i}        )).with_correlation_id(correlation_id));\n",
            "{i}    }}\n",
            "{i}    many => {{\n",
            "{i}        return Err(AppError::conflict(format!(\n",
            "{i}            \"operation `{op}` is ambiguous from the current state `{{}}`: the machine declares transitions to {{}}\",\n",
            "{i}            current_state,\n",
            "{i}            many.join(\", \")\n",
            "{i}        )).with_correlation_id(correlation_id));\n",
            "{i}    }}\n",
            "{i}}};\n",
        ),
        i = indent,
        op = op_name,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The construction KAT: the registry load, the parse, and the
    /// fail-closed runtime — one execution path, no match arms.
    #[test]
    fn runtime_construction_renders_the_kernel_load() {
        let block = runtime_construction("domain_types", "payroll_pay_run_status", "    ");
        assert!(
            block.contains(r#"domain_types::machines::machine_json("payroll_pay_run_status")"#),
            "{block}"
        );
        assert!(block.contains("codegraph_workflow::MachineRuntime::from_json(&machine_ir)"));
        assert!(!block.contains("match current_state"));
    }

    /// The probe KAT: candidates bake as string literals, the kernel's
    /// target-addressed lookup decides.
    #[test]
    fn probe_legal_block_bakes_the_candidates() {
        let block = probe_legal_block(&["Submitted".to_string(), "Approved".to_string()], "    ");
        assert!(block.contains(r#"["Submitted", "Approved"]"#), "{block}");
        assert!(
            block.contains("runtime.transition_for(&current_state, candidate).is_some()"),
            "{block}"
        );
    }

    /// The verdict KAT: the none-arm keeps the wire-contract message
    /// shape ("no transition from the current state"), the ambiguous arm
    /// names its targets.
    #[test]
    fn named_verdict_keeps_the_wire_message_shape() {
        let block = named_verdict_block("submit", "    ");
        assert!(
            block.contains("has no transition from the current state"),
            "{block}"
        );
        assert!(block.contains("is ambiguous from the current state"));
    }
}
