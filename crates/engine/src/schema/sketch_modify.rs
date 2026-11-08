//! Contracts and result types for the commands in `cmd/sketch_modify.rs` that have them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Erase every curve of the active sketch a path crosses (SketchUp's Eraser dragged across
/// edges). Whole curves go, with the constraints on them, as `sketch.delete` removes them.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct EraseParams {
    /// The path, `[x, y]` in the sketch, in the order it was drawn: 2 to 2000 positions.
    pub points: Vec<[f64; 2]>,
    /// Only report what the path would erase; the sketch is left as it is. Default: false.
    #[serde(default)]
    pub dry_run: Option<bool>,
}

/// What `sketch.erase` returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct EraseResult {
    /// The ids of the curves erased (or, with `dry_run`, that would be), in the order the path
    /// first crosses them.
    pub erased: Vec<String>,
    /// Whether this was a dry run.
    pub dry_run: bool,
    /// The sketch's solve after the erase: `{solved, dof}`; `null` on a dry run.
    pub sketch: Value,
    /// In the warn parameter-check mode: what the call's parameters got wrong.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}
