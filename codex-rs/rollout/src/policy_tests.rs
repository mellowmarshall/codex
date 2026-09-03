use codex_extension_items::ExtensionItem;
use codex_extension_items::image_generation::ImageGenerationItem;
use codex_extension_items::web_search::WebSearchItem;
use codex_protocol::dynamic_tools::DynamicToolCallOutputContentItem;
use codex_protocol::items::DynamicToolCallItem;
use codex_protocol::items::DynamicToolCallStatus;
use codex_protocol::items::FunctionCallOutputItem;
use codex_protocol::items::McpToolCallItem;
use codex_protocol::items::McpToolCallStatus;
use codex_protocol::items::TurnItem;
use codex_protocol::items::UserMessageItem;
use codex_protocol::mcp::CallToolResult;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::ItemCompletedEvent;
use codex_protocol::protocol::McpInvocation;
use codex_protocol::protocol::McpToolCallEndEvent;
use codex_protocol::protocol::ThreadHistoryMode;
use codex_protocol::protocol::UserMessageEvent;
use codex_protocol::protocol::WebSearchEndEvent;
use codex_protocol::user_input::UserInput;
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
                    image_url: "DATA:APPLICATION/OCTET-STREAM;base64,large-result".to_string(),
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
fn persisted_response_user_message_replaces_only_inline_media() {
    let item = RolloutItem::ResponseItem(ResponseItemEnvelope::new(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![
            ContentItem::InputText {
                text: "kept".to_string(),
            },
            ContentItem::InputImage {
                image_url: "data:application/octet-stream;base64,image".to_string(),
                detail: None,
            },
            ContentItem::InputImage {
                image_url: "https://example.com/image.png".to_string(),
                detail: None,
            },
            ContentItem::InputAudio {
                audio_url: "data:audio/wav;base64,audio".to_string(),
            },
            ContentItem::InputAudio {
                audio_url: "https://example.com/audio.wav".to_string(),
            },
        ],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }));

    let persisted = persisted_rollout_items(&[item], ThreadHistoryMode::Paginated);
    let RolloutItem::ResponseItem(item) = &persisted[0] else {
        panic!("expected response item");
    };
    let ResponseItem::Message { content, .. } = &item.item else {
        panic!("expected response message");
    };
    assert_eq!(
        content,
        &vec![
            ContentItem::InputText {
                text: "kept".to_string(),
            },
            ContentItem::InputText {
                text: "[inline user image omitted from persisted history]".to_string(),
            },
            ContentItem::InputImage {
                image_url: "https://example.com/image.png".to_string(),
                detail: None,
            },
            ContentItem::InputText {
                text: "[inline user audio omitted from persisted history]".to_string(),
            },
            ContentItem::InputAudio {
                audio_url: "https://example.com/audio.wav".to_string(),
            },
        ]
    );
}

#[test]
fn persisted_turn_user_message_replaces_only_inline_media() {
    let item = RolloutItem::EventMsg(EventMsg::ItemCompleted(ItemCompletedEvent {
        thread_id: codex_protocol::ThreadId::default(),
        turn_id: "turn-1".to_string(),
        item: TurnItem::UserMessage(UserMessageItem {
            id: "user-1".to_string(),
            client_id: Some("client-1".to_string()),
            content: vec![
                UserInput::Image {
                    image_url: "DATA:APPLICATION/OCTET-STREAM;base64,image".to_string(),
                    detail: None,
                },
                UserInput::Image {
                    image_url: "https://example.com/image.png".to_string(),
                    detail: None,
                },
                UserInput::Audio {
                    audio_url: "data:audio/wav;base64,audio".to_string(),
                },
                UserInput::LocalAudio {
                    path: "/tmp/audio.wav".into(),
                },
            ],
        }),
        started_at_ms: Some(1),
        completed_at_ms: 2,
    }));

    let persisted = persisted_rollout_items(&[item], ThreadHistoryMode::Paginated);
    let RolloutItem::EventMsg(EventMsg::ItemCompleted(completed)) = &persisted[0] else {
        panic!("expected completed item");
    };
    let TurnItem::UserMessage(user) = &completed.item else {
        panic!("expected user message");
    };
    assert_eq!(user.client_id.as_deref(), Some("client-1"));
    assert_eq!(
        user.content,
        vec![
            UserInput::Text {
                text: "[inline user image omitted from persisted history]".to_string(),
                text_elements: Vec::new(),
            },
            UserInput::Image {
                image_url: "https://example.com/image.png".to_string(),
                detail: None,
            },
            UserInput::Text {
                text: "[inline user audio omitted from persisted history]".to_string(),
                text_elements: Vec::new(),
            },
            UserInput::LocalAudio {
                path: "/tmp/audio.wav".into(),
            },
        ]
    );
}

#[test]
fn persisted_legacy_user_message_drops_inline_urls_and_keeps_metadata_aligned() {
    let item = RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
        client_id: Some("client-1".to_string()),
        message: "kept".to_string(),
        images: Some(vec![
            "data:image/png;base64,image".to_string(),
            "https://example.com/image.png".to_string(),
        ]),
        image_details: vec![None, Some(codex_protocol::models::ImageDetail::High)],
        local_images: vec!["/tmp/image.png".into()],
        local_image_details: vec![Some(codex_protocol::models::ImageDetail::Low)],
        audio: Some(vec![
            "data:audio/wav;base64,audio".to_string(),
            "https://example.com/audio.wav".to_string(),
        ]),
        local_audio: vec!["/tmp/audio.wav".into()],
        text_elements: Vec::new(),
    }));

    let persisted = persisted_rollout_items(&[item], ThreadHistoryMode::Legacy);
    let RolloutItem::EventMsg(EventMsg::UserMessage(user)) = &persisted[0] else {
        panic!("expected legacy user message");
    };
    assert_eq!(
        user.images,
        Some(vec!["https://example.com/image.png".to_string()])
    );
    assert_eq!(
        user.image_details,
        vec![Some(codex_protocol::models::ImageDetail::High)]
    );
    assert_eq!(
        user.local_images,
        vec![std::path::PathBuf::from("/tmp/image.png")]
    );
    assert_eq!(
        user.audio,
        Some(vec!["https://example.com/audio.wav".to_string()])
    );
    assert_eq!(
        user.local_audio,
        vec![std::path::PathBuf::from("/tmp/audio.wav")]
    );
}

#[test]
fn persisted_compaction_replaces_nested_inline_user_media() {
    let item = RolloutItem::Compacted(CompactedItem {
        message: "summary".to_string(),
        replacement_history: Some(vec![ResponseItemEnvelope::new(ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputImage {
                image_url: "data:image/png;base64,image".to_string(),
                detail: None,
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        })]),
        guardian_history: None,
        mcp_resource_origins: None,
        window_number: Some(1),
        first_window_id: None,
        previous_window_id: None,
        window_id: None,
        compaction_response_id: Some("response-1".to_string()),
        latest_token_usage_record: None,
    });

    let persisted = persisted_rollout_items(&[item], ThreadHistoryMode::Paginated);
    let RolloutItem::Compacted(compacted) = &persisted[0] else {
        panic!("expected compacted item");
    };
    assert_eq!(
        compacted.compaction_response_id.as_deref(),
        Some("response-1")
    );
    let ResponseItem::Message { content, .. } =
        &compacted.replacement_history.as_ref().unwrap()[0].item
    else {
        panic!("expected nested user message");
    };
    assert_eq!(
        content,
        &vec![ContentItem::InputText {
            text: "[inline user image omitted from persisted history]".to_string(),
        }]
    );
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
fn persisted_dynamic_tool_output_replaces_any_inline_data_media() {
    let item = RolloutItem::EventMsg(EventMsg::ItemCompleted(ItemCompletedEvent {
        thread_id: codex_protocol::ThreadId::default(),
        turn_id: "turn-1".to_string(),
        item: TurnItem::DynamicToolCall(DynamicToolCallItem {
            id: "dynamic-1".to_string(),
            namespace: None,
            tool: "tool".to_string(),
            arguments: serde_json::json!({}),
            status: DynamicToolCallStatus::Completed,
            content_items: Some(vec![DynamicToolCallOutputContentItem::InputImage {
                image_url: "DATA:APPLICATION/OCTET-STREAM;base64,sensitive".to_string(),
            }]),
            success: Some(true),
            error: None,
            duration: None,
        }),
        started_at_ms: Some(1),
        completed_at_ms: 2,
    }));

    let persisted = persisted_rollout_items(&[item], ThreadHistoryMode::Paginated);
    let serialized = serde_json::to_string(&persisted).expect("serialize persisted item");
    assert!(!serialized.contains("sensitive"));
    assert!(serialized.contains("inline tool image omitted"));
}

fn mcp_result_with_media() -> CallToolResult {
    CallToolResult {
        content: vec![
            serde_json::json!({"type": "image", "data": "sensitive-image"}),
            serde_json::json!({"type": "audio", "data": "sensitive-audio"}),
            serde_json::json!({"type": "resource", "resource": {"blob": "sensitive-blob"}}),
            serde_json::json!({"type": "text", "text": "kept"}),
        ],
        structured_content: Some(serde_json::json!({
            "nested": "DATA:APPLICATION/OCTET-STREAM;base64,sensitive-structured"
        })),
        is_error: Some(false),
        meta: None,
    }
}

#[test]
fn persisted_web_search_removes_media_and_bounds_opaque_results_in_both_history_modes() {
    let paginated = RolloutItem::EventMsg(EventMsg::ItemCompleted(ItemCompletedEvent {
        thread_id: codex_protocol::ThreadId::default(),
        turn_id: "turn-1".to_string(),
        item: TurnItem::Extension(ExtensionItem::WebSearch(WebSearchItem {
            id: "search-1".to_string(),
            query: "query".to_string(),
            action: None,
            results: Some(vec![serde_json::json!({
                "media": "DATA:APPLICATION/OCTET-STREAM;base64,sensitive-web",
                "large": "x".repeat(1024 * 1024),
            })]),
        })),
        started_at_ms: Some(1),
        completed_at_ms: 2,
    }));
    let legacy = RolloutItem::EventMsg(EventMsg::WebSearchEnd(WebSearchEndEvent {
        call_id: "search-1".to_string(),
        query: "query".to_string(),
        action: codex_protocol::models::WebSearchAction::Other,
        results: Some(vec![serde_json::json!({
            "resource": {"blob": "sensitive-web-blob"}
        })]),
    }));

    for (item, mode) in [
        (paginated, ThreadHistoryMode::Paginated),
        (legacy, ThreadHistoryMode::Legacy),
    ] {
        let persisted = persisted_rollout_items(&[item], mode);
        let serialized = serde_json::to_string(&persisted).expect("serialize persisted item");
        assert!(!serialized.contains("sensitive-web"));
        assert!(serialized.len() < 2048);
        assert!(serialized.contains("omitted from history"));
    }
}

#[test]
fn persisted_mcp_result_removes_media_from_paginated_item() {
    let item = RolloutItem::EventMsg(EventMsg::ItemCompleted(ItemCompletedEvent {
        thread_id: codex_protocol::ThreadId::default(),
        turn_id: "turn-1".to_string(),
        item: TurnItem::McpToolCall(McpToolCallItem {
            id: "mcp-1".to_string(),
            server: "server".to_string(),
            tool: "tool".to_string(),
            arguments: serde_json::json!({}),
            connector_id: None,
            mcp_app_resource_uri: None,
            link_id: None,
            app_name: None,
            action_name: None,
            plugin_id: None,
            read_only_hint: Some(true),
            status: McpToolCallStatus::Completed,
            result: Some(mcp_result_with_media()),
            error: None,
            duration: Some(std::time::Duration::from_millis(1)),
        }),
        started_at_ms: Some(1),
        completed_at_ms: 2,
    }));

    let persisted = persisted_rollout_items(&[item], ThreadHistoryMode::Paginated);
    let serialized = serde_json::to_string(&persisted).expect("serialize persisted item");
    assert!(!serialized.contains("sensitive-"));
    assert!(serialized.contains("kept"));
}

#[test]
fn persisted_mcp_result_removes_media_from_legacy_event() {
    let item = RolloutItem::EventMsg(EventMsg::McpToolCallEnd(McpToolCallEndEvent {
        call_id: "mcp-1".to_string(),
        invocation: McpInvocation {
            server: "server".to_string(),
            tool: "tool".to_string(),
            arguments: Some(serde_json::json!({})),
        },
        connector_id: None,
        mcp_app_resource_uri: None,
        link_id: None,
        app_name: None,
        action_name: None,
        plugin_id: None,
        read_only_hint: Some(true),
        duration: std::time::Duration::from_millis(1),
        result: Ok(mcp_result_with_media()),
    }));

    let persisted = persisted_rollout_items(&[item], ThreadHistoryMode::Legacy);
    let serialized = serde_json::to_string(&persisted).expect("serialize persisted item");
    assert!(!serialized.contains("sensitive-"));
    assert!(serialized.contains("kept"));
}

#[test]
fn persisted_compaction_drops_nested_inline_image_result() {
    let item = RolloutItem::Compacted(CompactedItem {
        message: "summary".to_string(),
        replacement_history: Some(vec![ResponseItemEnvelope::new(image_generation_response(
            "copied-base64-result",
        ))]),
        guardian_history: None,
        mcp_resource_origins: None,
        window_number: Some(1),
        first_window_id: None,
        previous_window_id: None,
        window_id: None,
        compaction_response_id: None,
        latest_token_usage_record: None,
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
