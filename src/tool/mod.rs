pub mod agentlink;
pub mod local;

use std::sync::Arc;

use agentlink_rust_sdk::AgentLinkClient;
use rig::tool::ToolDyn;

use self::agentlink::{
    GetTaskTool, GetUserProfileTool, ListMyTasksTool, SearchTasksTool, SendMessageTool,
};
use self::local::{FileList, FileRead, FileWrite, ShellExecute, WebFetch};

/// Build the complete set of tools available to the agent.
pub fn build_tools(client: Arc<AgentLinkClient>) -> Vec<Box<dyn ToolDyn>> {
    vec![
        // AgentLink platform tools
        Box::new(SendMessageTool::new(client.clone())),
        Box::new(GetTaskTool::new(client.clone())),
        Box::new(ListMyTasksTool::new(client.clone())),
        Box::new(SearchTasksTool::new(client.clone())),
        Box::new(GetUserProfileTool::new(client.clone())),
        // Local environment tools
        Box::new(FileRead),
        Box::new(FileWrite),
        Box::new(FileList),
        Box::new(ShellExecute::default()),
        Box::new(WebFetch),
    ]
}
