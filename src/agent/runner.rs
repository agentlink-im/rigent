use anyhow::Result;
use rig::agent::Agent;
use rig::client::{CompletionClient, ProviderClient};
use rig::completion::Prompt;
use rig::providers::{anthropic, deepseek, openai};
use rig::tool::ToolDyn;

use crate::config::FrameworkConfig;
use crate::skill::Skill;

enum InnerAgent {
    OpenAI(Agent<openai::completion::CompletionModel>),
    Anthropic(Agent<anthropic::completion::CompletionModel>),
    DeepSeek(Agent<deepseek::CompletionModel>),
}

/// A wrapper around Rig's Agent that abstracts over different LLM providers.
pub struct AgentRunner {
    inner: InnerAgent,
}

impl AgentRunner {
    pub fn build(config: &FrameworkConfig, skill: &Skill, tools: Vec<Box<dyn ToolDyn>>) -> Result<Self> {
        let preamble = build_preamble(skill);
        let inner = match config.llm_provider.as_str() {
            "openai" => {
                let client = openai::CompletionsClient::from_env()?;
                let agent = client
                    .agent(&config.llm_model)
                    .preamble(&preamble)
                    .tools(tools)
                    .default_max_turns(config.max_turns)
                    .build();
                InnerAgent::OpenAI(agent)
            }
            "anthropic" => {
                let client = anthropic::Client::from_env()?;
                let agent = client
                    .agent(&config.llm_model)
                    .preamble(&preamble)
                    .tools(tools)
                    .default_max_turns(config.max_turns)
                    .build();
                InnerAgent::Anthropic(agent)
            }
            "deepseek" => {
                let client = deepseek::Client::from_env()?;
                let agent = client
                    .agent(&config.llm_model)
                    .preamble(&preamble)
                    .tools(tools)
                    .default_max_turns(config.max_turns)
                    .build();
                InnerAgent::DeepSeek(agent)
            }
            other => {
                // Fallback: try OpenAI-compatible provider with custom base URL
                let base_url = std::env::var(format!("{}_BASE_URL", other.to_uppercase()))
                    .unwrap_or_else(|_| format!("https://api.{}.com/v1", other));
                let api_key = &config.llm_api_key;
                let client = openai::CompletionsClient::builder()
                    .base_url(&base_url)
                    .api_key(api_key)
                    .build()?;
                let agent = client
                    .agent(&config.llm_model)
                    .preamble(&preamble)
                    .tools(tools)
                    .default_max_turns(config.max_turns)
                    .build();
                InnerAgent::OpenAI(agent)
            }
        };

        Ok(Self { inner })
    }

    /// Run the agent with the given user input.
    /// Rig internally handles the iterative function calling loop.
    pub async fn prompt(&self, input: &str) -> Result<String> {
        let result = match &self.inner {
            InnerAgent::OpenAI(agent) => agent.prompt(input).await,
            InnerAgent::Anthropic(agent) => agent.prompt(input).await,
            InnerAgent::DeepSeek(agent) => agent.prompt(input).await,
        };
        Ok(result?)
    }
}

fn build_preamble(skill: &Skill) -> String {
    format!(
        "You are an AI agent on the AgentLink platform.\n\
         You have access to tools that let you interact with the platform and the local environment.\n\
         Use the tools whenever necessary to fulfill the user's request.\n\
         Think step by step, and if you need to perform multiple actions, use tools iteratively.\n\n\
         {}\n",
        skill.system_prompt_extension()
    )
}
