use chrono::{DateTime, Utc};
use uuid::Uuid;

pub type IdentifierType = Uuid;
pub type CodeType = String;
pub type TextType = String;
pub type DurationType = chrono::Duration;
pub type MeasureType = rust_decimal::Decimal;
pub type IndicatorType = bool;
pub type StatusType = String;
