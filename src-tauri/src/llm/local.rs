use std::sync::atomic::AtomicBool;

use serde_json::Value;

use crate::agent::conversation::Message;
use crate::store::Settings;

use super::{openai, AssistantTurn, ProviderProfile, StreamDelta};

#[allow(dead_code)]
pub async fn stream_completion(
    client: &reqwest::Client,
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    on_delta: impl FnMut(StreamDelta<'_>),
    cancel: &AtomicBool,
) -> Result<AssistantTurn, String> {
    stream_completion_with_profile(
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

pub async fn stream_completion_with_profile(
    client: &reqwest::Client,
    cfg: &Settings,
    messages: &[Message],
    tools: &Value,
    on_delta: impl FnMut(StreamDelta<'_>),
    cancel: &AtomicBool,
    profile: ProviderProfile,
) -> Result<AssistantTurn, String> {
    openai::stream_completion_with_profile(client, cfg, messages, tools, on_delta, cancel, profile)
        .await
}
