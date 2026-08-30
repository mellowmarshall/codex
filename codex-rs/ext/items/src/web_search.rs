use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::io;
use std::io::Write;
use ts_rs::TS;

const MAX_PERSISTED_RESULTS_BYTES: usize = 1024 * 1024;

// Standalone web-search item owned by the web extension. This is also the
// field-level representation exposed by app-server; core and rollout
// persistence only carry it inside an ExtensionItem envelope.
#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct WebSearchItem {
    pub id: String,
    pub query: String,
    pub action: Option<WebSearchAction>,
    /// Structured search results returned out-of-band by standalone web search.
    ///
    /// These stay as opaque JSON at the extension/app-server boundary so new
    /// result fields and result types can pass through without a Codex release.
    #[serde(default)]
    pub results: Option<Vec<JsonValue>>,
}

impl WebSearchItem {
    /// Return the UI/history copy without inline media bytes in opaque search results.
    pub fn without_inline_media(&self) -> Self {
        Self {
            id: self.id.clone(),
            query: self.query.clone(),
            action: self.action.clone(),
            results: self.results.as_deref().map(results_without_inline_media),
        }
    }
}

pub fn results_without_inline_media(results: &[JsonValue]) -> Vec<JsonValue> {
    let mut counter = SizeLimitWriter::new(MAX_PERSISTED_RESULTS_BYTES);
    if serde_json::to_writer(&mut counter, results).is_err() {
        return vec![serde_json::json!({
            "type": "text",
            "text": "[oversized web search results omitted from history]",
        })];
    }
    results
        .iter()
        .map(clone_value_without_inline_media)
        .collect()
}

struct SizeLimitWriter {
    remaining: usize,
}

impl SizeLimitWriter {
    fn new(limit: usize) -> Self {
        Self { remaining: limit }
    }
}

impl Write for SizeLimitWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if buffer.len() > self.remaining {
            return Err(io::Error::other("web search results exceed history limit"));
        }
        self.remaining -= buffer.len();
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn clone_value_without_inline_media(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::String(value) if has_data_scheme(value) => {
            JsonValue::String("[inline web search media omitted from history]".to_string())
        }
        JsonValue::Array(values) => JsonValue::Array(
            values
                .iter()
                .map(clone_value_without_inline_media)
                .collect(),
        ),
        JsonValue::Object(object)
            if matches!(
                object.get("type").and_then(JsonValue::as_str),
                Some("image" | "audio")
            ) || object
                .get("resource")
                .and_then(|resource| resource.get("blob"))
                .is_some() =>
        {
            serde_json::json!({
                "type": "text",
                "text": "[inline web search media omitted from history]",
            })
        }
        JsonValue::Object(object) => JsonValue::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), clone_value_without_inline_media(value)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn has_data_scheme(value: &str) -> bool {
    value
        .get(.."data:".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
}

// App-server-facing description of the action performed by standalone web search.
#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(tag = "type", rename_all = "camelCase")]
// Keep app-server's existing v2 TS path. The root WebSearchAction name is
// already used by the snake_case Responses API action type.
#[ts(export_to = "v2/")]
pub enum WebSearchAction {
    Search {
        query: Option<String>,
        queries: Option<Vec<String>>,
    },
    OpenPage {
        url: Option<String>,
    },
    FindInPage {
        url: Option<String>,
        pattern: Option<String>,
    },
    #[serde(other)]
    Other,
}
