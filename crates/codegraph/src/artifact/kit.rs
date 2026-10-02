//! L3 conformance kit (issue #275), Morphir MCK-style.
//!
//! A case file is markdown: one `## <name>` heading per case, with fenced
//! blocks carrying the expectation — `json canonical` (the canonical
//! serialization, must be a parse → normalize → re-serialize fixed
//! point), `json accepted` (an equivalent document that must normalize
//! onto the canonical bytes), and `json rejected diagnostic=<name>` (must
//! fail with that named diagnostic). The committed case file pinning the
//! graph document format lives at
//! `tests/fixtures/artifact_kit/graph_document_v1.md`.

use codegraph_grafeo::artifact::{ArtifactError, canonical_bytes, normalize, parse_document};

/// One parsed conformance case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitCase {
    pub name: String,
    pub canonical: Option<Vec<u8>>,
    pub accepted: Vec<Vec<u8>>,
    pub rejected: Vec<RejectedCase>,
}

/// A `rejected` fence: body plus the diagnostic name it must produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedCase {
    pub diagnostic: String,
    pub body: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum KitError {
    #[error("kit case '{0}' has no fences")]
    CaseWithoutFences(String),
    #[error("kit case '{name}' has accepted variants but no canonical document")]
    AcceptedWithoutCanonical { name: String },
    #[error(
        "kit fence {0:?} is not 'json canonical', 'json accepted', or 'json rejected diagnostic=<name>'"
    )]
    InvalidFence(String),
    #[error("kit case '{case}': invalid canonical document: {source}")]
    InvalidCanonical {
        case: String,
        #[source]
        source: ArtifactError,
    },
    #[error("kit case '{case}': canonical document is not a serialization fixed point")]
    CanonicalNotFixedPoint { case: String },
    #[error("kit case '{case}': invalid accepted variant {index}: {source}")]
    InvalidAccepted {
        case: String,
        index: usize,
        #[source]
        source: ArtifactError,
    },
    #[error(
        "kit case '{case}': accepted variant {index} does not normalize onto the canonical bytes"
    )]
    AcceptedMismatch { case: String, index: usize },
    #[error("kit case '{case}': rejected input was accepted")]
    RejectedAccepted { case: String },
    #[error("kit case '{case}': expected diagnostic {expected:?}, got {actual:?}")]
    DiagnosticMismatch {
        case: String,
        expected: String,
        actual: String,
    },
}

struct OpenFence {
    qualifier: String,
    body: Vec<u8>,
}

/// Parse a markdown case file into cases.
pub fn parse_cases(markdown: &str) -> Result<Vec<KitCase>, KitError> {
    let mut cases: Vec<KitCase> = Vec::new();
    let mut open: Option<OpenFence> = None;

    for line in markdown.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            close_fence(&mut cases, &mut open)?;
            cases.push(KitCase {
                name: rest.trim().to_string(),
                canonical: None,
                accepted: Vec::new(),
                rejected: Vec::new(),
            });
        } else if let Some(qualifier) = line.trim_start().strip_prefix("```") {
            let qualifier = qualifier.trim();
            // A bare ``` closes the open fence; an info string opens one
            // (and implicitly closes an unterminated one first).
            let opening = !qualifier.is_empty();
            close_fence(&mut cases, &mut open)?;
            if opening {
                current_case_name(&cases, line)?;
                open = Some(OpenFence {
                    qualifier: qualifier.to_string(),
                    body: Vec::new(),
                });
            }
        } else if let Some(fence) = open.as_mut() {
            fence.body.extend_from_slice(line.as_bytes());
            fence.body.push(b'\n');
        }
    }
    close_fence(&mut cases, &mut open)?;
    Ok(cases)
}

fn current_case_name(cases: &[KitCase], line: &str) -> Result<String, KitError> {
    cases
        .last()
        .map(|c| c.name.clone())
        .ok_or_else(|| KitError::InvalidFence(line.to_string()))
}

fn close_fence(cases: &mut [KitCase], open: &mut Option<OpenFence>) -> Result<(), KitError> {
    let fence = match open.take() {
        Some(fence) => fence,
        None => return Ok(()),
    };
    let case = cases
        .last_mut()
        .ok_or_else(|| KitError::InvalidFence(fence.qualifier.clone()))?;
    store_fence(case, &fence.qualifier, &fence.body)
}

fn store_fence(case: &mut KitCase, qualifier: &str, body: &[u8]) -> Result<(), KitError> {
    let content = trim_ascii(body);
    let mut parts = qualifier.split_whitespace();
    match (parts.next(), parts.next(), parts.next()) {
        (Some("json"), Some("canonical"), None) => {
            case.canonical = Some(content.to_vec());
            Ok(())
        }
        (Some("json"), Some("accepted"), None) => {
            case.accepted.push(content.to_vec());
            Ok(())
        }
        (Some("json"), Some("rejected"), _) => {
            let diagnostic = qualifier
                .split_whitespace()
                .find_map(|p| p.strip_prefix("diagnostic="))
                .ok_or_else(|| KitError::InvalidFence(qualifier.to_string()))?;
            case.rejected.push(RejectedCase {
                diagnostic: diagnostic.to_string(),
                body: content.to_vec(),
            });
            Ok(())
        }
        _ => Err(KitError::InvalidFence(qualifier.to_string())),
    }
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes.len()
        - bytes
            .iter()
            .rev()
            .position(|b| !b.is_ascii_whitespace())
            .unwrap_or(bytes.len());
    &bytes[start..end]
}

/// Run every case in the file; a failing case is a named `KitError`.
pub fn run_cases(markdown: &str) -> Result<(), KitError> {
    for case in parse_cases(markdown)? {
        run_case(&case)?;
    }
    Ok(())
}

fn run_case(case: &KitCase) -> Result<(), KitError> {
    let canonical = match &case.canonical {
        Some(bytes) => {
            let mut doc = parse_document(bytes).map_err(|source| KitError::InvalidCanonical {
                case: case.name.clone(),
                source,
            })?;
            normalize(&mut doc);
            let serialized =
                canonical_bytes(&doc).map_err(|source| KitError::InvalidCanonical {
                    case: case.name.clone(),
                    source,
                })?;
            if serialized != *bytes {
                return Err(KitError::CanonicalNotFixedPoint {
                    case: case.name.clone(),
                });
            }
            serialized
        }
        None => {
            if !case.accepted.is_empty() {
                return Err(KitError::AcceptedWithoutCanonical {
                    name: case.name.clone(),
                });
            }
            Vec::new()
        }
    };

    for (index, bytes) in case.accepted.iter().enumerate() {
        let mut doc = parse_document(bytes).map_err(|source| KitError::InvalidAccepted {
            case: case.name.clone(),
            index,
            source,
        })?;
        normalize(&mut doc);
        let serialized = canonical_bytes(&doc).map_err(|source| KitError::InvalidAccepted {
            case: case.name.clone(),
            index,
            source,
        })?;
        if serialized != canonical {
            return Err(KitError::AcceptedMismatch {
                case: case.name.clone(),
                index,
            });
        }
    }

    for rejected in &case.rejected {
        match parse_document(&rejected.body) {
            Ok(_) => {
                return Err(KitError::RejectedAccepted {
                    case: case.name.clone(),
                });
            }
            Err(err) if err.diagnostic() != rejected.diagnostic => {
                return Err(KitError::DiagnosticMismatch {
                    case: case.name.clone(),
                    expected: rejected.diagnostic.clone(),
                    actual: err.diagnostic().to_string(),
                });
            }
            Err(_) => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CASES: &str = include_str!("../../tests/fixtures/artifact_kit/graph_document_v1.md");

    #[test]
    fn kit_case_canonical_accepted_rejected() {
        let cases = parse_cases(CASES).unwrap();
        assert_eq!(cases.len(), 3);
        assert!(cases[0].canonical.is_some());
        assert!(cases[0].accepted.is_empty());
        assert!(cases[1].canonical.is_some());
        assert_eq!(cases[1].accepted.len(), 1);
        assert!(cases[2].canonical.is_none());
        assert_eq!(cases[2].rejected.len(), 1);
        run_cases(CASES).unwrap();
    }

    #[test]
    fn kit_case_rejects_wrong_diagnostic_name() {
        let md = "## broken\n\n```json rejected diagnostic=not_the_actual_error\n{\"formatVersion\":99,\"nodes\":[],\"edges\":[]}\n```\n";
        let err = run_cases(md).expect_err("diagnostic mismatch must fail the case");
        assert!(matches!(err, KitError::DiagnosticMismatch { .. }), "{err}");
    }

    #[test]
    fn kit_case_rejects_unknown_fence_qualifier() {
        let md = "## odd\n\n```yaml canonical\nformatVersion: 1\n```\n";
        let err = run_cases(md).expect_err("non-json fences are not supported yet");
        assert!(matches!(err, KitError::InvalidFence(_)), "{err}");
    }

    #[test]
    fn kit_case_rejects_canonical_drift() {
        let md = concat!(
            "## drift\n\n```json canonical\n",
            "{\"edges\":[],\"formatVersion\":1,\"nodes\":[]}\n",
            "```\n"
        );
        let err = run_cases(md).expect_err("alphabetical field order must not be canonical");
        assert!(
            matches!(err, KitError::CanonicalNotFixedPoint { .. }),
            "{err}"
        );
    }
}
