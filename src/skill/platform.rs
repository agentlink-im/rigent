use std::io::{Cursor, Read};

use agentlink_rust_sdk::AgentLinkClient;
use anyhow::{Context, Result};

use super::loader::parse_skill_md;
use super::types::{Skill, SkillMeta};

pub async fn load_platform_skill(client: &AgentLinkClient, skill_id: &str) -> Result<Skill> {
    // 1. Fetch skill detail metadata
    let detail = client
        .skills
        .get_skill_detail(skill_id)
        .await
        .with_context(|| format!("Failed to fetch skill detail for id: {}", skill_id))?;

    // 2. Download skill bundle
    let bundle_bytes = client
        .skills
        .download_skill_bundle(skill_id)
        .await
        .with_context(|| format!("Failed to download skill bundle for id: {}", skill_id))?;

    // 3. Extract SKILL.md from zip
    let skill_md = extract_skill_md(&bundle_bytes)
        .with_context(|| "Failed to extract SKILL.md from skill bundle")?;

    // 4. Parse SKILL.md
    let (meta, md_content) = if let Some(ref md) = skill_md {
        parse_skill_md(md).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Failed to parse SKILL.md frontmatter, using raw content");
            (
                SkillMeta {
                    name: detail.name.clone(),
                    description: detail.description.clone().unwrap_or_default(),
                    version: detail.version.clone().unwrap_or_default(),
                    user_invocable: true,
                    argument_hint: None,
                },
                md.clone(),
            )
        })
    } else {
        (
            SkillMeta {
                name: detail.name.clone(),
                description: detail.description.clone().unwrap_or_default(),
                version: detail.version.clone().unwrap_or_default(),
                user_invocable: true,
                argument_hint: None,
            },
            String::new(),
        )
    };

    // 5. Merge platform metadata with SKILL.md content
    let content = merge_content(&detail, &md_content);

    Ok(Skill { meta, content })
}

fn extract_skill_md(bundle_bytes: &[u8]) -> Result<Option<String>> {
    let reader = Cursor::new(bundle_bytes);
    let mut archive = zip::ZipArchive::new(reader)
        .context("Failed to read skill bundle as ZIP archive")?;

    // Look for skill/SKILL.md in the bundle
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name();
        if name.ends_with("SKILL.md") && !name.starts_with("__MACOSX") {
            let mut content = String::new();
            file.read_to_string(&mut content)?;
            return Ok(Some(content));
        }
    }

    Ok(None)
}

fn merge_content(
    detail: &agentlink_protocol::skill::SkillDetailView,
    skill_md_content: &str,
) -> String {
    let mut parts = Vec::new();

    parts.push(format!("# Skill: {}\n", detail.name));

    if let Some(ref desc) = detail.description {
        parts.push(format!("## Description\n{}\n", desc));
    }

    if let Some(ref long_desc) = detail.long_description {
        parts.push(format!("## Overview\n{}\n", long_desc));
    }

    if !detail.capabilities.is_empty() {
        parts.push("## Capabilities".to_string());
        for cap in &detail.capabilities {
            parts.push(format!("- {}", cap));
        }
        parts.push(String::new());
    }

    if !detail.use_cases.is_empty() {
        parts.push("## Use Cases".to_string());
        for case in &detail.use_cases {
            parts.push(format!("- {}", case));
        }
        parts.push(String::new());
    }

    if !detail.example_prompts.is_empty() {
        parts.push("## Example Prompts".to_string());
        for prompt in &detail.example_prompts {
            parts.push(format!("- {}", prompt));
        }
        parts.push(String::new());
    }

    if !skill_md_content.is_empty() {
        parts.push("## Instructions".to_string());
        parts.push(skill_md_content.to_string());
    }

    parts.join("\n")
}
