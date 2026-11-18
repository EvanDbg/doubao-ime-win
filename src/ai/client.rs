use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::data::AiConfig;

pub struct AiPostProcessor;

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: [ChatMessage<'a>; 2],
    temperature: f32,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: String,
}

impl AiPostProcessor {
    pub async fn process(text: &str, config: &AiConfig) -> Result<String> {
        if text.trim().is_empty() {
            return Ok(String::new());
        }
        if config.endpoint.trim().is_empty() || config.api_key.trim().is_empty() {
            return Err(anyhow!("AI endpoint or API key is empty"));
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms.clamp(1000, 30000)))
            .danger_accept_invalid_certs(config.accept_invalid_certs)
            .build()?;

        let request = ChatRequest {
            model: &config.model,
            messages: [
                ChatMessage {
                    role: "system",
                    content: "你是语音识别后处理器。只改写用户提供的语音识别文本：修正错别字、同音字和明显断句，保持原意与语言，不要解释，不要添加任何前后缀。",
                },
                ChatMessage {
                    role: "user",
                    content: text,
                },
            ],
            temperature: 0.1,
        };

        let response = client
            .post(&config.endpoint)
            .header("Authorization", format!("Bearer {}", config.api_key))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("AI request failed: {}", response.status()));
        }

        let result: ChatResponse = response.json().await?;
        let content = result
            .choices
            .first()
            .map(|choice| choice.message.content.trim().to_string())
            .unwrap_or_default();

        if content.is_empty() {
            return Err(anyhow!("AI returned empty text"));
        }
        Ok(content)
    }
}
