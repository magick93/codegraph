//! The IFML e2e generator (#317 module split): every submodule re-exported
//! through here, so the external `ifml::e2e_test::IfmlE2eTestGenerator`
//! path is unchanged.

mod assembly;
mod auth;
mod fixtures;
mod generator;
mod kernel;
mod pom;
mod pom_render;
mod render;
mod spec_payload;
mod ux_plans;

pub use fixtures::Fixture;
pub use generator::IfmlE2eTestGenerator;
pub use spec_payload::{
    ClickThroughTest, ModalCloseAssertions, PersonaTest, RenderTest, RoundTripTest, TransitionStep,
    UxCellFormat, UxChipCheck, UxColumnCheck, UxCopyCheck, UxMenuCheck, UxTimelineCheck,
    UxViewTest, ValidationTest, ViewTestSpec, WorkflowTest,
};

#[cfg(test)]
mod tests;
