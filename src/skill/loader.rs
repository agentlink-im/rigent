use std::path::PathBuf;

use agentlink_rust_sdk::AgentLinkClient;
use anyhow::{Context, Result};
use regex::Regex;

use super::platform::load_platform_skill;
use super::types::{Skill, SkillMeta};

pub enum SkillSource {
    Local { skills_root: PathBuf },
    Platform {
        client: std::sync::Arc<AgentLinkClient>,
    },
}

pub struct SkillLoader {
    source: SkillSource,
}

impl SkillLoader {
    pub fn local(skills_root: impl Into<PathBuf>) -> Self {
        Self {
            source: SkillSource::Local {
                skills_root: skills_root.into(),
            },
        }
    }

    pub fn platform(client: std::sync::Arc<AgentLinkClient>) -> Self {
        Self {
            source: SkillSource::Platform { client },
        }
    }

    pub async fn load(&self, name_or_id: &str) -> Result<Skill> {
        match &self.source {
            SkillSource::Local { skills_root } => load_local_skill(skills_root, name_or_id),
            SkillSource::Platform { client } => {
                load_platform_skill(client, name_or_id).await
            }
        }
    }
}

fn load_local_skill(skills_root: &PathBuf, name: &str) -> Result<Skill> {
    let path = skills_root.join(name).join("SKILL.md");
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read skill file: {}", path.display()))?;

    let (meta, content) = parse_skill_md(&raw)
        .with_context(|| format!("Failed to parse skill file: {}", path.display()))?;

    Ok(Skill { meta, content })
}

pub fn parse_skill_md(raw: &str) -> Result<(SkillMeta, String)> {
    let re = Regex::new(r"(?s)^---\s*\n(.*?)\n---\s*\n?(.*)$").expect("valid regex");

    let caps = re
        .captures(raw.trim())
        .context("SKILL.md must start with YAML frontmatter delimited by ---")?;

    let frontmatter = caps.get(1).context("missing frontmatter")?.as_str();
    let content = caps.get(2).context("missing content")?.as_str().trim().to_string();

    let meta: SkillMeta = serde_yaml::from_str(frontmatter)
        .context("Failed to parse YAML frontmatter in SKILL.md")?;

    Ok((meta, content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_md() {
        let raw = r#"---
name: audit
description: Run technical quality checks.
version: 2.1.1
user_invocable: true
---

## MANDATORY PREPARATION

Invoke /impeccable first.
"#;

        let (meta, content) = parse_skill_md(raw).unwrap();
        assert_eq!(meta.name, "audit");
        assert_eq!(meta.description, "Run technical quality checks.");
        assert_eq!(meta.version, "2.1.1");
        assert!(meta.user_invocable);
        assert!(content.contains("MANDATORY PREPARATION"));
    }
}
