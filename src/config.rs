use std::env;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct FrameworkConfig {
    /// AgentLink platform base URL
    pub agentlink_base_url: String,
    /// AgentLink API key for authentication
    pub agentlink_api_key: String,

    /// LLM provider: openai | deepseek | anthropic | openrouter
    pub llm_provider: String,
    /// LLM API key
    pub llm_api_key: String,
    /// LLM model name
    pub llm_model: String,

    /// Skill source: "local" | "platform"
    pub skill_source: String,
    /// Name of the skill to load (subdirectory in .agents/skills/ for local, or skill id/namespace for platform)
    pub skill_name: String,

    /// Maximum turns for multi-turn tool calling
    pub max_turns: usize,

    /// Maximum messages to keep per conversation in short-term memory (0 = disabled)
    pub max_history: usize,

    /// Number of messages to batch-summarize into long-term memory when STM overflows
    pub ltm_batch_size: usize,
}

impl FrameworkConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            agentlink_base_url: env::var("AGENTLINK_BASE_URL")
                .unwrap_or_else(|_| "https://beta-api.agentlink.chat/".to_string()),
            agentlink_api_key: env::var("AGENTLINK_API_KEY")
                .context("AGENTLINK_API_KEY environment variable is required")?,

            llm_provider: env::var("LLM_PROVIDER").unwrap_or_else(|_| "deepseek".to_string()),
            llm_api_key: env::var("LLM_API_KEY")
                .context("LLM_API_KEY environment variable is required")?,
            llm_model: env::var("LLM_MODEL").unwrap_or_else(|_| "deepseek-chat".to_string()),

            skill_source: env::var("SKILL_SOURCE").unwrap_or_else(|_| "local".to_string()),
            skill_name: env::var("SKILL_NAME").unwrap_or_else(|_| "audit".to_string()),

            max_turns: env::var("MAX_TURNS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),

            max_history: env::var("MAX_HISTORY")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),

            ltm_batch_size: env::var("LTM_BATCH_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
        })
    }
}
