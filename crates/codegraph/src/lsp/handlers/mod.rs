//! LSP request handlers, split by feature. Public handler paths are
//! preserved through re-exports (`handlers::handle_*` etc.), and the
//! `lsp::mox` module keeps reaching `collect_errors` / `has_any_error` /
//! `get_word_at_position` through this module.

mod code_action;
mod completion;
mod diagnostics;
mod hover;
mod positions;
mod semantic_tokens;
mod tree_utils;

pub use code_action::handle_code_action;
pub use completion::handle_completion;
pub use diagnostics::{compute_diagnostics, handle_document_diagnostic};
pub use hover::{handle_goto_definition, handle_hover};
pub use positions::{UpdatePositionsRequest, handle_update_positions};
pub use semantic_tokens::{TOKEN_MODIFIERS, TOKEN_TYPES, handle_semantic_tokens_full};

pub(crate) use hover::get_word_at_position;
pub(crate) use tree_utils::{collect_errors, has_any_error};
