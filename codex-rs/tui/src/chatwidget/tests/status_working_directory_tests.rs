use super::*;
use pretty_assertions::assert_eq;

fn command(id: &str, cwd: &Path, source: ExecCommandSource) -> AppServerThreadItem {
    AppServerThreadItem::CommandExecution {
        id: id.to_string(),
        command: "pwd".to_string(),
        cwd: codex_utils_path_uri::LegacyAppPathString::from_path(cwd),
        plugin_id: None,
        script_path: None,
        process_id: None,
        source,
        status: AppServerCommandExecutionStatus::InProgress,
        command_actions: Vec::new(),
        aggregated_output: None,
        exit_code: None,
        duration_ms: None,
    }
}

#[tokio::test]
async fn status_working_directory_follows_only_this_sessions_latest_start() {
    let (mut first, _rx, _ops) = make_chatwidget_manual(/*model_override*/ None).await;
    let (mut second, _rx2, _ops2) = make_chatwidget_manual(/*model_override*/ None).await;
    first.config.tui_status_line = Some(vec!["current-dir".to_string(), "git-branch".to_string()]);
    second.config.tui_status_line = first.config.tui_status_line.clone();
    let original = first.config.cwd.clone();
    let alpha = test_path_buf("/worktrees/alpha");
    let beta = test_path_buf("/worktrees/beta");
    let gamma = test_path_buf("/worktrees/gamma");
    let old = command("old", &alpha, ExecCommandSource::UnifiedExecStartup);
    handle_exec_begin(&mut first, old.clone());
    first.set_status_line_branch(alpha.clone(), Some("alpha".to_string()));
    handle_exec_begin(
        &mut second,
        command("second", &gamma, ExecCommandSource::Agent),
    );
    handle_exec_begin(
        &mut first,
        command("new", &beta, ExecCommandSource::UnifiedExecStartup),
    );

    assert_eq!(first.config.cwd, original);
    assert_eq!(first.current_cwd.as_deref(), Some(original.as_path()));
    assert_eq!(
        first.status_line_value_for_item(StatusLineItem::CurrentDir),
        Some(beta.display().to_string())
    );
    assert_eq!(
        second.status_line_value_for_item(StatusLineItem::CurrentDir),
        Some(gamma.display().to_string())
    );
    assert_eq!(first.status_line_branch, None);
    assert_eq!(
        first.status_line_branch_cwd.as_deref(),
        Some(beta.as_path())
    );

    // Polling an older process and its eventual completion must not rewind the display.
    handle_exec_begin(
        &mut first,
        command("poll", &alpha, ExecCommandSource::UnifiedExecInteraction),
    );
    end_exec(&mut first, old, "done", "", /*exit_code*/ 0);
    first.set_status_line_branch(alpha, Some("stale-alpha".to_string()));
    assert_eq!(
        first.status_line_value_for_item(StatusLineItem::CurrentDir),
        Some(beta.display().to_string())
    );
    assert_eq!(first.status_line_branch, None);
    first.set_status_line_branch(beta, Some("beta".to_string()));
    second.set_status_line_branch(gamma, Some("gamma".to_string()));
    let rendered = format!(
        "First: {}\nSecond: {}",
        status_line_text(&first).unwrap(),
        status_line_text(&second).unwrap()
    );
    insta::assert_snapshot!(rendered.replace('\\', "/").replace("C:/", "/"), @r"
    First: /worktrees/beta · beta
    Second: /worktrees/gamma · gamma
    ");
}

#[tokio::test]
async fn status_working_directory_restores_history_without_resetting_on_model_settings() {
    let (mut chat, _rx, _ops) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    let original = chat.config.cwd.clone();
    let alpha = test_path_buf("/worktrees/alpha");
    let beta = test_path_buf("/worktrees/beta");
    for (id, cwd) in [("older", &alpha), ("newer", &beta)] {
        let mut item = command(id, cwd, ExecCommandSource::Agent);
        if let AppServerThreadItem::CommandExecution { status, .. } = &mut item {
            *status = AppServerCommandExecutionStatus::Completed;
        }
        chat.replay_thread_item(
            item,
            "turn-1".to_string(),
            ReplayKind::ResumeInitialMessages,
        );
    }
    let mut newer = command(
        "newer-completed",
        &beta,
        ExecCommandSource::UnifiedExecStartup,
    );
    if let AppServerThreadItem::CommandExecution { status, .. } = &mut newer {
        *status = AppServerCommandExecutionStatus::Completed;
    }
    chat.replay_thread_turns(
        vec![AppServerTurn {
            id: "turn-2".to_string(),
            items_view: codex_app_server_protocol::TurnItemsView::Full,
            items: vec![
                command(
                    "older-running",
                    &alpha,
                    ExecCommandSource::UnifiedExecStartup,
                ),
                newer,
            ],
            status: AppServerTurnStatus::InProgress,
            error: None,
            started_at: Some(0),
            completed_at: None,
            duration_ms: None,
        }],
        ReplayKind::ResumeInitialMessages,
    );
    assert_eq!(
        chat.status_line_value_for_item(StatusLineItem::CurrentDir),
        Some(beta.display().to_string())
    );
    let settings = codex_app_server_protocol::ThreadSettings {
        cwd: original,
        approval_policy: AskForApproval::Never,
        approvals_reviewer: codex_app_server_protocol::ApprovalsReviewer::User,
        sandbox_policy: codex_app_server_protocol::SandboxPolicy::DangerFullAccess,
        active_permission_profile: None,
        model: chat.current_model().to_string(),
        model_provider: chat.config.model_provider_id.clone(),
        service_tier: None,
        effort: Some(ReasoningEffortConfig::High),
        summary: None,
        collaboration_mode: chat.effective_collaboration_mode(),
        multi_agent_mode: Default::default(),
        personality: None,
    };
    chat.handle_server_notification(
        ServerNotification::ThreadSettingsUpdated(
            codex_app_server_protocol::ThreadSettingsUpdatedNotification {
                thread_id: chat.thread_id.map(|id| id.to_string()).unwrap_or_default(),
                thread_settings: settings.clone(),
            },
        ),
        /*replay_kind*/ None,
    );
    assert_eq!(
        chat.status_line_value_for_item(StatusLineItem::CurrentDir),
        Some(beta.display().to_string())
    );
    chat.handle_server_notification(
        ServerNotification::ThreadSettingsUpdated(
            codex_app_server_protocol::ThreadSettingsUpdatedNotification {
                thread_id: chat.thread_id.map(|id| id.to_string()).unwrap_or_default(),
                thread_settings: codex_app_server_protocol::ThreadSettings {
                    cwd: alpha.abs(),
                    ..settings
                },
            },
        ),
        /*replay_kind*/ None,
    );
    assert_eq!(
        chat.status_line_value_for_item(StatusLineItem::CurrentDir),
        Some(alpha.display().to_string())
    );
    assert_eq!(chat.status_line_command_cwd, None);
}
