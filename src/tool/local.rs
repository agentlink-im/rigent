use std::collections::HashSet;

use anyhow::Result;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, info, warn};

// ===================================================================
// File Read Tool
// ===================================================================

#[derive(Deserialize, Serialize)]
pub struct FileRead;

#[derive(Deserialize)]
pub struct FileReadArgs {
    path: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LocalToolError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Path not allowed: {0}")]
    PathNotAllowed(String),
}

impl Tool for FileRead {
    const NAME: &'static str = "file_read";
    type Error = LocalToolError;
    type Args = FileReadArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Read the contents of a local file.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute or relative path to the file" }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!(tool = Self::NAME, path = %args.path, "Executing tool");
        let content = tokio::fs::read_to_string(&args.path).await?;
        debug!(tool = Self::NAME, path = %args.path, bytes = content.len(), "Tool completed");
        Ok(content)
    }
}

// ===================================================================
// File Write Tool
// ===================================================================

#[derive(Deserialize, Serialize)]
pub struct FileWrite;

#[derive(Deserialize)]
pub struct FileWriteArgs {
    path: String,
    content: String,
}

impl Tool for FileWrite {
    const NAME: &'static str = "file_write";
    type Error = LocalToolError;
    type Args = FileWriteArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Write content to a local file (creates or overwrites).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to the file" },
                    "content": { "type": "string", "description": "Content to write" }
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!(tool = Self::NAME, path = %args.path, bytes = args.content.len(), "Executing tool");
        tokio::fs::write(&args.path, &args.content).await?;
        info!(tool = Self::NAME, path = %args.path, "File written successfully");
        Ok(format!("File written successfully: {}", args.path))
    }
}

// ===================================================================
// File List Tool
// ===================================================================

#[derive(Deserialize, Serialize)]
pub struct FileList;

#[derive(Deserialize)]
pub struct FileListArgs {
    path: String,
}

impl Tool for FileList {
    const NAME: &'static str = "file_list";
    type Error = LocalToolError;
    type Args = FileListArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "List files and directories at the given path.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to list" }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!(tool = Self::NAME, path = %args.path, "Executing tool");
        let mut entries = tokio::fs::read_dir(&args.path).await?;
        let mut lines = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().to_string();
            let typ = if entry.file_type().await?.is_dir() {
                "dir"
            } else {
                "file"
            };
            lines.push(format!("{} ({typ})", name));
        }
        debug!(tool = Self::NAME, path = %args.path, entries = lines.len(), "Tool completed");
        Ok(lines.join("\n"))
    }
}

// ===================================================================
// Shell Execute Tool
// ===================================================================

#[derive(Deserialize, Serialize)]
pub struct ShellExecute {
    forbidden_commands: HashSet<String>,
}

impl Default for ShellExecute {
    fn default() -> Self {
        let mut forbidden = HashSet::new();
        forbidden.insert("rm".to_string());
        forbidden.insert("mv".to_string());
        forbidden.insert("dd".to_string());
        forbidden.insert("mkfs".to_string());
        forbidden.insert("fdisk".to_string());
        forbidden.insert("format".to_string());
        Self {
            forbidden_commands: forbidden,
        }
    }
}

#[derive(Deserialize)]
pub struct ShellExecuteArgs {
    command: String,
}

impl Tool for ShellExecute {
    const NAME: &'static str = "shell_execute";
    type Error = LocalToolError;
    type Args = ShellExecuteArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Execute a shell command. Forbidden commands: rm, mv, dd, mkfs, fdisk, format.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute" }
                },
                "required": ["command"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let cmd = args.command.trim();
        info!(tool = Self::NAME, command = %cmd, "Executing tool");
        let first_token = cmd.split_whitespace().next().unwrap_or("");

        if self.forbidden_commands.contains(first_token) {
            warn!(tool = Self::NAME, command = %first_token, "Forbidden command rejected");
            return Err(LocalToolError::PathNotAllowed(format!(
                "Command '{}' is forbidden for security reasons.",
                first_token
            )));
        }

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let mut result = String::new();
        if !stdout.is_empty() {
            result.push_str(&format!("STDOUT:\n{stdout}\n"));
        }
        if !stderr.is_empty() {
            result.push_str(&format!("STDERR:\n{stderr}\n"));
        }
        if result.is_empty() {
            result = "(no output)".to_string();
        }

        info!(
            tool = Self::NAME,
            exit_code = output.status.code(),
            stdout_len = stdout.len(),
            stderr_len = stderr.len(),
            "Shell command completed"
        );
        Ok(result)
    }
}

// ===================================================================
// Web Fetch Tool
// ===================================================================

#[derive(Deserialize, Serialize)]
pub struct WebFetch;

#[derive(Deserialize)]
pub struct WebFetchArgs {
    url: String,
}

#[derive(Debug, thiserror::Error)]
pub enum WebFetchError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

impl Tool for WebFetch {
    const NAME: &'static str = "web_fetch";
    type Error = WebFetchError;
    type Args = WebFetchArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Fetch the content of a web page by URL.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL to fetch" }
                },
                "required": ["url"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!(tool = Self::NAME, url = %args.url, "Executing tool");
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let text = client.get(&args.url).send().await?.text().await?;
        info!(tool = Self::NAME, url = %args.url, bytes = text.len(), "Web fetch completed");
        // Truncate if too long
        let max_len = 100_000;
        if text.len() > max_len {
            Ok(format!("{}\n...[truncated {} chars]", &text[..max_len], text.len() - max_len))
        } else {
            Ok(text)
        }
    }
}
