use std::collections::HashMap;
use std::sync::Arc;

use rig::completion::Message;
use rig::message::{AssistantContent, ToolResultContent, UserContent};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::agent::AgentRunner;

/// Ratio of `stm_limit` at which background compression is triggered.
const SOFT_LIMIT_RATIO: f64 = 0.8;

/// Per-conversation state holding short-term and long-term memory.
struct Conversation {
    /// Short-term memory: full recent messages (including tool calls / tool results)
    stm: Vec<Message>,
    /// Long-term memory: summaries of older conversation segments
    ltm: Vec<String>,
    /// Background compression task spawned when the soft limit is hit.
    /// The oldest batch is drained from STM immediately; the handle is stored
    /// here so that subsequent `push_message` (hard limit) and `get_history`
    /// can await the LLM summary and move it into LTM.
    compression_task: Option<JoinHandle<anyhow::Result<String>>>,
}

/// Layered conversation memory with STM + LTM.
///
/// - **STM** keeps the most recent `stm_limit` messages in full fidelity.
/// - **LTM** stores summaries. When STM overflows, the oldest batch is
///   summarized by the LLM and moved into LTM.
///
/// # Compression behaviour
///
/// | STM usage | Action |
/// |-----------|--------|
/// | ≤ 80 % (`stm_limit`) | No-op |
/// | > 80 % | Drain oldest batch from STM and spawn a **background** task to
///   summarise them. The caller is **not** blocked. |
/// | > 100 % | If a background compression is still running, **block** until it
///   finishes. If STM is still over the limit afterwards, compact
///   **synchronously** inline. |
#[derive(Clone)]
pub struct ConversationMemory {
    store: Arc<RwLock<HashMap<Uuid, Arc<Mutex<Conversation>>>>>,
    stm_limit: usize,
    ltm_batch_size: usize,
    agent: Arc<AgentRunner>,
}

impl ConversationMemory {
    pub fn new(stm_limit: usize, ltm_batch_size: usize, agent: Arc<AgentRunner>) -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            stm_limit,
            ltm_batch_size: ltm_batch_size.max(1),
            agent,
        }
    }

    /// Get or create the conversation state for a given ID.
    async fn get_or_create(&self, conversation_id: Uuid) -> Arc<Mutex<Conversation>> {
        {
            let read_guard = self.store.read().await;
            if let Some(mem) = read_guard.get(&conversation_id) {
                return mem.clone();
            }
        }

        let mut write_guard = self.store.write().await;
        write_guard
            .entry(conversation_id)
            .or_insert_with(|| {
                Arc::new(Mutex::new(Conversation {
                    stm: Vec::new(),
                    ltm: Vec::new(),
                    compression_task: None,
                }))
            })
            .clone()
    }

    /// Await a pending background compression task and move its result into LTM.
    async fn await_compression(&self, mem: &Arc<Mutex<Conversation>>) {
        let task = {
            let mut guard = mem.lock().await;
            guard.compression_task.take()
        };

        if let Some(task) = task {
            match task.await {
                Ok(Ok(summary)) => {
                    let mut guard = mem.lock().await;
                    guard.ltm.push(summary);
                }
                Ok(Err(e)) => {
                    warn!(error = %e, "Background compression failed");
                }
                Err(e) => {
                    warn!(error = %e, "Background compression task panicked");
                }
            }
        }
    }

    /// If a background compression has already finished, await it (non-blocking)
    /// and move its result into LTM.
    async fn try_flush_finished(&self, mem: &Arc<Mutex<Conversation>>) -> bool {
        let should_flush = {
            let guard = mem.lock().await;
            guard.compression_task.as_ref().is_some_and(|t| t.is_finished())
        };

        if should_flush {
            self.await_compression(mem).await;
            true
        } else {
            false
        }
    }

    /// Build the full history to send to the LLM.
    ///
    /// LTM summaries are injected as system messages at the front,
    /// followed by the complete STM messages.
    ///
    /// Any pending background compression is awaited first so that no messages
    /// are lost between STM and LTM.
    pub async fn get_history(&self, conversation_id: Uuid) -> Vec<Message> {
        let mem = self.get_or_create(conversation_id).await;

        // Flush background compression so the history is complete.
        self.await_compression(&mem).await;

        let guard = mem.lock().await;

        let mut history = Vec::new();

        // Inject LTM summaries as system messages
        let ltm_count = guard.ltm.len();
        if ltm_count > 0 {
            let combined = guard.ltm.join("\n---\n");
            history.push(Message::system(format!(
                "Previous conversation summaries:\n{}",
                combined
            )));
        }

        // Append STM in full fidelity
        let stm_count = guard.stm.len();
        history.extend(guard.stm.clone());

        debug!(
            ltm_summaries = ltm_count,
            stm_messages = stm_count,
            total_history = history.len(),
            "Built conversation history"
        );

        history
    }

    /// Append a message to STM and trigger compression if a limit is exceeded.
    pub async fn push_message(&self, conversation_id: Uuid, message: Message) {
        let mem = self.get_or_create(conversation_id).await;

        // Non-blocking flush of any already-finished background compression.
        self.try_flush_finished(&mem).await;

        let mut guard = mem.lock().await;

        guard.stm.push(message);
        let stm_len = guard.stm.len();
        debug!(conversation_id = %conversation_id, stm_len, "Message pushed to STM");

        let soft_limit = (self.stm_limit as f64 * SOFT_LIMIT_RATIO).ceil() as usize;
        let hard_limit = self.stm_limit;

        if stm_len > hard_limit {
            // Hard limit: if a background compression is still running, block
            // until it finishes before we decide whether to compact inline.
            info!(stm_len, hard_limit, "STM hard limit reached, compacting");
            let has_pending = guard.compression_task.is_some();
            if has_pending {
                info!("Waiting for background compression to finish");
                drop(guard);
                self.await_compression(&mem).await;
                guard = mem.lock().await;
            }

            // After waiting (or if there was nothing to wait for), if STM is
            // still over the hard limit, compact synchronously.
            if guard.stm.len() > hard_limit {
                if let Err(e) = self.compact(&mut guard).await {
                    warn!(error = %e, "Failed to compact conversation memory");
                }
            }
        } else if stm_len > soft_limit && guard.compression_task.is_none() {
            // Soft limit: spawn background compression so that by the time we
            // hit the hard limit the expensive LLM call is (hopefully) already
            // done.
            info!(
                stm_len,
                soft_limit,
                batch_size = self.ltm_batch_size,
                "STM soft limit reached, spawning background compression"
            );
            let batch_size = std::cmp::min(self.ltm_batch_size, guard.stm.len());
            let old_messages: Vec<Message> = guard.stm.drain(0..batch_size).collect();

            let agent = self.agent.clone();
            let handle = tokio::spawn(async move {
                let summary_text = format_messages_for_summary(&old_messages);

                let prompt = format!(
                    "Summarize the following conversation segment concisely. \
                     Preserve key facts, decisions, and context. \
                     Discard redundant or trivial details. \
                     Keep it under 3 sentences.\n\n{}",
                    summary_text
                );

                agent.prompt(&prompt).await
            });

            guard.compression_task = Some(handle);
        }
    }

    /// Extract the oldest batch from STM, summarize them via LLM, and push the
    /// summary into LTM.
    async fn compact(&self, conv: &mut Conversation) -> anyhow::Result<()> {
        let batch_size = std::cmp::min(self.ltm_batch_size, conv.stm.len());
        let old_messages: Vec<Message> = conv.stm.drain(0..batch_size).collect();

        let summary_text = format_messages_for_summary(&old_messages);

        let prompt = format!(
            "Summarize the following conversation segment concisely. \
             Preserve key facts, decisions, and context. \
             Discard redundant or trivial details. \
             Keep it under 3 sentences.\n\n{}",
            summary_text
        );

        match self.agent.prompt(&prompt).await {
            Ok(summary) => {
                info!(
                    batch_size,
                    summary_len = summary.len(),
                    "Compacted STM batch into LTM summary"
                );
                conv.ltm.push(summary);
            }
            Err(e) => {
                warn!(error = %e, "Summary generation failed; old messages are dropped");
            }
        }

        Ok(())
    }
}

/// Render a slice of Rig messages into plain text suitable for summarization.
fn format_messages_for_summary(messages: &[Message]) -> String {
    let mut lines = Vec::new();
    for msg in messages {
        match msg {
            Message::System { content } => {
                lines.push(format!("System: {}", content));
            }
            Message::User { content } => {
                let text = format_user_content(content);
                lines.push(format!("User: {}", text));
            }
            Message::Assistant { content, .. } => {
                let text = format_assistant_content(content);
                lines.push(format!("Assistant: {}", text));
            }
        }
    }
    lines.join("\n")
}

/// Extract plain text from user content (handles Text and ToolResult).
fn format_user_content(content: &rig::OneOrMany<UserContent>) -> String {
    content
        .iter()
        .map(|item| match item {
            UserContent::Text(t) => t.text.clone(),
            UserContent::ToolResult(tr) => {
                let parts: Vec<String> = tr
                    .content
                    .iter()
                    .map(|tc| match tc {
                        ToolResultContent::Text(t) => t.text.clone(),
                        _ => "[non-text tool result]".to_string(),
                    })
                    .collect();
                format!("[Tool {} result: {}]", tr.id, parts.join(" "))
            }
            _ => "[non-text content]".to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract plain text from assistant content (handles Text and ToolCall).
fn format_assistant_content(content: &rig::OneOrMany<AssistantContent>) -> String {
    content
        .iter()
        .map(|item| match item {
            AssistantContent::Text(t) => t.text.clone(),
            AssistantContent::ToolCall(tc) => format!(
                "[Tool call: {}({})]",
                tc.function.name, tc.function.arguments
            ),
            _ => "[non-text content]".to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}
