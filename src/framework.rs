use std::sync::Arc;

use agentlink_protocol::message::{MessageResponse, SendMessageRequest};
use agentlink_protocol::MessageType;
use agentlink_rust_sdk::{AgentLinkClient, SdkConfig};
use anyhow::{Context, Result};
use tracing::info;

use crate::agent::AgentRunner;
use crate::config::FrameworkConfig;
use crate::skill::{Skill, SkillLoader};
use crate::tool::build_tools;

#[derive(Clone)]
pub struct AgentFramework {
    pub sdk_client: AgentLinkClient,
    pub agent: Arc<AgentRunner>,
    pub skill: Skill,
    pub my_user_id: uuid::Uuid,
}

impl AgentFramework {
    pub async fn new(config: &FrameworkConfig) -> Result<Self> {
        // 1. Connect to AgentLink
        let sdk_client = AgentLinkClient::new(
            SdkConfig::new(&config.agentlink_base_url).with_token(&config.agentlink_api_key),
        )
        .context("Failed to create AgentLink client")?;

        let me = sdk_client
            .users
            .get_current_user()
            .await
            .context("Failed to get current user")?;
        let my_user_id = me.id;
        info!(
            user_id = %my_user_id,
            linkid = %me.linkid,
            display_name = %me.display_name.unwrap_or_default(),
            "Agent authenticated"
        );

        // 2. Load skill based on source
        let skill_loader = if config.skill_source == "platform" {
            SkillLoader::platform(Arc::new(sdk_client.clone()))
        } else {
            SkillLoader::local(".agents/skills")
        };

        let skill = skill_loader
            .load(&config.skill_name)
            .await?;

        info!(
            skill_name = %skill.meta.name,
            skill_version = %skill.meta.version,
            skill_source = %config.skill_source,
            "Skill loaded"
        );

        // 3. Build tools
        let sdk_client_arc = Arc::new(sdk_client.clone());
        let tools = build_tools(sdk_client_arc);

        // 4. Build LLM agent
        let agent = AgentRunner::build(config, &skill, tools)?;

        Ok(Self {
            sdk_client,
            agent: Arc::new(agent),
            skill,
            my_user_id,
        })
    }

    pub async fn handle_message(&self, msg: MessageResponse) -> Result<String> {
        // Ignore our own messages
        if msg.sender_id == self.my_user_id {
            return Ok(String::new());
        }

        let input = match msg.kind {
            MessageType::Text => msg.content,
            MessageType::File => {
                format!(
                    "[User sent a file] Filename: {}. Please acknowledge.",
                    msg.metadata
                        .as_ref()
                        .and_then(|m| m.get("filename").and_then(|v| v.as_str()))
                        .unwrap_or("unknown")
                )
            }
            MessageType::Image => {
                format!(
                    "[User sent an image] Filename: {}. Please acknowledge.",
                    msg.metadata
                        .as_ref()
                        .and_then(|m| m.get("filename").and_then(|v| v.as_str()))
                        .unwrap_or("unknown")
                )
            }
            _ => {
                return Ok(String::new());
            }
        };

        info!(
            conversation_id = %msg.conversation_id,
            sender_id = %msg.sender_id,
            sender_name = %msg.sender_name,
            content = %input,
            "Processing message"
        );

        let response = self.agent.prompt(&input).await?;

        info!(response = %response, "Agent responded");

        Ok(response)
    }

    pub async fn send_reply(&self, conversation_id: &str, content: String, reply_to: Option<uuid::Uuid>) -> Result<()> {
        let req = SendMessageRequest {
            content,
            kind: Some(MessageType::Text),
            metadata: None,
            reply_to,
        };
        self.sdk_client
            .messages
            .send_message(conversation_id, req)
            .await
            .context("Failed to send reply")?;
        Ok(())
    }

    pub async fn set_availability(&self, available: bool) -> Result<()> {
        self.sdk_client
            .agents
            .update_agent_availability(&self.my_user_id.to_string(), available)
            .await
            .context("Failed to update availability")?;
        Ok(())
    }
}
