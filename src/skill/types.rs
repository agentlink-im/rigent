use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default = "default_user_invocable")]
    pub user_invocable: bool,
    #[serde(default)]
    pub argument_hint: Option<String>,
}

fn default_user_invocable() -> bool {
    true
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub meta: SkillMeta,
    pub content: String,
}

impl Skill {
    pub fn system_prompt_extension(&self) -> String {
        format!(
            "# Skill: {}\n\n## Description\n{}\n\n## Instructions\n{}\n",
            self.meta.name,
            self.meta.description,
            self.content
        )
    }
}
