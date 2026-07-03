use std::{fs, path::Path};

use miette::Result;

use crate::ResultContextExt;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use html_escape::encode_text;

use crate::{metadata::SessionMetadata, session::SessionPaths};

// Latin `woff2` subsets embedded so the inspect page stays self-contained and
// offline-safe when opened straight from a session directory.
// RaccoonSerif by emhuo (OFL, https://emhuo.itch.io/raccoonserif-pixel-font);
// license copy lives next to the files as OFL-RaccoonSerif.txt.
const FONT_RACCOON_400: &[u8] = include_bytes!("../assets/fonts/raccoonserif-base.woff2");
const FONT_RACCOON_700: &[u8] = include_bytes!("../assets/fonts/raccoonserif-bold.woff2");
const FONT_GAEGU_400: &[u8] = include_bytes!("../assets/fonts/gaegu-400.woff2");
const FONT_GAEGU_700: &[u8] = include_bytes!("../assets/fonts/gaegu-700.woff2");
const FONT_NOTO_SERIF: &[u8] = include_bytes!("../assets/fonts/noto-serif-latin.woff2");
const FONT_NOTO_SERIF_ITALIC: &[u8] =
    include_bytes!("../assets/fonts/noto-serif-latin-italic.woff2");
const FONT_SPACEMONO: &[u8] = include_bytes!("../assets/fonts/spacemono-400.woff2");
const MASCOT: &[u8] = include_bytes!("../assets/mascot.png");

pub fn write_inspect_html(paths: &SessionPaths, metadata: &SessionMetadata) -> Result<()> {
    // Best-effort: older sessions may not have a PR block yet.
    let pr_block = fs::read_to_string(&paths.pr_comment_md).ok();
    let html = render_inspect_html(paths, metadata, pr_block.as_deref());
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
    metadata: &SessionMetadata,
    pr_block: Option<&str>,
) -> String {
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
            "<span class=\"badge\"><b>{:.1}</b>s</span>",
            "<span class=\"badge\">MP4</span>",
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
            let end_frame = step
                .start_frame
                .saturating_add(step_frames.saturating_sub(1));
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
                "frames": format!("{}–{}", step.start_frame, end_frame),
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

    let path_row = |id: &str, label: &str, value: &str| {
        format!(
            concat!(
                "<div class=\"pathrow\"><b>{label}</b>",
                "<span class=\"pathtext\" id=\"{id}\">{value}</span>",
                "<button class=\"copy\" type=\"button\" data-copy=\"{id}\">copy</button></div>",
            ),
            id = id,
            label = label,
            value = encode_text(value),
        )
    };
    let paths_html = format!(
        "{}{}{}",
        path_row("path-session", "session", &paths.root.display().to_string()),
        path_row("path-video", "video", &metadata.video.display().to_string()),
        path_row(
            "path-pr",
            "pr block",
            &paths.pr_comment_md.display().to_string()
        ),
    );

    // Collapsed preview of the exact markdown `sepia pr` will put on the PR.
    let pr_block_html = pr_block
        .map(|block| {
            format!(
                concat!(
                    "<details class=\"prblock\"><summary>Peek at the PR block</summary>",
                    "<div class=\"prbody\">",
                    "<button class=\"copy prcopy\" type=\"button\" data-copy=\"prblock-pre\">copy</button>",
                    "<pre id=\"prblock-pre\">{}</pre>",
                    "</div></details>",
                ),
                encode_text(block.trim_end()),
            )
        })
        .unwrap_or_default();

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
    <div class="porthole"><img class="mascot" src="{mascot}" alt="Sepia the cuttlefish"></div>
    <div class="brandbox">
      <div class="wordmark">Sepia</div>
      <div class="hello">your demo is ready — take a look! ✦</div>
      <div class="badges">{badges}</div>
    </div>
  </header>

  <div class="layout">
    <div class="col">
    <div class="card watch" id="watch">
      <div class="title"><span class="dot"></span>Watch it back</div>
      <div class="screen"><video id="vid" src="{video}" preload="metadata" playsinline></video></div>
      <div class="bar">
        <button class="play" id="play" aria-label="Play or pause">▶</button>
        <div class="rail" id="rail" role="slider" tabindex="0" aria-label="Seek video"
             aria-valuemin="0" aria-valuemax="{total:.1}" aria-valuenow="0">
          <div class="prog" id="prog"></div><div class="knob" id="knob">🫧</div>
        </div>
        <div class="tc" id="tc">0.0 / {total:.1}s</div>
        <button class="ctl" id="speed" type="button" title="Playback speed">1×</button>
        <button class="ctl" id="loop" type="button" title="Loop playback (l)" aria-pressed="false">∞</button>
        <button class="ctl" id="fs" type="button" title="Fullscreen (f)">⛶</button>
      </div>
      <div class="hintline">
        <span class="now" id="now"></span>
        <span class="keys">space ⏯ · ←/→ ±1s · ,/. frame · f fullscreen</span>
      </div>
    </div>

    <div class="card paths-card">
      <div class="title"><span class="dot"></span>Kept safely over here</div>
      <div class="paths">{paths_html}</div>
    </div>
    </div>

    <div class="col">
    <div class="card journey">
      <div class="title"><span class="dot"></span>The little journey · tap to jump</div>
      <div class="steps" id="steps"></div>
    </div>

    <div class="card share-card">
      <div class="title"><span class="dot"></span>Ready for PR</div>
      <p class="desc">{description}</p>
      <ol class="next">
        <li>Watch the recording and jump through the steps.</li>
        <li>If it needs changes, tell the agent what to adjust and rerun.</li>
        <li>If it looks good, attach <code>demo.mp4</code> to the PR:</li>
      </ol>
      <div class="cmds">
        <div class="cmdrow"><code id="cmd-attach">sepia pr --attach</code><button class="copy" type="button" data-copy="cmd-attach">copy</button></div>
        <div class="cmdrow"><code id="cmd-grab">sepia pr --grab</code><button class="copy" type="button" data-copy="cmd-grab">copy</button></div>
      </div>
      {pr_block_html}
    </div>
    </div>
  </div>

  <footer>recorded {created} · made with a <span class="h">♥</span> by <span class="pix">SEPIA</span> the cuttlefish</footer>
</div>

<div class="lightbox" id="lightbox" hidden>
  <figure><img id="lb-img" alt="Step screenshot"><figcaption id="lb-cap"></figcaption></figure>
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
        pr_block_html = pr_block_html,
        created = created,
        consts = consts,
        script = SCRIPT,
    )
}

/// Embedded latin font faces: (family, css weight or range, style, bytes).
const FONTS: &[(&str, &str, &str, &[u8])] = &[
    ("RaccoonSerif", "400", "normal", FONT_RACCOON_400),
    ("RaccoonSerif", "700", "normal", FONT_RACCOON_700),
    ("Gaegu", "400", "normal", FONT_GAEGU_400),
    ("Gaegu", "700", "normal", FONT_GAEGU_700),
    // Noto Serif ships as a variable font: one file per style covers all weights.
    ("Noto Serif", "400 700", "normal", FONT_NOTO_SERIF),
    ("Noto Serif", "400 700", "italic", FONT_NOTO_SERIF_ITALIC),
    ("Space Mono", "400", "normal", FONT_SPACEMONO),
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
        let html = render_inspect_html(&paths, &sample_metadata(), None);
        assert!(html.contains("<video"));
        assert!(html.contains("Ready for PR"));
        assert!(!html.contains("Changes needed:"));
    }

    #[test]
    fn embeds_fonts_and_mascot_for_offline_use() {
        let paths = SessionPaths::from_root(PathBuf::from("/tmp/sepia/demo"));
        let html = render_inspect_html(&paths, &sample_metadata(), None);
        assert!(html.contains("data:font/woff2;base64,"));
        assert!(html.contains("data:image/png;base64,"));
        // No external network requests.
        assert!(!html.contains("fonts.googleapis.com"));
    }

    #[test]
    fn embeds_per_step_seek_data() {
        let paths = SessionPaths::from_root(PathBuf::from("/tmp/sepia/demo"));
        let html = render_inspect_html(&paths, &sample_metadata(), None);
        assert!(html.contains("Scroll package list"));
        assert!(html.contains("steps/step-01-initial.png"));
        assert!(html.contains("const STEPS="));
        // Per-step frame ranges feed the step tooltips.
        assert!(html.contains("2–31"));
    }

    #[test]
    fn exposes_player_controls_and_copy_targets() {
        let paths = SessionPaths::from_root(PathBuf::from("/tmp/sepia/demo"));
        let html = render_inspect_html(&paths, &sample_metadata(), None);
        for id in [
            "id=\"fs\"",
            "id=\"speed\"",
            "id=\"loop\"",
            "id=\"lightbox\"",
        ] {
            assert!(html.contains(id), "missing {id}");
        }
        assert!(html.contains("data-copy=\"path-session\""));
        assert!(html.contains("data-copy=\"cmd-attach\""));
        // No PR block on disk means no embedded preview.
        assert!(!html.contains("Peek at the PR block"));
    }

    #[test]
    fn embeds_pr_block_preview_escaped() {
        let paths = SessionPaths::from_root(PathBuf::from("/tmp/sepia/demo"));
        let block = "<!-- sepia:pr-demo:start -->\n## Sepia Demo\n<!-- sepia:pr-demo:end -->\n";
        let html = render_inspect_html(&paths, &sample_metadata(), Some(block));
        assert!(html.contains("Peek at the PR block"));
        assert!(html.contains("&lt;!-- sepia:pr-demo:start --&gt;"));
        // Raw HTML comments from the block must not survive unescaped.
        assert!(!html.contains("<!-- sepia:pr-demo:start -->"));
    }
}
