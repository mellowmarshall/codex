use crate::CompactedItem;
use crate::ResponseItemEnvelope;
use crate::RolloutItem;
use crate::protocol::EventMsg;
use codex_extension_items::ExtensionItem;
use codex_extension_items::image_generation::ImageGenerationItem as ExtensionImageGenerationItem;
use codex_protocol::dynamic_tools::DynamicToolCallOutputContentItem;
use codex_protocol::items::DynamicToolCallItem;
use codex_protocol::items::ImageGenerationItem;
use codex_protocol::items::TurnItem;
use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::ImageGenerationEndEvent;
use codex_protocol::protocol::ItemCompletedEvent;
use codex_protocol::protocol::SubAgentActivityKind;
use codex_protocol::protocol::ThreadHistoryMode;

const OMITTED_INLINE_TOOL_IMAGE: &str = "[inline tool image omitted from persisted history]";
const OMITTED_INLINE_TOOL_AUDIO: &str = "[inline tool audio omitted from persisted history]";

/// Whether a rollout `item` should be persisted in rollout files.
pub fn is_persisted_rollout_item(item: &RolloutItem, history_mode: ThreadHistoryMode) -> bool {
    match item {
        RolloutItem::ResponseItem(item) => should_persist_response_item(&item.item),
        RolloutItem::InterAgentCommunication(_)
        | RolloutItem::InterAgentCommunicationMetadata { .. } => true,
        RolloutItem::EventMsg(ev) => should_persist_event_msg(ev, history_mode),
        RolloutItem::RealtimeItem(_) => matches!(history_mode, ThreadHistoryMode::Paginated),
        // Persist Codex executive markers so we can analyze flows (e.g., compaction, API turns).
        RolloutItem::Compacted(_)
        | RolloutItem::TurnContext(_)
        | RolloutItem::WorldState(_)
        | RolloutItem::SecurityRiskScore(_)
        | RolloutItem::SessionMeta(_) => true,
    }
}

/// Return the rollout items that should be persisted for a live append.
pub fn persisted_rollout_items(
    items: &[RolloutItem],
    history_mode: ThreadHistoryMode,
) -> Vec<RolloutItem> {
    items
        .iter()
        .filter_map(|item| persisted_rollout_item(item, history_mode))
        .collect()
}

/// Return a rollout item prepared for durable storage, or `None` when it is transient.
///
/// Keep this as the single-item persistence boundary for migration and telemetry callers. This
/// prevents them from filtering an item without also removing generated-image bytes.
pub fn persisted_rollout_item(
    item: &RolloutItem,
    history_mode: ThreadHistoryMode,
) -> Option<RolloutItem> {
    if !is_persisted_rollout_item(item, history_mode) {
        return None;
    }
    Some(clone_without_inline_media_payloads(item))
}

fn clone_without_inline_media_payloads(item: &RolloutItem) -> RolloutItem {
    match item {
        RolloutItem::ResponseItem(item) => {
            RolloutItem::ResponseItem(clone_response_envelope_without_inline_media(item))
        }
        RolloutItem::Compacted(compacted) => RolloutItem::Compacted(CompactedItem {
            message: compacted.message.clone(),
            replacement_history: compacted.replacement_history.as_ref().map(|history| {
                history
                    .iter()
                    .map(clone_response_envelope_without_inline_media)
                    .collect()
            }),
            mcp_resource_origins: compacted.mcp_resource_origins.clone(),
            window_number: compacted.window_number,
            first_window_id: compacted.first_window_id.clone(),
            previous_window_id: compacted.previous_window_id.clone(),
            window_id: compacted.window_id.clone(),
        }),
        RolloutItem::EventMsg(event) => {
            RolloutItem::EventMsg(clone_event_without_inline_media(event))
        }
        _ => item.clone(),
    }
}

fn clone_response_envelope_without_inline_media(
    envelope: &ResponseItemEnvelope,
) -> ResponseItemEnvelope {
    ResponseItemEnvelope {
        item: clone_response_without_inline_media(&envelope.item),
        metadata: envelope.metadata.clone(),
    }
}

fn clone_response_without_inline_media(item: &ResponseItem) -> ResponseItem {
    match item {
        ResponseItem::ImageGenerationCall {
            id,
            status,
            revised_prompt,
            internal_chat_message_metadata_passthrough,
            ..
        } => ResponseItem::ImageGenerationCall {
            id: id.clone(),
            status: status.clone(),
            revised_prompt: revised_prompt.clone(),
            result: String::new(),
            internal_chat_message_metadata_passthrough: internal_chat_message_metadata_passthrough
                .clone(),
        },
        ResponseItem::FunctionCallOutput {
            id,
            call_id,
            name,
            namespace,
            output,
            internal_chat_message_metadata_passthrough,
        } => ResponseItem::FunctionCallOutput {
            id: id.clone(),
            call_id: call_id.clone(),
            name: name.clone(),
            namespace: namespace.clone(),
            output: clone_output_without_inline_media(output),
            internal_chat_message_metadata_passthrough: internal_chat_message_metadata_passthrough
                .clone(),
        },
        ResponseItem::CustomToolCallOutput {
            id,
            call_id,
            name,
            output,
            internal_chat_message_metadata_passthrough,
        } => ResponseItem::CustomToolCallOutput {
            id: id.clone(),
            call_id: call_id.clone(),
            name: name.clone(),
            output: clone_output_without_inline_media(output),
            internal_chat_message_metadata_passthrough: internal_chat_message_metadata_passthrough
                .clone(),
        },
        _ => item.clone(),
    }
}

fn clone_output_without_inline_media(
    output: &FunctionCallOutputPayload,
) -> FunctionCallOutputPayload {
    FunctionCallOutputPayload {
        body: match &output.body {
            FunctionCallOutputBody::Text(text) => FunctionCallOutputBody::Text(text.clone()),
            FunctionCallOutputBody::ContentItems(items) => FunctionCallOutputBody::ContentItems(
                items
                    .iter()
                    .map(clone_function_output_item_without_inline_media)
                    .collect(),
            ),
        },
        success: output.success,
    }
}

fn clone_function_output_item_without_inline_media(
    item: &FunctionCallOutputContentItem,
) -> FunctionCallOutputContentItem {
    match item {
        FunctionCallOutputContentItem::InputImage { image_url, .. }
            if image_url.starts_with("data:image/") =>
        {
            FunctionCallOutputContentItem::InputText {
                text: OMITTED_INLINE_TOOL_IMAGE.to_string(),
            }
        }
        FunctionCallOutputContentItem::InputAudio { audio_url }
            if audio_url.starts_with("data:audio/") =>
        {
            FunctionCallOutputContentItem::InputText {
                text: OMITTED_INLINE_TOOL_AUDIO.to_string(),
            }
        }
        _ => item.clone(),
    }
}

fn clone_event_without_inline_media(event: &EventMsg) -> EventMsg {
    match event {
        EventMsg::ItemCompleted(event) => EventMsg::ItemCompleted(ItemCompletedEvent {
            thread_id: event.thread_id,
            turn_id: event.turn_id.clone(),
            item: clone_turn_item_without_inline_media(&event.item),
            started_at_ms: event.started_at_ms,
            completed_at_ms: event.completed_at_ms,
        }),
        EventMsg::ImageGenerationEnd(event) => {
            EventMsg::ImageGenerationEnd(ImageGenerationEndEvent {
                call_id: event.call_id.clone(),
                status: event.status.clone(),
                revised_prompt: event.revised_prompt.clone(),
                result: String::new(),
                transparent_background: event.transparent_background,
                failure: event.failure.clone(),
                saved_path: event.saved_path.clone(),
            })
        }
        _ => event.clone(),
    }
}

fn clone_turn_item_without_inline_media(item: &TurnItem) -> TurnItem {
    match item {
        TurnItem::Extension(ExtensionItem::ImageGeneration(image)) => TurnItem::Extension(
            ExtensionItem::ImageGeneration(ExtensionImageGenerationItem {
                id: image.id.clone(),
                status: image.status.clone(),
                revised_prompt: image.revised_prompt.clone(),
                result: String::new(),
                transparent_background: image.transparent_background,
                failure: image.failure.clone(),
                saved_path: image.saved_path.clone(),
            }),
        ),
        TurnItem::ImageGeneration(image) => TurnItem::ImageGeneration(ImageGenerationItem {
            id: image.id.clone(),
            status: image.status.clone(),
            revised_prompt: image.revised_prompt.clone(),
            result: String::new(),
            saved_path: image.saved_path.clone(),
        }),
        TurnItem::DynamicToolCall(item) => TurnItem::DynamicToolCall(DynamicToolCallItem {
            id: item.id.clone(),
            namespace: item.namespace.clone(),
            tool: item.tool.clone(),
            arguments: item.arguments.clone(),
            status: item.status,
            content_items: item.content_items.as_ref().map(|items| {
                items
                    .iter()
                    .map(clone_dynamic_tool_item_without_inline_media)
                    .collect()
            }),
            success: item.success,
            error: item.error.clone(),
            duration: item.duration,
        }),
        _ => item.clone(),
    }
}

fn clone_dynamic_tool_item_without_inline_media(
    item: &DynamicToolCallOutputContentItem,
) -> DynamicToolCallOutputContentItem {
    match item {
        DynamicToolCallOutputContentItem::InputImage { image_url }
            if image_url.starts_with("data:image/") =>
        {
            DynamicToolCallOutputContentItem::InputText {
                text: OMITTED_INLINE_TOOL_IMAGE.to_string(),
            }
        }
        DynamicToolCallOutputContentItem::InputAudio { audio_url }
            if audio_url.starts_with("data:audio/") =>
        {
            DynamicToolCallOutputContentItem::InputText {
                text: OMITTED_INLINE_TOOL_AUDIO.to_string(),
            }
        }
        _ => item.clone(),
    }
}

/// Whether a `ResponseItem` should be persisted in rollout files.
#[inline]
pub fn should_persist_response_item(item: &ResponseItem) -> bool {
    match item {
        ResponseItem::Message { .. }
        | ResponseItem::AgentMessage { .. }
        | ResponseItem::Reasoning { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::ToolSearchCall { .. }
        | ResponseItem::FunctionCallOutput { .. }
        | ResponseItem::ToolSearchOutput { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::CustomToolCallOutput { .. }
        | ResponseItem::WebSearchCall { .. }
        | ResponseItem::ImageGenerationCall { .. }
        | ResponseItem::Compaction { .. }
        | ResponseItem::ContextCompaction { .. } => true,
        ResponseItem::AdditionalTools { .. }
        | ResponseItem::CompactionTrigger { .. }
        | ResponseItem::Other => false,
    }
}

/// Whether a `ResponseItem` should be persisted for the memories.
#[inline]
pub fn should_persist_response_item_for_memories(item: &ResponseItem) -> bool {
    match item {
        ResponseItem::Message { role, .. } => role != "developer",
        ResponseItem::AgentMessage { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::ToolSearchCall { .. }
        | ResponseItem::FunctionCallOutput { .. }
        | ResponseItem::ToolSearchOutput { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::CustomToolCallOutput { .. }
        | ResponseItem::WebSearchCall { .. } => true,
        ResponseItem::AdditionalTools { .. }
        | ResponseItem::Reasoning { .. }
        | ResponseItem::ImageGenerationCall { .. }
        | ResponseItem::Compaction { .. }
        | ResponseItem::CompactionTrigger { .. }
        | ResponseItem::ContextCompaction { .. }
        | ResponseItem::Other => false,
    }
}

/// Whether an `EventMsg` should be persisted in rollout files.
#[inline]
pub fn should_persist_event_msg(ev: &EventMsg, history_mode: ThreadHistoryMode) -> bool {
    match ev {
        EventMsg::ItemCompleted(event) => {
            // Paginated rollouts store TurnItems.
            // Legacy rollouts keep only items with no lossless raw ResponseItem or legacy
            // equivalent.
            matches!(history_mode, ThreadHistoryMode::Paginated)
                || matches!(
                    event.item,
                    TurnItem::FunctionCallOutput(_)
                        | TurnItem::Plan(_)
                        | TurnItem::Extension(ExtensionItem::Sleep(_))
                )
                || matches!(
                    &event.item,
                    TurnItem::SubAgentActivity(item)
                        if item.kind == SubAgentActivityKind::Completed
                )
        }
        EventMsg::TokenCount(_)
        | EventMsg::ThreadGoalUpdated(_)
        | EventMsg::ThreadRolledBack(_)
        | EventMsg::TurnAborted(_)
        | EventMsg::TurnStarted(_)
        | EventMsg::TurnComplete(_)
        | EventMsg::ThreadSettingsApplied(_) => true,

        // Only persist these legacy events when the thread's history mode is Legacy.
        // New, paginated rollouts persist ItemCompleted events with TurnItems.
        EventMsg::UserMessage(_)
        | EventMsg::AgentMessage(_)
        | EventMsg::AgentReasoning(_)
        | EventMsg::AgentReasoningRawContent(_)
        | EventMsg::EnteredReviewMode(_)
        | EventMsg::ExitedReviewMode(_)
        | EventMsg::PatchApplyEnd(_)
        | EventMsg::ContextCompacted(_)
        | EventMsg::McpToolCallEnd(_)
        | EventMsg::WebSearchEnd(_)
        | EventMsg::ImageGenerationEnd(_) => {
            matches!(history_mode, ThreadHistoryMode::Legacy)
        }
        EventMsg::SubAgentActivity(event) => {
            matches!(history_mode, ThreadHistoryMode::Legacy)
                && event.kind != SubAgentActivityKind::Completed
        }

        // Transient, non-durable events.
        EventMsg::Error(_)
        | EventMsg::ThreadQueueChanged(_)
        | EventMsg::GuardianAssessment(_)
        | EventMsg::ExecCommandEnd(_)
        | EventMsg::ViewImageToolCall(_)
        | EventMsg::CollabAgentSpawnEnd(_)
        | EventMsg::CollabAgentInteractionEnd(_)
        | EventMsg::CollabWaitingEnd(_)
        | EventMsg::CollabCloseEnd(_)
        | EventMsg::CollabResumeEnd(_)
        | EventMsg::DynamicToolCallRequest(_)
        | EventMsg::DynamicToolCallResponse(_)
        | EventMsg::Warning(_)
        | EventMsg::GuardianWarning(_)
        | EventMsg::RealtimeConversationStarted(_)
        | EventMsg::RealtimeConversationSdp(_)
        | EventMsg::RealtimeConversationRealtime(_)
        | EventMsg::RealtimeConversationClosed(_)
        | EventMsg::SafetyBuffering(_)
        | EventMsg::ModelReroute(_)
        | EventMsg::ModelVerification(_)
        | EventMsg::TurnModerationMetadata(_)
        | EventMsg::AgentReasoningSectionBreak(_)
        | EventMsg::RawResponseItem(_)
        | EventMsg::RawResponseCompleted(_)
        | EventMsg::SessionConfigured(_)
        | EventMsg::EnvironmentConnected(_)
        | EventMsg::EnvironmentDisconnected(_)
        | EventMsg::McpToolCallBegin(_)
        | EventMsg::ExecCommandBegin(_)
        | EventMsg::TerminalInteraction(_)
        | EventMsg::ExecCommandOutputDelta(_)
        | EventMsg::ExecApprovalRequest(_)
        | EventMsg::RequestPermissions(_)
        | EventMsg::RequestUserInput(_)
        | EventMsg::ElicitationRequest(_)
        | EventMsg::ApplyPatchApprovalRequest(_)
        | EventMsg::StreamError(_)
        | EventMsg::PatchApplyBegin(_)
        | EventMsg::PatchApplyUpdated(_)
        | EventMsg::TurnDiff(_)
        | EventMsg::RealtimeConversationListVoicesResponse(_)
        | EventMsg::McpStartupUpdate(_)
        | EventMsg::McpStartupComplete(_)
        | EventMsg::WebSearchBegin(_)
        | EventMsg::PlanUpdate(_)
        | EventMsg::ShutdownComplete
        | EventMsg::DeprecationNotice(_)
        | EventMsg::ItemStarted(_)
        | EventMsg::HookStarted(_)
        | EventMsg::HookCompleted(_)
        | EventMsg::AgentMessageContentDelta(_)
        | EventMsg::PlanDelta(_)
        | EventMsg::ReasoningContentDelta(_)
        | EventMsg::ReasoningRawContentDelta(_)
        | EventMsg::ImageGenerationBegin(_)
        | EventMsg::CollabAgentSpawnBegin(_)
        | EventMsg::CollabAgentInteractionBegin(_)
        | EventMsg::CollabWaitingBegin(_)
        | EventMsg::CollabCloseBegin(_)
        | EventMsg::CollabResumeBegin(_) => false,
    }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
