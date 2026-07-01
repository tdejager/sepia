use std::{fs, path::PathBuf};

use anyhow::{Context, Result, bail};
use skill::{
    manager::SkillManager,
    types::{AgentId, InstallMode, InstallOptions, InstallScope, ListOptions, RemoveOptions},
};
use tempfile::tempdir;

pub const EMBEDDED_SKILL: &str = include_str!("../skills/sepia/SKILL.md");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInstallRequest {
    pub agents: Vec<String>,
    pub global: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInstallSummary {
    pub installed_agents: Vec<String>,
}

pub async fn install_embedded_skill(request: SkillInstallRequest) -> Result<SkillInstallSummary> {
    let manager = SkillManager::builder().build();
    let temp = tempdir().context("failed to create temporary skill install directory")?;
    let skill_dir = temp.path().join("sepia");
    fs::create_dir_all(&skill_dir).context("failed to create temporary Sepia skill directory")?;
    fs::write(skill_dir.join("SKILL.md"), EMBEDDED_SKILL)
        .context("failed to write embedded Sepia skill")?;

    let skills = manager
        .discover_skills(temp.path(), &Default::default())
        .await
        .context("failed to discover embedded Sepia skill")?;
    let sepia = skills
        .iter()
        .find(|skill| skill.name == "sepia")
        .context("embedded Sepia skill was not discovered")?;

    let target_agents = resolve_target_agents(&manager, &request.agents).await?;
    let options = InstallOptions {
        scope: if request.global {
            InstallScope::Global
        } else {
            InstallScope::Project
        },
        mode: InstallMode::Copy,
        cwd: None,
    };

    let mut installed_agents = Vec::new();
    for agent in target_agents {
        manager
            .install_skill(sepia, &agent, &options)
            .await
            .with_context(|| format!("failed to install Sepia skill for agent `{agent}`"))?;
        installed_agents.push(agent.to_string());
    }

    Ok(SkillInstallSummary { installed_agents })
}

pub async fn list_installed_skills(global: bool, agents: Vec<String>) -> Result<Vec<String>> {
    let manager = SkillManager::builder().build();
    let agent_filter = agents.into_iter().map(AgentId::new).collect();
    let installed = manager
        .list_installed(&ListOptions {
            scope: Some(if global {
                InstallScope::Global
            } else {
                InstallScope::Project
            }),
            agent_filter,
            cwd: None,
        })
        .await?;
    Ok(installed
        .into_iter()
        .map(|skill| format!("{} ({})", skill.name, skill.path.display()))
        .collect())
}

pub async fn remove_embedded_skill(global: bool, agents: Vec<String>) -> Result<()> {
    let manager = SkillManager::builder().build();
    manager
        .remove_skills(
            &["sepia".to_string()],
            &RemoveOptions {
                scope: if global {
                    InstallScope::Global
                } else {
                    InstallScope::Project
                },
                agents: agents.into_iter().map(AgentId::new).collect(),
                cwd: None,
            },
        )
        .await?;
    Ok(())
}

async fn resolve_target_agents(
    manager: &SkillManager,
    explicit: &[String],
) -> Result<Vec<AgentId>> {
    if explicit.is_empty() {
        let detected = manager.detect_installed_agents().await;
        if detected.is_empty() {
            bail!(
                "No installed agents detected for skill installation. Retry with `sepia skill install --agent <agent-id>`."
            );
        }
        return Ok(detected);
    }
    Ok(explicit.iter().map(AgentId::new).collect())
}

#[must_use]
pub fn bundled_skill_preview() -> &'static str {
    EMBEDDED_SKILL
}

#[must_use]
pub fn default_skill_source_path() -> PathBuf {
    PathBuf::from("skills/sepia/SKILL.md")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_standard_skill_descriptor() {
        assert!(bundled_skill_preview().contains("name: sepia"));
        assert!(bundled_skill_preview().contains("description:"));
    }
}
