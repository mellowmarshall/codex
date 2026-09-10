//! Resolve display context from actual command starts, without changing execution settings.

use codex_app_server_protocol::CommandExecutionSource;
use codex_app_server_protocol::CommandExecutionStatus;
use codex_app_server_protocol::ThreadItem;
use std::path::PathBuf;

pub(crate) fn command_cwd(item: &ThreadItem) -> Option<PathBuf> {
    let ThreadItem::CommandExecution {
        cwd,
        source: CommandExecutionSource::Agent | CommandExecutionSource::UnifiedExecStartup,
        status:
            CommandExecutionStatus::InProgress
            | CommandExecutionStatus::Completed
            | CommandExecutionStatus::Failed,
        ..
    } = item
    else {
        return None;
    };
    cwd.to_inferred_path_uri().map(|path| path.to_path_buf())
}
