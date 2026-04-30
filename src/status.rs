use std::sync::Arc;

use agentlink_protocol::message::{AgentStatusMetadata, AgentStatusType, SendMessageRequest};
use agentlink_protocol::MessageType;
use agentlink_rust_sdk::AgentLinkClient;
use anyhow::Result;
use serde_json::json;
use tracing::{debug, error};

// Task-local status reporter for the current message handling context.
//
// Rig's tool calling happens within the same async task as `AgentFramework::chat`,
// so task-local storage correctly propagates to all tool invocations without
// cross-task contention.
tokio::task_local! {
    pub static STATUS_REPORTER: Arc<StatusReporter>;
}

/// Reports agent intermediate execution status to the AgentLink platform
/// by sending `agent_status` messages into the current conversation.
#[derive(Clone)]
pub struct StatusReporter {
    client: AgentLinkClient,
    conversation_id: String,
}

impl StatusReporter {
    pub fn new(client: AgentLinkClient, conversation_id: String) -> Self {
        Self {
            client,
            conversation_id,
        }
    }

    /// Run an async block with this reporter installed as the task-local reporter.
    pub async fn scope<F, Fut, R>(&self, f: F) -> R
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = R>,
    {
        STATUS_REPORTER
            .scope(Arc::new(self.clone()), f())
            .await
    }

    /// Agent is analysing the user request.
    pub async fn thinking(&self, detail: &str) {
        let _ = self
            .send(AgentStatusType::Thinking, "分析需求", detail, None)
            .await;
    }

    /// Agent is calling the LLM (inference in progress).
    pub async fn processing(&self, detail: &str) {
        let _ = self
            .send(AgentStatusType::Processing, "推理中", detail, None)
            .await;
    }

    /// Agent is invoking a tool.
    pub async fn tool_call(&self, tool_name: &str, detail: &str) {
        let _ = self
            .send(AgentStatusType::ToolCall, tool_name, detail, None)
            .await;
    }

    /// An intermediate step completed successfully.
    pub async fn complete(&self, step_name: &str, detail: &str) {
        let _ = self
            .send(AgentStatusType::Complete, step_name, detail, Some(1.0))
            .await;
    }

    /// Something went wrong and the agent is retrying / recovering.
    pub async fn error_retry(&self, detail: &str) {
        let _ = self
            .send(AgentStatusType::ErrorRetry, "处理出错", detail, None)
            .await;
    }

    async fn send(
        &self,
        status_type: AgentStatusType,
        step_name: &str,
        detail: &str,
        progress: Option<f32>,
    ) -> Result<()> {
        let metadata = AgentStatusMetadata {
            status_type,
            step_name: step_name.to_string(),
            detail: Some(detail.to_string()),
            progress,
            total_steps: None,
            current_step: None,
            tool_name: None,
            tool_input: None,
            started_at: None,
            estimated_duration_ms: None,
        };

        let req = SendMessageRequest {
            content: detail.to_string(),
            kind: Some(MessageType::AgentStatus),
            metadata: Some(json!(metadata)),
            reply_to: None,
        };

        debug!(
            conversation_id = %self.conversation_id,
            step_name = %step_name,
            status_type = ?metadata.status_type,
            "Sending agent status"
        );

        match self
            .client
            .messages
            .send_message(&self.conversation_id, req)
            .await
        {
            Ok(_) => debug!("Agent status sent successfully"),
            Err(e) => error!(error = %e, "Failed to send agent status"),
        }

        Ok(())
    }
}

/// Helper: report a tool-call start status if a task-local reporter exists.
pub async fn report_tool_call(tool_name: &str, detail: &str) {
    if let Ok(reporter) = STATUS_REPORTER.try_with(|r| r.clone()) {
        reporter.tool_call(tool_name, detail).await;
    }
}

/// Helper: report a tool-call completion status if a task-local reporter exists.
pub async fn report_tool_complete(tool_name: &str, detail: &str) {
    if let Ok(reporter) = STATUS_REPORTER.try_with(|r| r.clone()) {
        reporter.complete(tool_name, detail).await;
    }
}

/// Helper: report a tool-call error status if a task-local reporter exists.
pub async fn report_tool_error(tool_name: &str, error: &str) {
    if let Ok(reporter) = STATUS_REPORTER.try_with(|r| r.clone()) {
        reporter
            .error_retry(&format!("工具 {} 执行失败: {}", tool_name, error))
            .await;
    }
}
