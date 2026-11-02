//! MCP's compact tool surface, mapped onto the existing desktop operations.
use super::ApiError;
use rmcp::model::{Tool, ToolAnnotations};
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use serde_json::{json, Value};

const HTTP_ONLY: &[&str] = &[
    "copy_text",
    "clear_logs",
    "clear_transfers",
    "open_directory",
    "preview_file",
    "accept_transfer",
    "reject_transfer",
    "pause_transfer",
    "resume_transfer",
    "cancel_transfer",
    "dismiss_transfer",
];

#[derive(Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
enum TransferAction {
    Accept,
    Reject,
    Pause,
    Resume,
    Cancel,
    Dismiss,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OperateTransfer {
    action: TransferAction,
    transfer_key: String,
    /// Only for action=accept: absolute destination directory. Omit to use the
    /// GUI's default directory.
    save_dir: Option<String>,
}

pub(super) fn tools(mut operations: Vec<Tool>) -> Vec<Tool> {
    operations.retain(|tool| !HTTP_ONLY.contains(&tool.name.as_ref()));
    let schema = serde_json::to_value(schema_for!(OperateTransfer)).expect("transfer schema");
    operations.push(
        Tool::new(
            "operate_transfer",
            "Operate on a transfer from get_snapshot or wait_for_file. accept/reject decide an incoming offer; pause/resume control an active/paused transfer; cancel ends an active, paused or awaiting-accept transfer; dismiss removes a finished record without deleting files. saveDir applies only to accept.",
            schema.as_object().unwrap().clone(),
        )
        .annotate(ToolAnnotations::new().read_only(false).destructive(true).open_world(true)),
    );
    operations
}

pub(super) fn resolve(name: String, arguments: Value) -> Result<(String, Value), ApiError> {
    if HTTP_ONLY.contains(&name.as_str()) {
        return Err(ApiError::not_found(&name));
    }
    if name != "operate_transfer" {
        return Ok((name, arguments));
    }
    let args: OperateTransfer =
        serde_json::from_value(arguments).map_err(|error| ApiError::invalid(error.to_string()))?;
    if args.action != TransferAction::Accept && args.save_dir.is_some() {
        return Err(ApiError::invalid(
            "saveDir is only supported for action=accept",
        ));
    }
    let operation = match args.action {
        TransferAction::Accept => "accept_transfer",
        TransferAction::Reject => "reject_transfer",
        TransferAction::Pause => "pause_transfer",
        TransferAction::Resume => "resume_transfer",
        TransferAction::Cancel => "cancel_transfer",
        TransferAction::Dismiss => "dismiss_transfer",
    };
    let mut arguments = json!({"transferKey":args.transfer_key});
    if let Some(save_dir) = args.save_dir {
        arguments["saveDir"] = json!(save_dir);
    }
    Ok((operation.into(), arguments))
}
