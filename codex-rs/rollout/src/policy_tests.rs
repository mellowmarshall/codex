use codex_extension_items::ExtensionItem;
use codex_extension_items::image_generation::ImageGenerationItem;
use codex_protocol::items::TurnItem;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::ItemCompletedEvent;
use codex_protocol::protocol::ThreadHistoryMode;
use codex_utils_absolute_path::test_support::PathBufExt;
use codex_utils_absolute_path::test_support::test_path_buf;
use pretty_assertions::assert_eq;

use super::persisted_rollout_items;
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
}

#[test]
fn unsaved_image_generation_keeps_inline_result() {
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
    assert_eq!(image.result, "only-durable-copy");
    assert_eq!(image.saved_path, None);
}
