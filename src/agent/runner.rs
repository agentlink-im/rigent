use anyhow::Result;
use rig::agent::{Agent, PromptRequest, PromptResponse};
use rig::client::CompletionClient;
use rig::completion::{Chat, Message, Prompt};
use rig::providers::{anthropic, deepseek, openai};
use rig::tool::ToolDyn;
use tracing::{debug, info};

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
                let client = openai::CompletionsClient::builder()
                    .api_key(&config.llm_api_key)
                    .build()?;
                let agent = client
                    .agent(&config.llm_model)
                    .preamble(&preamble)
                    .tools(tools)
                    .default_max_turns(config.max_turns)
                    .build();
                InnerAgent::OpenAI(agent)
            }
            "anthropic" => {
                let client = anthropic::Client::builder()
                    .api_key(&config.llm_api_key)
                    .build()?;
                let agent = client
                    .agent(&config.llm_model)
                    .preamble(&preamble)
                    .tools(tools)
                    .default_max_turns(config.max_turns)
                    .build();
                InnerAgent::Anthropic(agent)
            }
            "deepseek" => {
                let client = deepseek::Client::builder()
                    .api_key(&config.llm_api_key)
                    .build()?;
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
                let client = openai::CompletionsClient::builder()
                    .base_url(&base_url)
                    .api_key(&config.llm_api_key)
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
        info!(input_len = input.len(), "LLM prompt start");
        let result = match &self.inner {
            InnerAgent::OpenAI(agent) => agent.prompt(input).await,
            InnerAgent::Anthropic(agent) => agent.prompt(input).await,
            InnerAgent::DeepSeek(agent) => agent.prompt(input).await,
        };
        let output = result?;
        info!(output_len = output.len(), "LLM prompt complete");
        Ok(output)
    }

    /// Run the agent with conversation history.
    /// The `history` contains previous messages; `prompt` is the current user input.
    pub async fn chat(&self, history: Vec<Message>, prompt: &str) -> Result<String> {
        info!(history_len = history.len(), prompt_len = prompt.len(), "LLM chat start");
        let result = match &self.inner {
            InnerAgent::OpenAI(agent) => agent.chat(Message::user(prompt), history).await,
            InnerAgent::Anthropic(agent) => agent.chat(Message::user(prompt), history).await,
            InnerAgent::DeepSeek(agent) => agent.chat(Message::user(prompt), history).await,
        };
        let output = result?;
        info!(output_len = output.len(), "LLM chat complete");
        Ok(output)
    }

    /// Run the agent with conversation history and return the full message exchange.
    ///
    /// Returns `(final_output, new_messages)` where `new_messages` contains the complete
    /// turn including any tool calls and tool results that occurred during multi-turn execution.
    pub async fn chat_with_details(
        &self,
        history: Vec<Message>,
        prompt: &str,
    ) -> Result<(String, Vec<Message>)> {
        info!(history_len = history.len(), prompt_len = prompt.len(), "LLM chat_with_details start");
        let prompt_msg = Message::user(prompt);
        let result: Result<(String, Vec<Message>)> = match &self.inner {
            InnerAgent::OpenAI(agent) => {
                debug!(provider = "openai", "Sending request to LLM");
                let response: PromptResponse = PromptRequest::from_agent(agent, prompt_msg.clone())
                    .with_history(history)
                    .extended_details()
                    .await?;
                Ok((response.output, response.messages.unwrap_or_default()))
            }
            InnerAgent::Anthropic(agent) => {
                debug!(provider = "anthropic", "Sending request to LLM");
                let response: PromptResponse = PromptRequest::from_agent(agent, prompt_msg.clone())
                    .with_history(history)
                    .extended_details()
                    .await?;
                Ok((response.output, response.messages.unwrap_or_default()))
            }
            InnerAgent::DeepSeek(agent) => {
                debug!(provider = "deepseek", "Sending request to LLM");
                let response: PromptResponse = PromptRequest::from_agent(agent, prompt_msg.clone())
                    .with_history(history)
                    .extended_details()
                    .await?;
                Ok((response.output, response.messages.unwrap_or_default()))
            }
        };
        let (output, messages) = result?;
        info!(
            output_len = output.len(),
            new_messages = messages.len(),
            "LLM chat_with_details complete"
        );
        Ok((output, messages))
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
