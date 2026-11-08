//! Contracts and result types for sketch editing and completion in `cmd/sketch.rs`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use serde_json::Value;

use super::types::{IdOrName, present};

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct EditParams {
    pub sketch: IdOrName,
    /// When present, temporarily roll to this sketch and restore this position when the edit
    /// ends. Null means the end of the timeline; omission leaves the timeline in place.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<usize>")]
    pub restore_marker: Option<Option<usize>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ExitParams {
    /// Restore the temporary editor marker (default true). False transfers it directly to
    /// another editor without rebuilding the intermediate timeline.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "bool")]
    pub restore_marker: Option<bool>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct EditResult {
    pub sketch: u64,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FinishResult {
    pub sketch: Option<u64>,
    pub profiles: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CancelResult {
    pub sketch: u64,
    pub removed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}

/// Draw a pie in the active sketch (SketchUp's Pie): a centre point arc and the two radius lines
/// that close it, sharing its centre and ends.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct PieParams {
    /// The centre: `[x, y]` or a point reference.
    pub center: Value,
    /// The arc's start: `[x, y]` or a point reference. Its distance from `center` is the radius.
    pub start: Value,
    /// Where the arc ends, counter-clockwise from `start` (only its direction from `center`
    /// counts). Give it or `sweep`.
    #[serde(default)]
    pub end: Option<Value>,
    /// The arc's sweep in degrees, signed (negative: clockwise), in place of `end`.
    #[serde(default)]
    pub sweep: Option<f64>,
    /// Draw it as construction geometry. Default: false.
    #[serde(default)]
    pub construction: Option<bool>,
}

/// What `sketch.arc.pie` returns: the arc and its two radius lines, and the sketch's solve.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PieResult {
    /// The arc's id, then the radius lines' (to the start, then to the end).
    pub curves: Vec<String>,
    /// Constraints added (none: the curves share their points).
    pub constraints: Vec<String>,
    /// The sketch's solve: `{solved, dof}`.
    pub sketch: Value,
    /// In the warn parameter-check mode: what the call's parameters got wrong.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}
