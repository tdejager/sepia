use std::{fs, path::PathBuf};

use anyhow::{Context, Result, bail};
use skill::{
    manager::SkillManager,
    types::{
        AgentId, InstallMode, InstallOptions, InstallScope, InstalledSkill, ListOptions,
        RemoveOptions,
    },
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

pub async fn sepia_skill_install_tip() -> Option<String> {
    let manager = SkillManager::builder().build();
    let detected = manager.detect_installed_agents().await;
    if detected.is_empty() {
        return None;
    }

    let installed = manager
        .list_installed(&ListOptions {
            scope: None,
            agent_filter: detected.clone(),
            cwd: None,
        })
        .await
        .ok()?;
    let missing = missing_sepia_skill_agents(&detected, &installed);
    if missing.is_empty() {
        return None;
    }

    let missing = missing
        .into_iter()
        .map(|agent| agent.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "Tip: install the Sepia agent skill for {missing} with `sepia skill install`."
    ))
}

fn missing_sepia_skill_agents(detected: &[AgentId], installed: &[InstalledSkill]) -> Vec<AgentId> {
    let installed_agents = installed
        .iter()
        .filter(|skill| skill.name == "sepia")
        .flat_map(|skill| skill.agents.iter())
        .collect::<std::collections::BTreeSet<_>>();

    detected
        .iter()
        .filter(|agent| !installed_agents.contains(agent))
        .cloned()
        .collect()
}

async fn resolve_target_agents(
    manager: &SkillManager,
    explicit: &[String],
) -> Result<Vec<AgentId>> {
    let detected = if explicit.is_empty() {
        manager.detect_installed_agents().await
    } else {
        Vec::new()
    };
    resolve_target_agents_from_detected(explicit, detected)
}

fn resolve_target_agents_from_detected(
    explicit: &[String],
    detected: Vec<AgentId>,
) -> Result<Vec<AgentId>> {
    if !explicit.is_empty() {
        return Ok(explicit.iter().map(AgentId::new).collect());
    }
    if detected.is_empty() {
        bail!(
            "No installed agents detected for skill installation. Retry with `sepia skill install --agent <agent-id>`."
        );
    }
    Ok(detected)
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

    #[test]
    fn explicit_agents_override_detection() {
        let selected = resolve_target_agents_from_detected(
            &["pi".into(), "codex".into()],
            vec![AgentId::new("claude-code")],
        )
        .unwrap();

        assert_eq!(selected, vec![AgentId::new("pi"), AgentId::new("codex")]);
    }

    #[test]
    fn detected_agents_are_used_when_no_explicit_filter_is_given() {
        let selected = resolve_target_agents_from_detected(
            &[],
            vec![AgentId::new("codex"), AgentId::new("pi")],
        )
        .unwrap();

        assert_eq!(selected, vec![AgentId::new("codex"), AgentId::new("pi")]);
    }

    #[test]
    fn missing_explicit_and_detected_agents_is_an_error() {
        let error = resolve_target_agents_from_detected(&[], vec![]).unwrap_err();

        assert!(error.to_string().contains("No installed agents detected"));
    }

    #[test]
    fn missing_skill_agents_excludes_agents_with_sepia_installed() {
        let missing = missing_sepia_skill_agents(
            &[AgentId::new("pi"), AgentId::new("codex")],
            &[InstalledSkill {
                name: "sepia".into(),
                description: "Sepia skill".into(),
                path: PathBuf::from("/tmp/sepia"),
                canonical_path: None,
                scope: InstallScope::Global,
                agents: vec![AgentId::new("pi")],
            }],
        );

        assert_eq!(missing, vec![AgentId::new("codex")]);
    }

    #[test]
    fn missing_skill_agents_ignores_other_skills() {
        let missing = missing_sepia_skill_agents(
            &[AgentId::new("pi")],
            &[InstalledSkill {
                name: "other".into(),
                description: "Other skill".into(),
                path: PathBuf::from("/tmp/other"),
                canonical_path: None,
                scope: InstallScope::Global,
                agents: vec![AgentId::new("pi")],
            }],
        );

        assert_eq!(missing, vec![AgentId::new("pi")]);
    }
}
