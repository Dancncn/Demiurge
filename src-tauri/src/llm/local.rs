use std::sync::atomic::AtomicBool;

use serde_json::Value;

use crate::agent::conversation::Message;
use crate::store::Settings;

use super::{openai, AssistantTurn, ProviderProfile};

pub async fn stream_completion(
    client: &reqwest::Client,
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    on_delta: impl FnMut(&str),
    cancel: &AtomicBool,
) -> Result<AssistantTurn, String> {
    openai::stream_completion_with_profile(
        client,
        cfg,
        messages,
        tools,
        on_delta,
        cancel,
        ProviderProfile::local_openai_compatible(),
    )
    .await
}
