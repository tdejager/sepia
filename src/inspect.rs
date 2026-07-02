use std::{fs, path::Path};

use miette::Result;

use crate::ResultContextExt;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use html_escape::encode_text;

use crate::{metadata::SessionMetadata, session::SessionPaths};

// Latin `woff2` subsets embedded so the inspect page stays self-contained and
// offline-safe when opened straight from a session directory.
const FONT_SILKSCREEN: &[u8] = include_bytes!("../assets/fonts/silkscreen-400.woff2");
const FONT_GAEGU_400: &[u8] = include_bytes!("../assets/fonts/gaegu-400.woff2");
const FONT_GAEGU_700: &[u8] = include_bytes!("../assets/fonts/gaegu-700.woff2");
const FONT_LORA_400: &[u8] = include_bytes!("../assets/fonts/lora-400.woff2");
const FONT_LORA_600: &[u8] = include_bytes!("../assets/fonts/lora-600.woff2");
const FONT_LORA_400_ITALIC: &[u8] = include_bytes!("../assets/fonts/lora-400-italic.woff2");
const FONT_SPACEMONO: &[u8] = include_bytes!("../assets/fonts/spacemono-400.woff2");
const MASCOT: &[u8] = include_bytes!("../assets/mascot.png");

pub fn write_inspect_html(paths: &SessionPaths, metadata: &SessionMetadata) -> Result<()> {
    let html = render_inspect_html(paths, metadata);
    fs::write(&paths.inspect_html, html).with_context(|| {
        format!(
            "failed to write inspect UI at {}",
            paths.inspect_html.display()
        )
    })
}

#[must_use]
pub fn render_inspect_html(paths: &SessionPaths, metadata: &SessionMetadata) -> String {
    let fps = f64::from(metadata.output_fps.max(1));
    let total = f64::from(metadata.frame_count) / fps;

    let title = encode_text(&metadata.name);
    let description = encode_text(metadata.description.as_deref().unwrap_or(""));
    let video_name = metadata
        .video
        .file_name()
        .and_then(|p| p.to_str())
        .unwrap_or("demo.mp4");

    let badges = format!(
        concat!(
            "<span class=\"badge\"><b>{}</b> STEPS</span>",
            "<span class=\"badge\"><b>{}</b> FPS</span>",
            "<span class=\"badge\"><b>{}</b> FRAMES</span>",
            "<span class=\"badge\"><b>{:.1}</b>S</span>",
            "<span class=\"badge\">COLOR-CORRECT</span>",
        ),
        metadata.steps.len(),
        metadata.output_fps,
        metadata.frame_count,
        total,
    );

    // Per-step data for the journey list and click-to-seek. Escaping happens in
    // the browser via textContent, so JSON is the only encoding needed here.
    let steps_json: Vec<serde_json::Value> = metadata
        .steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let step_frames = step.captured_frames + step.hold_frames;
            let duration = f64::from(step_frames) / fps;
            let start = f64::from(step.start_frame.saturating_sub(1)) / fps;
            let thumb = step
                .screenshot
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|f| format!("steps/{}", f.to_string_lossy()));
            serde_json::json!({
                "n": format!("{:02}", i + 1),
                "name": step.name,
                "kind": step.kind.label(),
                "note": format!("{} · {:.1}s", step.kind.note(), duration),
                "thumb": thumb,
                "t": start,
            })
        })
        .collect();
    let steps_data = serde_json::to_string(&steps_json)
        .unwrap_or_else(|_| "[]".to_owned())
        .replace("</", "<\\/");

    let consts = format!(
        "const FPS={};const TOTAL={:.4};const STEPS={};",
        metadata.output_fps, total, steps_data
    );

    let paths_html = format!(
        "<b>session</b> {session}<br><b>video</b> {video}<br><b>pr block</b> {pr}",
        session = encode_text(&paths.root.display().to_string()),
        video = encode_text(&metadata.video.display().to_string()),
        pr = encode_text(&paths.pr_comment_md.display().to_string()),
    );

    let created = metadata.created_at.format("%Y-%m-%d %H:%M");
    let mascot = data_uri("image/png", MASCOT);

    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Sepia Inspect — {title}</title>
<style>
{fonts}
{style}
</style>
</head>
<body>
<div class="sea" id="sea"></div>
<div class="wrap">
  <header>
    <img class="mascot" src="{mascot}" alt="Sepia the cuttlefish">
    <div class="brandbox">
      <div class="wordmark">Sepia</div>
      <div class="hello">your demo is ready — take a look! ✦</div>
      <div class="badges">{badges}</div>
    </div>
  </header>

  <div class="layout">
    <div class="card watch">
      <div class="title"><span class="dot"></span>Watch it back</div>
      <div class="screen"><video id="vid" src="{video}" preload="metadata" playsinline></video></div>
      <div class="bar">
        <button class="play" id="play" aria-label="Play or pause">▶</button>
        <div class="rail" id="rail"><div class="prog" id="prog"></div><div class="knob" id="knob">🫧</div></div>
        <div class="tc" id="tc">0.0 / {total:.1}s</div>
      </div>
    </div>

    <div class="card journey">
      <div class="title"><span class="dot"></span>The little journey · tap to jump</div>
      <div class="steps" id="steps"></div>
    </div>

    <div class="card paths-card">
      <div class="title"><span class="dot"></span>Kept safely over here</div>
      <div class="paths">{paths_html}</div>
    </div>

    <div class="card note-card">
      <div class="title"><span class="dot"></span>Tell the agent what to tweak</div>
      <p class="desc">{description}</p>
      <textarea>Changes needed:
- </textarea>
    </div>
  </div>

  <footer>recorded {created} · made with a <span class="h">♥</span> by <span class="pix">SEPIA</span> the cuttlefish</footer>
</div>

<script>
{consts}
{script}
</script>
</body>
</html>
"##,
        title = title,
        fonts = font_faces(),
        style = STYLE,
        mascot = mascot,
        badges = badges,
        video = encode_text(video_name),
        total = total,
        paths_html = paths_html,
        description = description,
        created = created,
        consts = consts,
        script = SCRIPT,
    )
}

/// Embedded latin font faces: (family, weight, style, bytes).
const FONTS: &[(&str, u32, &str, &[u8])] = &[
    ("Silkscreen", 400, "normal", FONT_SILKSCREEN),
    ("Gaegu", 400, "normal", FONT_GAEGU_400),
    ("Gaegu", 700, "normal", FONT_GAEGU_700),
    ("Lora", 400, "normal", FONT_LORA_400),
    ("Lora", 600, "normal", FONT_LORA_600),
    ("Lora", 400, "italic", FONT_LORA_400_ITALIC),
    ("Space Mono", 400, "normal", FONT_SPACEMONO),
];

fn font_faces() -> String {
    FONTS
        .iter()
        .map(|(family, weight, style, bytes)| {
            format!(
                "@font-face{{font-family:'{family}';font-style:{style};font-weight:{weight};\
                 font-display:swap;src:url({src}) format('woff2')}}",
                src = data_uri("font/woff2", bytes),
            )
        })
        .collect()
}

/// A `data:` URI with base64-encoded bytes, used to inline fonts and images.
fn data_uri(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

const STYLE: &str = include_str!("../assets/inspect/style.css");

const SCRIPT: &str = include_str!("../assets/inspect/script.js");

pub fn open_in_browser(path: &Path) -> Result<()> {
    opener::open_browser(path)
        .with_context(|| format!("failed to open inspect UI at {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::TimeZone;

    use crate::metadata::{SessionMetadata, SessionStep};

    use super::*;

    fn sample_metadata() -> SessionMetadata {
        SessionMetadata {
            name: "Windowed Browse".into(),
            description: Some("Windowed package and advisory browse".into()),
            url: "http://localhost:3001".into(),
            created_at: chrono::Local
                .with_ymd_and_hms(2026, 7, 2, 14, 15, 0)
                .unwrap(),
            output_fps: 24,
            frame_count: 120,
            video: PathBuf::from("/tmp/sepia/demo/demo.mp4"),
            inspect: PathBuf::from("/tmp/sepia/demo/inspect.html"),
            screenshots: vec![],
            steps: vec![
                SessionStep {
                    name: "Initial packages page".into(),
                    kind: crate::config::StepKind::Wait,
                    start_frame: 2,
                    captured_frames: 1,
                    hold_frames: 29,
                    screenshot: Some(PathBuf::from("/tmp/sepia/demo/steps/step-01-initial.png")),
                },
                SessionStep {
                    name: "Scroll package list".into(),
                    kind: crate::config::StepKind::Scroll,
                    start_frame: 32,
                    captured_frames: 32,
                    hold_frames: 17,
                    screenshot: None,
                },
            ],
        }
    }

    #[test]
    fn renders_human_inspection_page() {
        let paths = SessionPaths::from_root(PathBuf::from("/tmp/sepia/demo"));
        let html = render_inspect_html(&paths, &sample_metadata());
        assert!(html.contains("<video"));
        assert!(html.contains("Changes needed:"));
    }

    #[test]
    fn embeds_fonts_and_mascot_for_offline_use() {
        let paths = SessionPaths::from_root(PathBuf::from("/tmp/sepia/demo"));
        let html = render_inspect_html(&paths, &sample_metadata());
        assert!(html.contains("data:font/woff2;base64,"));
        assert!(html.contains("data:image/png;base64,"));
        // No external network requests.
        assert!(!html.contains("fonts.googleapis.com"));
    }

    #[test]
    fn embeds_per_step_seek_data() {
        let paths = SessionPaths::from_root(PathBuf::from("/tmp/sepia/demo"));
        let html = render_inspect_html(&paths, &sample_metadata());
        assert!(html.contains("Scroll package list"));
        assert!(html.contains("steps/step-01-initial.png"));
        assert!(html.contains("const STEPS="));
    }
}
