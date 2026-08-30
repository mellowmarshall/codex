use codex_extension_items::ExtensionItem;
use codex_extension_items::image_generation::ImageGenerationItem;
use codex_protocol::items::FunctionCallOutputItem;
use codex_protocol::items::TurnItem;
use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::ItemCompletedEvent;
use codex_protocol::protocol::ThreadHistoryMode;
use codex_utils_absolute_path::test_support::PathBufExt;
use codex_utils_absolute_path::test_support::test_path_buf;
use pretty_assertions::assert_eq;

use super::persisted_rollout_items;
use crate::CompactedItem;
use crate::ResponseItemEnvelope;
use crate::RolloutItem;

#[test]
fn persisted_image_generation_uses_saved_file_instead_of_inline_result() {
    let thread_id = codex_protocol::ThreadId::default();
    let item = RolloutItem::EventMsg(EventMsg::ItemCompleted(ItemCompletedEvent {
        thread_id,
        turn_id: "turn-1".to_string(),
        item: TurnItem::Extension(ExtensionItem::ImageGeneration(ImageGenerationItem {
            id: "image-1".to_string(),
            status: "completed".to_string(),
            revised_prompt: Some("prompt".to_string()),
            result: "large-base64-result".to_string(),
            transparent_background: None,
            failure: None,
            saved_path: Some(test_path_buf("/tmp/image-1.png").abs()),
            imagegen_request_id: Some("request-1".to_string()),
        })),
        started_at_ms: Some(1),
        completed_at_ms: 2,
    }));

    let persisted = persisted_rollout_items(&[item], ThreadHistoryMode::Paginated);

    let RolloutItem::EventMsg(EventMsg::ItemCompleted(completed)) = &persisted[0] else {
        panic!("expected completed item");
    };
    let TurnItem::Extension(ExtensionItem::ImageGeneration(image)) = &completed.item else {
        panic!("expected generated image");
    };
    assert_eq!(image.result, "");
    assert_eq!(
        image.saved_path,
        Some(test_path_buf("/tmp/image-1.png").abs())
    );
    assert_eq!(image.imagegen_request_id.as_deref(), Some("request-1"));
}

#[test]
fn unsaved_image_generation_drops_inline_result() {
    let thread_id = codex_protocol::ThreadId::default();
    let item = RolloutItem::EventMsg(EventMsg::ItemCompleted(ItemCompletedEvent {
        thread_id,
        turn_id: "turn-1".to_string(),
        item: TurnItem::Extension(ExtensionItem::ImageGeneration(ImageGenerationItem {
            id: "image-1".to_string(),
            status: "completed".to_string(),
            revised_prompt: Some("prompt".to_string()),
            result: "only-durable-copy".to_string(),
            transparent_background: None,
            failure: None,
            saved_path: None,
            imagegen_request_id: None,
        })),
        started_at_ms: Some(1),
        completed_at_ms: 2,
    }));

    let persisted = persisted_rollout_items(&[item], ThreadHistoryMode::Paginated);

    let RolloutItem::EventMsg(EventMsg::ItemCompleted(completed)) = &persisted[0] else {
        panic!("expected completed item");
    };
    let TurnItem::Extension(ExtensionItem::ImageGeneration(image)) = &completed.item else {
        panic!("expected generated image");
    };
    assert_eq!(image.result, "");
    assert_eq!(image.saved_path, None);
}

#[test]
fn persisted_function_call_output_replaces_inline_media_in_both_history_modes() {
    let thread_id = codex_protocol::ThreadId::default();
    let item = RolloutItem::EventMsg(EventMsg::ItemCompleted(ItemCompletedEvent {
        thread_id,
        turn_id: "turn-1".to_string(),
        item: TurnItem::FunctionCallOutput(FunctionCallOutputItem {
            id: "function-1".to_string(),
            name: "view_image".to_string(),
            namespace: Some("functions".to_string()),
            output: FunctionCallOutputBody::ContentItems(vec![
                FunctionCallOutputContentItem::InputText {
                    text: "kept".to_string(),
                },
                FunctionCallOutputContentItem::InputImage {
                    image_url: "data:image/png;base64,large-result".to_string(),
                    detail: None,
                },
                FunctionCallOutputContentItem::InputAudio {
                    audio_url: "data:audio/wav;base64,large-result".to_string(),
                },
                FunctionCallOutputContentItem::EncryptedContent {
                    encrypted_content: "kept encrypted".to_string(),
                },
            ]),
        }),
        started_at_ms: Some(1),
        completed_at_ms: 2,
    }));

    for history_mode in [ThreadHistoryMode::Legacy, ThreadHistoryMode::Paginated] {
        let persisted = persisted_rollout_items(std::slice::from_ref(&item), history_mode);

        let RolloutItem::EventMsg(EventMsg::ItemCompleted(completed)) = &persisted[0] else {
            panic!("expected completed item");
        };
        let TurnItem::FunctionCallOutput(output) = &completed.item else {
            panic!("expected function call output");
        };
        assert_eq!(output.id, "function-1");
        assert_eq!(output.name, "view_image");
        assert_eq!(output.namespace.as_deref(), Some("functions"));
        assert_eq!(
            output.output,
            FunctionCallOutputBody::ContentItems(vec![
                FunctionCallOutputContentItem::InputText {
                    text: "kept".to_string(),
                },
                FunctionCallOutputContentItem::InputText {
                    text: "[inline tool image omitted from persisted history]".to_string(),
                },
                FunctionCallOutputContentItem::InputText {
                    text: "[inline tool audio omitted from persisted history]".to_string(),
                },
                FunctionCallOutputContentItem::EncryptedContent {
                    encrypted_content: "kept encrypted".to_string(),
                },
            ])
        );
    }
}

fn image_generation_response(result: &str) -> ResponseItem {
    ResponseItem::ImageGenerationCall {
        id: None,
        status: "completed".to_string(),
        revised_prompt: Some("prompt".to_string()),
        result: result.to_string(),
        internal_chat_message_metadata_passthrough: None,
    }
}

#[test]
fn persisted_response_image_generation_drops_inline_result() {
    let item = RolloutItem::ResponseItem(ResponseItemEnvelope::new(image_generation_response(
        "large-base64-result",
    )));

    let persisted = persisted_rollout_items(&[item], ThreadHistoryMode::Paginated);

    let RolloutItem::ResponseItem(item) = &persisted[0] else {
        panic!("expected response item");
    };
    let ResponseItem::ImageGenerationCall { result, .. } = &item.item else {
        panic!("expected image generation call");
    };
    assert_eq!(result, "");
}

#[test]
fn persisted_tool_output_replaces_inline_image_and_keeps_text() {
    let item = RolloutItem::ResponseItem(ResponseItemEnvelope::new(
        ResponseItem::CustomToolCallOutput {
            id: None,
            call_id: "call-1".to_string(),
            name: Some("view_image".to_string()),
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::ContentItems(vec![
                    FunctionCallOutputContentItem::InputText {
                        text: "kept".to_string(),
                    },
                    FunctionCallOutputContentItem::InputImage {
                        image_url: "data:image/png;base64,large-result".to_string(),
                        detail: None,
                    },
                    FunctionCallOutputContentItem::InputAudio {
                        audio_url: "data:audio/wav;base64,large-result".to_string(),
                    },
                ]),
                success: Some(true),
            },
            internal_chat_message_metadata_passthrough: None,
        },
    ));

    let persisted = persisted_rollout_items(&[item], ThreadHistoryMode::Paginated);

    let RolloutItem::ResponseItem(item) = &persisted[0] else {
        panic!("expected response item");
    };
    let ResponseItem::CustomToolCallOutput { output, .. } = &item.item else {
        panic!("expected custom tool output");
    };
    assert_eq!(
        output.content_items(),
        Some(
            [
                FunctionCallOutputContentItem::InputText {
                    text: "kept".to_string(),
                },
                FunctionCallOutputContentItem::InputText {
                    text: "[inline tool image omitted from persisted history]".to_string(),
                },
                FunctionCallOutputContentItem::InputText {
                    text: "[inline tool audio omitted from persisted history]".to_string(),
                },
            ]
            .as_slice()
        )
    );
}

#[test]
fn persisted_compaction_drops_nested_inline_image_result() {
    let item = RolloutItem::Compacted(CompactedItem {
        message: "summary".to_string(),
        replacement_history: Some(vec![ResponseItemEnvelope::new(image_generation_response(
            "copied-base64-result",
        ))]),
        mcp_resource_origins: None,
        window_number: Some(1),
        first_window_id: None,
        previous_window_id: None,
        window_id: None,
    });

    let persisted = persisted_rollout_items(&[item], ThreadHistoryMode::Paginated);

    let RolloutItem::Compacted(compacted) = &persisted[0] else {
        panic!("expected compacted item");
    };
    let ResponseItem::ImageGenerationCall { result, .. } =
        &compacted.replacement_history.as_ref().unwrap()[0].item
    else {
        panic!("expected nested image generation call");
    };
    assert_eq!(result, "");
}
