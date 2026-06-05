use codegraph_core::traits::GraphIngestor;
use codegraph_core::types::{
    ActionNode, EdgeProperties, EdgeType, EventNode, ParameterDefinitionNode,
    ViewComponentNode, ViewContainerNode,
};
use codegraph_ifml_dsl::*;

use crate::error::{Error, Result};

/// Ingest a parsed IFML model into the graph database.
pub async fn ingest_ifml_model(
    db: &dyn GraphIngestor,
    model: &IfmlModel,
) -> Result<IfmlIngestStats> {
    let mut stats = IfmlIngestStats::default();

    // Ingest all view containers
    for view in &model.views {
        let vc_id = ingest_view_container(db, view).await?;
        stats.view_containers += 1;

        // Ingest container params
        for param in &view.params {
            let param_id = db
                .ingest_parameter_definition(&ParameterDefinitionNode {
                    name: param.name.clone(),
                    direction: "in".to_string(),
                    type_ref: param.type_ref.clone(),
                    domain: None,
                })
                .await
                .map_err(|e| Error::Graph(e))?;
            stats.parameters += 1;

            db.ingest_edge(
                &vc_id,
                &param_id,
                EdgeType::HasParameter,
                Some(&EdgeProperties {
                    direction: Some("in".to_string()),
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| Error::Graph(e))?;
        }

        // Ingest nested containers
        for container in &view.containers {
            let _container_id = ingest_container(db, container, &vc_id).await?;
            stats.containers += 1;
        }

        // Ingest view components
        for comp in &view.components {
            let _comp_id = ingest_view_component(db, comp, &vc_id).await?;
            stats.components += 1;
        }

        // Ingest view-level events
        for event in &view.events {
            handle_event(db, event, &vc_id).await?;
            stats.events += 1;
        }
    }

    // Ingest standalone actions
    for action in &model.actions {
        let action_id = db
            .ingest_action_node(&ActionNode {
                name: action.name.clone(),
                domain: None,
            })
            .await
            .map_err(|e| Error::Graph(e))?;
        stats.actions += 1;

        // Ingest action body events
        for event in &action.events {
            handle_event(db, event, &action_id).await?;
            stats.events += 1;
        }
    }

    Ok(stats)
}

async fn ingest_view_container(
    db: &dyn GraphIngestor,
    view: &ViewDeclaration,
) -> Result<String> {
    let node = ViewContainerNode {
        name: view.name.clone(),
        label: view.label.clone(),
        is_xor: view.is_xor,
        is_default: false,
        is_landmark: view.is_landmark,
        is_modal: view.is_modal,
        domain: None,
    };
    let id = db
        .ingest_view_container(&node)
        .await
        .map_err(|e| Error::Graph(e))?;
    Ok(id)
}

async fn ingest_container(
    db: &dyn GraphIngestor,
    container: &ContainerDeclaration,
    parent_id: &str,
) -> Result<String> {
    let node = ViewContainerNode {
        name: container.name.clone(),
        label: None,
        is_xor: false,
        is_default: container.is_default,
        is_landmark: false,
        is_modal: false,
        domain: None,
    };
    let id = db
        .ingest_view_container(&node)
        .await
        .map_err(|e| Error::Graph(e))?;

    // Link to parent
    db.ingest_edge(parent_id, &id, EdgeType::ContainsViewContainer, None)
        .await
        .map_err(|e| Error::Graph(e))?;

    // Ingest container's components
    for comp in &container.components {
        let comp_id = ingest_view_component(db, comp, &id).await?;
        let _ = comp_id;
    }

    // Ingest container's events
    for event in &container.events {
        handle_event(db, event, &id).await?;
    }

    Ok(id)
}

async fn ingest_view_component(
    db: &dyn GraphIngestor,
    comp: &ComponentDeclaration,
    parent_id: &str,
) -> Result<String> {
    let component_type = comp
        .properties
        .iter()
        .find(|p| p.key == "type")
        .map(|p| match &p.value {
            ValueExpression::Identifier(s) => s.clone(),
            _ => "unknown".to_string(),
        })
        .unwrap_or_else(|| "unknown".to_string());

    let entity = comp
        .properties
        .iter()
        .find(|p| p.key == "data")
        .and_then(|p| match &p.value {
            ValueExpression::Identifier(s) => Some(s.clone()),
            _ => None,
        });

    let fields = comp
        .properties
        .iter()
        .find(|p| p.key == "fields")
        .and_then(|p| match &p.value {
            ValueExpression::Array(items) => Some(
                items
                    .iter()
                    .filter_map(|v| match v {
                        ValueExpression::Identifier(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        });

    let filter = comp
        .properties
        .iter()
        .find(|p| p.key == "filter")
        .and_then(|p| match &p.value {
            ValueExpression::Identifier(s) => Some(s.clone()),
            _ => None,
        });

    let mode = comp
        .properties
        .iter()
        .find(|p| p.key == "mode")
        .and_then(|p| match &p.value {
            ValueExpression::Identifier(s) => Some(s.clone()),
            _ => None,
        });

    let node = ViewComponentNode {
        name: comp.name.clone(),
        component_type,
        mode,
        entity,
        fields,
        filter,
        domain: None,
    };

    let id = db
        .ingest_view_component(&node)
        .await
        .map_err(|e| Error::Graph(e))?;

    // Edge: parent contains component
    db.ingest_edge(parent_id, &id, EdgeType::ContainsViewComponent, None)
        .await
        .map_err(|e| Error::Graph(e))?;

    // Ingest events
    for event in &comp.events {
        handle_event(db, event, &id).await?;
    }

    Ok(id)
}

async fn handle_event(
    db: &dyn GraphIngestor,
    event: &EventHandler,
    parent_id: &str,
) -> Result<()> {
    let event_name = format!(
        "{}_{}",
        parent_id.replace(':', "_"),
        event.event_type.to_string()
    );
    let event_type_str = match &event.event_type {
        EventType::Select => "select",
        EventType::Submit => "submit",
        EventType::Click => "click",
        EventType::Change => "change",
        EventType::Load => "load",
        EventType::Save => "save",
        EventType::Cancel => "cancel",
        EventType::Delete => "delete",
        EventType::Confirm => "confirm",
        EventType::Back => "back",
        EventType::Custom(s) => s,
    };

    let event_node = EventNode {
        name: event_name.clone(),
        event_type: event_type_str.to_string(),
        params: if event.params.is_empty() {
            None
        } else {
            Some(event.params.clone())
        },
        domain: None,
    };

    let event_id = db
        .ingest_event(&event_node)
        .await
        .map_err(|e| Error::Graph(e))?;

    // Edge: parent has event
    db.ingest_edge(parent_id, &event_id, EdgeType::HasEvent, None)
        .await
        .map_err(|e| Error::Graph(e))?;

    // Handle action
    match &event.action {
        EventAction::Navigate { target, binding } => {
            let binding_str = binding.as_ref().map(|b| {
                let pairs: Vec<String> = b
                    .pairs
                    .iter()
                    .map(|(k, v)| format!("\"{}\": \"{}\"", k, expr_to_string(v)))
                    .collect();
                format!("{{{}}}", pairs.join(", "))
            });

            db.ingest_edge(
                &event_id,
                &format!("vc:{}", target),
                EdgeType::NavigationFlow,
                Some(&EdgeProperties {
                    target_param_binding: binding_str,
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| Error::Graph(e))?;
        }
        EventAction::Refresh { target, binding: _ } => {
            // Data flow to the target component
            db.ingest_edge(
                &event_id,
                &format!("comp:{}", target),
                EdgeType::DataFlow,
                None,
            )
            .await
            .map_err(|e| Error::Graph(e))?;
        }
        EventAction::ActionInvocation { name, body } => {
            let action_id = format!("action:{}", name);
            db.ingest_edge(
                &event_id,
                &action_id,
                EdgeType::TriggersAction,
                None,
            )
            .await
            .map_err(|e| Error::Graph(e))?;

            // Handle nested action body events (success/error outcomes)
            if let Some(body) = body {
                for body_event in &body.handlers {
                    let outcome_str = body_event.event_type.to_string();
                    let outcome_event_id = db
                        .ingest_event(&EventNode {
                            name: format!(
                                "{}_{}",
                                action_id.replace(':', "_"),
                                outcome_str
                            ),
                            event_type: outcome_str.clone(),
                            params: None,
                            domain: None,
                        })
                        .await
                        .map_err(|e| Error::Graph(e))?;

                    db.ingest_edge(
                        &action_id,
                        &outcome_event_id,
                        EdgeType::ActionEvent,
                        Some(&EdgeProperties {
                            outcome: Some(outcome_str),
                            ..Default::default()
                        }),
                    )
                    .await
                    .map_err(|e| Error::Graph(e))?;

                    // Recurse: outcome events can also have navigate/refresh
                    match &body_event.action {
                        EventAction::Navigate { target, binding } => {
                            let binding_str = binding.as_ref().map(|b| {
                                let pairs: Vec<String> = b
                                    .pairs
                                    .iter()
                                    .map(|(k, v)| {
                                        format!("\"{}\": \"{}\"", k, expr_to_string(v))
                                    })
                                    .collect();
                                format!("{{{}}}", pairs.join(", "))
                            });
                            db.ingest_edge(
                                &outcome_event_id,
                                &format!("vc:{}", target),
                                EdgeType::NavigationFlow,
                                Some(&EdgeProperties {
                                    target_param_binding: binding_str,
                                    ..Default::default()
                                }),
                            )
                            .await
                            .map_err(|e| Error::Graph(e))?;
                        }
                        _ => {}
                    }
                }
            }
        }
        EventAction::Stay => {
            // No edge needed for stay
        }
    }

    Ok(())
}

fn expr_to_string(expr: &Expression) -> String {
    match expr {
        Expression::Ident(s) => s.clone(),
        Expression::StringLit(s) => format!("\"{}\"", s),
        Expression::NumLit(n) => n.to_string(),
        Expression::BoolLit(b) => b.to_string(),
        Expression::FieldExpr { object, field } => {
            format!("{}.{}", expr_to_string(object), field)
        }
        Expression::BinOp { left, op, right } => {
            let op_str = match op {
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::Lt => "<",
                BinOp::Le => "<=",
                BinOp::Gt => ">",
                BinOp::Ge => ">=",
                BinOp::RegexMatch => "~=",
                BinOp::NegRegex => "!~",
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::Mod => "%",
                BinOp::And => "&&",
                BinOp::Or => "||",
            };
            format!(
                "{} {} {}",
                expr_to_string(left),
                op_str,
                expr_to_string(right)
            )
        }
        Expression::UnaryOp { op, operand } => {
            let op_str = match op {
                UnaryOp::Not => "!",
                UnaryOp::Neg => "-",
            };
            format!("{}{}", op_str, expr_to_string(operand))
        }
        Expression::Group(inner) => format!("({})", expr_to_string(inner)),
        Expression::Call { name, args } => {
            let args_str: Vec<String> = args.iter().map(expr_to_string).collect();
            format!("{}({})", name, args_str.join(", "))
        }
    }
}

#[derive(Debug, Default)]
pub struct IfmlIngestStats {
    pub view_containers: usize,
    pub containers: usize,
    pub components: usize,
    pub events: usize,
    pub parameters: usize,
    pub actions: usize,
}

impl std::fmt::Display for IfmlIngestStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} views, {} nested containers, {} components, {} events, {} params, {} actions",
            self.view_containers,
            self.containers,
            self.components,
            self.events,
            self.parameters,
            self.actions,
        )
    }
}
