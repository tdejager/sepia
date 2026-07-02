use std::{fs, path::Path};

use miette::Result;

use crate::ResultContextExt;
use html_escape::encode_text;

use crate::{config::DemoConfig, session::SessionPaths, timeline::TimelinePlan};

pub fn write_inspect_html(
    paths: &SessionPaths,
    config: &DemoConfig,
    plan: &TimelinePlan,
) -> Result<()> {
    let html = render_inspect_html(paths, config, plan);
    fs::write(&paths.inspect_html, html).with_context(|| {
        format!(
            "failed to write inspect UI at {}",
            paths.inspect_html.display()
        )
    })
}

#[must_use]
pub fn render_inspect_html(
    paths: &SessionPaths,
    config: &DemoConfig,
    plan: &TimelinePlan,
) -> String {
    let title = encode_text(&config.name);
    let description = encode_text(config.description.as_deref().unwrap_or(""));
    let video = paths
        .video
        .file_name()
        .and_then(|p| p.to_str())
        .unwrap_or("demo.mp4");

    let mut steps = String::new();
    for segment in &plan.segments {
        steps.push_str(&format!(
            "<li><strong>{}</strong> — {:?}, {}ms, capture {}, hold {}</li>\n",
            encode_text(&segment.step_name),
            segment.kind,
            segment.duration_ms,
            segment.frames_to_capture,
            segment.hold_frames
        ));
    }

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Sepia Inspect — {title}</title>
<style>
body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; margin: 2rem; line-height: 1.45; color: #1f2328; }}
video {{ max-width: 100%; border: 1px solid #d0d7de; border-radius: 8px; }}
code, pre {{ background: #f6f8fa; border-radius: 6px; }}
pre {{ padding: 1rem; overflow: auto; }}
.card {{ border: 1px solid #d0d7de; border-radius: 8px; padding: 1rem; margin: 1rem 0; }}
textarea {{ width: 100%; min-height: 8rem; }}
</style>
</head>
<body>
<h1>Sepia Inspect — {title}</h1>
<p>{description}</p>
<video src="{video}" controls></video>
<div class="card">
<h2>Timeline</h2>
<ol>
{steps}</ol>
</div>
<div class="card">
<h2>Paths</h2>
<pre>Session: {session}
Video:   {video_path}</pre>
</div>
<div class="card">
<h2>Feedback for agent</h2>
<textarea>Changes needed:
- </textarea>
</div>
</body>
</html>
"#,
        session = encode_text(&paths.root.display().to_string()),
        video_path = encode_text(&paths.video.display().to_string()),
    )
}

pub fn open_in_browser(path: &Path) -> Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "xdg-open"
    };

    let mut command = std::process::Command::new(opener);
    if cfg!(target_os = "windows") {
        command.args(["/C", "start", ""]).arg(path);
    } else {
        command.arg(path);
    }
    command
        .spawn()
        .with_context(|| format!("failed to open inspect UI at {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{config::CaptureConfig, timeline::TimelineCompiler};

    use super::*;

    #[test]
    fn renders_human_inspection_page() {
        let config = DemoConfig {
            name: "Demo".into(),
            description: Some("Readable".into()),
            url: "http://localhost".into(),
            session: None,
            capture: CaptureConfig::default(),
            steps: vec![],
        };
        let plan = TimelineCompiler::compile(&config);
        let paths = SessionPaths::from_root(PathBuf::from("/tmp/sepia/demo"));
        let html = render_inspect_html(&paths, &config, &plan);
        assert!(html.contains("<video"));
        assert!(html.contains("Changes needed:"));
    }
}
