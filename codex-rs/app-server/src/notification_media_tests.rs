use super::*;
use codex_app_server_protocol::DynamicToolCallStatus;
use codex_app_server_protocol::ItemCompletedNotification;
use codex_app_server_protocol::McpToolCallResult;
use codex_app_server_protocol::McpToolCallStatus;
use codex_app_server_protocol::RawResponseItemCompletedNotification;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ImageDetail;
use pretty_assertions::assert_eq;
use serde_json::json;

fn raw_notification(item: ResponseItem) -> ServerNotification {
    ServerNotification::RawResponseItemCompleted(RawResponseItemCompletedNotification {
        thread_id: "thread".to_string(),
        turn_id: "turn".to_string(),
        item,
    })
}

fn completed_notification(item: ThreadItem) -> ServerNotification {
    ServerNotification::ItemCompleted(ItemCompletedNotification {
        item,
        thread_id: "thread".to_string(),
        turn_id: "turn".to_string(),
        completed_at_ms: 1,
    })
}

fn assert_notification_eq(actual: ServerNotification, expected: ServerNotification) {
    assert_eq!(
        serde_json::to_value(actual).expect("serialize actual notification"),
        serde_json::to_value(expected).expect("serialize expected notification"),
    );
}

fn tool_output_items() -> Vec<FunctionCallOutputContentItem> {
    vec![
        FunctionCallOutputContentItem::InputImage {
            image_url: "data:image/png;base64,image".to_string(),
            detail: Some(ImageDetail::High),
        },
        FunctionCallOutputContentItem::InputAudio {
            audio_url: "data:audio/wav;base64,audio".to_string(),
        },
        FunctionCallOutputContentItem::InputText {
            text: "keep text".to_string(),
        },
        FunctionCallOutputContentItem::EncryptedContent {
            encrypted_content: "keep encrypted".to_string(),
        },
    ]
}

fn filtered_tool_output_items() -> Vec<FunctionCallOutputContentItem> {
    vec![
        FunctionCallOutputContentItem::InputText {
            text: "keep text".to_string(),
        },
        FunctionCallOutputContentItem::EncryptedContent {
            encrypted_content: "keep encrypted".to_string(),
        },
    ]
}

#[test]
fn raw_function_and_custom_tool_notifications_drop_media_only() {
    let function = raw_notification(ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("function".to_string()),
        name: None,
        namespace: None,
        output: FunctionCallOutputPayload::from_content_items(tool_output_items()),
        internal_chat_message_metadata_passthrough: None,
    });
    let expected_function = raw_notification(ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some("function".to_string()),
        name: None,
        namespace: None,
        output: FunctionCallOutputPayload::from_content_items(filtered_tool_output_items()),
        internal_chat_message_metadata_passthrough: None,
    });
    assert_notification_eq(without_notification_media(function), expected_function);

    let custom = raw_notification(ResponseItem::CustomToolCallOutput {
        id: None,
        call_id: "custom".to_string(),
        name: Some("view_image".to_string()),
        output: FunctionCallOutputPayload::from_content_items(tool_output_items()),
        internal_chat_message_metadata_passthrough: None,
    });
    let expected_custom = raw_notification(ResponseItem::CustomToolCallOutput {
        id: None,
        call_id: "custom".to_string(),
        name: Some("view_image".to_string()),
        output: FunctionCallOutputPayload::from_content_items(filtered_tool_output_items()),
        internal_chat_message_metadata_passthrough: None,
    });
    assert_notification_eq(without_notification_media(custom), expected_custom);
}

#[test]
fn raw_image_generation_notification_clears_only_the_result() {
    let notification = raw_notification(ResponseItem::ImageGenerationCall {
        id: None,
        status: "completed".to_string(),
        revised_prompt: Some("keep prompt".to_string()),
        result: "large base64 result".to_string(),
        internal_chat_message_metadata_passthrough: None,
    });
    let expected = raw_notification(ResponseItem::ImageGenerationCall {
        id: None,
        status: "completed".to_string(),
        revised_prompt: Some("keep prompt".to_string()),
        result: String::new(),
        internal_chat_message_metadata_passthrough: None,
    });
    assert_notification_eq(without_notification_media(notification), expected);
}

#[test]
fn function_call_output_notification_drops_media_only() {
    let function = completed_notification(ThreadItem::FunctionCallOutput {
        id: "function".to_string(),
        name: "view_image".to_string(),
        namespace: Some("functions".to_string()),
        output: FunctionCallOutputBody::ContentItems(tool_output_items()),
    });
    let expected = completed_notification(ThreadItem::FunctionCallOutput {
        id: "function".to_string(),
        name: "view_image".to_string(),
        namespace: Some("functions".to_string()),
        output: FunctionCallOutputBody::ContentItems(filtered_tool_output_items()),
    });

    assert_notification_eq(without_notification_media(function), expected);
}

#[test]
fn user_and_dynamic_tool_notifications_drop_inline_media_only() {
    let user = completed_notification(ThreadItem::UserMessage {
        id: "user".to_string(),
        client_id: None,
        content: vec![
            UserInput::Image {
                url: "data:image/png;base64,image".to_string(),
                detail: None,
            },
            UserInput::Audio {
                url: "data:audio/wav;base64,audio".to_string(),
            },
            UserInput::Text {
                text: "keep text".to_string(),
                text_elements: Vec::new(),
            },
        ],
    });
    let expected_user = completed_notification(ThreadItem::UserMessage {
        id: "user".to_string(),
        client_id: None,
        content: vec![UserInput::Text {
            text: "keep text".to_string(),
            text_elements: Vec::new(),
        }],
    });
    assert_notification_eq(without_notification_media(user), expected_user);

    let dynamic = completed_notification(ThreadItem::DynamicToolCall {
        id: "dynamic".to_string(),
        namespace: None,
        tool: "tool".to_string(),
        arguments: json!({}),
        status: DynamicToolCallStatus::Completed,
        content_items: Some(vec![
            DynamicToolCallOutputContentItem::InputImage {
                image_url: "data:image/png;base64,image".to_string(),
            },
            DynamicToolCallOutputContentItem::InputAudio {
                audio_url: "data:audio/wav;base64,audio".to_string(),
            },
            DynamicToolCallOutputContentItem::InputText {
                text: "keep text".to_string(),
            },
        ]),
        success: Some(true),
        duration_ms: Some(1),
    });
    let expected_dynamic = completed_notification(ThreadItem::DynamicToolCall {
        id: "dynamic".to_string(),
        namespace: None,
        tool: "tool".to_string(),
        arguments: json!({}),
        status: DynamicToolCallStatus::Completed,
        content_items: Some(vec![DynamicToolCallOutputContentItem::InputText {
            text: "keep text".to_string(),
        }]),
        success: Some(true),
        duration_ms: Some(1),
    });
    assert_notification_eq(without_notification_media(dynamic), expected_dynamic);
}

#[test]
fn mcp_notification_drops_media_and_blob_resources_only() {
    let item = |content| {
        completed_notification(ThreadItem::McpToolCall {
            id: "mcp".to_string(),
            server: "server".to_string(),
            tool: "tool".to_string(),
            status: McpToolCallStatus::Completed,
            arguments: json!({}),
            app_context: None,
            mcp_app_resource_uri: None,
            plugin_id: None,
            read_only_hint: None,
            result: Some(Box::new(McpToolCallResult {
                content,
                structured_content: Some(json!({"keep": true})),
                meta: Some(json!({"keep": true})),
            })),
            error: None,
            duration_ms: Some(1),
        })
    };
    let notification = item(vec![
        json!({"type": "image", "data": "image"}),
        json!({"type": "audio", "data": "audio"}),
        json!({"type": "resource", "resource": {"blob": "bytes"}}),
        json!({"type": "text", "text": "keep text"}),
    ]);
    let expected = item(vec![json!({"type": "text", "text": "keep text"})]);
    assert_notification_eq(without_notification_media(notification), expected);
}
