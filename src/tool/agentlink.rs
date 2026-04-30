use std::sync::Arc;

use agentlink_protocol::message::SendMessageRequest;
use agentlink_protocol::task::TaskSearchQuery;
use agentlink_protocol::MessageType;
use agentlink_rust_sdk::AgentLinkClient;
use anyhow::Result;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use serde_json::json;
use tracing::{debug, info};

#[derive(Debug, thiserror::Error)]
pub enum AgentLinkToolError {
    #[error("SDK error: {0}")]
    Sdk(#[from] agentlink_rust_sdk::SdkError),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Invalid parameter: {0}")]
    InvalidParam(String),
}

// ===================================================================
// Send Message Tool
// ===================================================================

#[derive(Clone)]
pub struct SendMessageTool {
    client: Arc<AgentLinkClient>,
}

impl SendMessageTool {
    pub fn new(client: Arc<AgentLinkClient>) -> Self {
        Self { client }
    }
}

#[derive(Deserialize)]
pub struct SendMessageArgs {
    conversation_id: String,
    content: String,
}

impl Tool for SendMessageTool {
    const NAME: &'static str = "send_message";
    type Error = AgentLinkToolError;
    type Args = SendMessageArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Send a text message to a conversation on the AgentLink platform.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "conversation_id": { "type": "string", "description": "UUID of the conversation" },
                    "content": { "type": "string", "description": "Message content" }
                },
                "required": ["conversation_id", "content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!(tool = Self::NAME, conversation_id = %args.conversation_id, "Executing tool");
        let req = SendMessageRequest {
            content: args.content.clone(),
            kind: Some(MessageType::Text),
            metadata: None,
            reply_to: None,
        };
        let resp = self.client.messages.send_message(&args.conversation_id, req).await?;
        info!(tool = Self::NAME, message_id = %resp.id, "Message sent successfully");
        Ok(format!("Message sent successfully. ID: {}", resp.id))
    }
}

// ===================================================================
// Get Task Tool
// ===================================================================

#[derive(Clone)]
pub struct GetTaskTool {
    client: Arc<AgentLinkClient>,
}

impl GetTaskTool {
    pub fn new(client: Arc<AgentLinkClient>) -> Self {
        Self { client }
    }
}

#[derive(Deserialize)]
pub struct GetTaskArgs {
    task_id: String,
}

impl Tool for GetTaskTool {
    const NAME: &'static str = "get_task";
    type Error = AgentLinkToolError;
    type Args = GetTaskArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Get details of a specific task by its ID.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "Task ID or UUID" }
                },
                "required": ["task_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!(tool = Self::NAME, task_id = %args.task_id, "Executing tool");
        let task = self.client.tasks.get_task_by_id(&args.task_id).await?;
        debug!(tool = Self::NAME, task_id = %args.task_id, "Task fetched");
        Ok(serde_json::to_string_pretty(&task)?)
    }
}

// ===================================================================
// List My Tasks Tool
// ===================================================================

#[derive(Clone)]
pub struct ListMyTasksTool {
    client: Arc<AgentLinkClient>,
}

impl ListMyTasksTool {
    pub fn new(client: Arc<AgentLinkClient>) -> Self {
        Self { client }
    }
}

#[derive(Deserialize)]
pub struct ListMyTasksArgs {}

impl Tool for ListMyTasksTool {
    const NAME: &'static str = "list_my_tasks";
    type Error = AgentLinkToolError;
    type Args = ListMyTasksArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "List all tasks associated with the current agent/user.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!(tool = Self::NAME, "Executing tool");
        let tasks = self.client.tasks.get_my_tasks().await?;
        debug!(tool = Self::NAME, count = tasks.tasks.len(), "Tasks listed");
        Ok(serde_json::to_string_pretty(&tasks)?)
    }
}

// ===================================================================
// Search Tasks Tool
// ===================================================================

#[derive(Clone)]
pub struct SearchTasksTool {
    client: Arc<AgentLinkClient>,
}

impl SearchTasksTool {
    pub fn new(client: Arc<AgentLinkClient>) -> Self {
        Self { client }
    }
}

#[derive(Deserialize)]
pub struct SearchTasksArgs {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    status: Option<agentlink_protocol::TaskStatus>,
}

impl Tool for SearchTasksTool {
    const NAME: &'static str = "search_tasks";
    type Error = AgentLinkToolError;
    type Args = SearchTasksArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Search tasks on the platform with optional query and status filter.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search keyword" },
                    "status": { "type": "string", "description": "Task status filter (e.g. open, in_progress, completed)" }
                },
                "required": []
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!(tool = Self::NAME, query = ?args.query, status = ?args.status, "Executing tool");
        let query = TaskSearchQuery {
            q: args.query,
            status: args.status,
            page: None,
            per_page: Some(20),
            task_type: None,
            budget_min: None,
            budget_max: None,
        };
        let resp = self.client.tasks.list_tasks(query).await?;
        debug!(tool = Self::NAME, result_count = resp.data.len(), "Tasks searched");
        Ok(serde_json::to_string_pretty(&resp)?)
    }
}

// ===================================================================
// Get User Profile Tool
// ===================================================================

#[derive(Clone)]
pub struct GetUserProfileTool {
    client: Arc<AgentLinkClient>,
}

impl GetUserProfileTool {
    pub fn new(client: Arc<AgentLinkClient>) -> Self {
        Self { client }
    }
}

#[derive(Deserialize)]
pub struct GetUserProfileArgs {
    user_id: String,
}

impl Tool for GetUserProfileTool {
    const NAME: &'static str = "get_user_profile";
    type Error = AgentLinkToolError;
    type Args = GetUserProfileArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Get the profile of a user or agent by ID or linkid.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "user_id": { "type": "string", "description": "User ID or linkid" }
                },
                "required": ["user_id"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!(tool = Self::NAME, user_id = %args.user_id, "Executing tool");
        let user = self.client.users.get_user(&args.user_id).await?;
        debug!(tool = Self::NAME, user_id = %args.user_id, "User profile fetched");
        Ok(serde_json::to_string_pretty(&user)?)
    }
}
