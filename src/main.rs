use std::env;

use anyhow::Result;
use agentlink_rust_sdk::event_handler::{CONNECTION_READY, ERROR, MESSAGE_CREATED};
use rigent::{config::FrameworkConfig, framework::AgentFramework};
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            env::var("RUST_LOG")
                .unwrap_or_else(|_| "rigent=info,agentlink_rust_sdk=warn,rig_core=warn".into()),
        )
        .init();

    let config = FrameworkConfig::from_env()?;
    info!(
        provider = %config.llm_provider,
        model = %config.llm_model,
        skill = %config.skill_name,
        max_turns = config.max_turns,
        "Starting base agent framework"
    );

    let framework = AgentFramework::new(&config).await?;

    // Register message handler
    let msg_framework = framework.clone();
    framework.sdk_client.on(MESSAGE_CREATED, move |payload| {
        let fw = msg_framework.clone();
        async move {
            let msg = payload.message;
            let conversation_id = msg.conversation_id;
            let msg_id = msg.id;

            match fw.handle_message(msg).await {
                Ok(response) if !response.is_empty() => {
                    if let Err(e) = fw.send_reply(&conversation_id.to_string(), response, Some(msg_id)).await {
                        error!(error = %e, "Failed to send reply");
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    error!(error = %e, "Failed to handle message");
                    let error_msg = "Sorry, I encountered an error processing your message. Please try again.".to_string();
                    if let Err(e) = fw.send_reply(&conversation_id.to_string(), error_msg, Some(msg_id)).await {
                        error!(error = %e, "Failed to send error reply");
                    }
                }
            }
        }
    });

    // Register connection event handlers
    framework.sdk_client.on(CONNECTION_READY, |payload| async move {
        info!(
            user_id = %payload.user_id,
            linkid = %payload.linkid,
            "WebSocket connected and ready"
        );
    });

    framework.sdk_client.on(ERROR, |payload| async move {
        error!(
            code = %payload.code,
            message = %payload.message,
            "WebSocket error received"
        );
    });

    // Set agent availability to online
    info!("📡 Sending request to server: set agent availability = ONLINE");
    if let Err(e) = framework.set_availability(true).await {
        error!(error = %e, "❌ Failed to set agent availability to online");
    } else {
        info!("✅ Server confirmed: agent availability set to ONLINE");
    }

    // Run WebSocket event loop
    let mut poll_client = framework.sdk_client.clone();
    let poll_handle = tokio::spawn(async move {
        info!("Entering WebSocket event poll loop...");
        if let Err(e) = poll_client.poll().await {
            error!(error = %e, "Event poll loop ended with error");
        }
    });

    // Wait for shutdown signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT, shutting down gracefully...");
        }
        _ = async {
            #[cfg(unix)]
            {
                let mut sigterm = tokio::signal::unix::signal(
                    tokio::signal::unix::SignalKind::terminate()
                ).expect("Failed to create SIGTERM handler");
                sigterm.recv().await;
            }
            #[cfg(not(unix))]
            {
                std::future::pending::<()>().await;
            }
        } => {
            info!("Received SIGTERM, shutting down gracefully...");
        }
        result = poll_handle => {
            if let Err(e) = result {
                error!(error = %e, "Poll task panicked");
            }
            info!("Poll loop ended, shutting down...");
        }
    }

    // Set agent availability to offline on shutdown
    info!("📡 Sending request to server: set agent availability = OFFLINE");
    if let Err(e) = framework.set_availability(false).await {
        error!(error = %e, "❌ Failed to set agent availability to offline");
    } else {
        info!("✅ Server confirmed: agent availability set to OFFLINE");
    }

    info!("Base agent stopped");
    Ok(())
}
