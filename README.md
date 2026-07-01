# Sepia

Sepia is an agent-native browser demo capture tool for PR evidence. It produces color-correct MP4 demos by driving browser actions, capturing PNG frames, and assembling those frames with FFmpeg. Generated artifacts are written outside target source repositories by default.

## Status

Implemented:

- `sepia run <demo.toml>`
- `sepia inspect [session-dir]`
- `sepia pr --dry-run`
- `sepia skill install/list/remove`
- pixi-build packaging metadata
- timeline granularity for smooth scrolls and readable holds
- automated unit tests and a fake `agent-browser`/`ffmpeg` integration test

Still planned:

- repo-contents fallback provider
- mock-server tests for GitHub upload/comment APIs
- deeper skill install tests with isolated home directories

## Install for development

```bash
pixi install
```

Useful commands:

```bash
pixi run sepia --help
pixi run test
pixi run check
```

## Capture a demo

Start the application you want to record, then run:

```bash
pixi run sepia run examples/basilisk-windowed-browse.toml
```

Sepia writes artifacts under `~/Downloads/sepia/<timestamp-name>/` unless `--output-root` is supplied:

```txt
frames/
steps/
demo.mp4
timeline.json
timeline.md
summary.md
inspect.html
pr-comment.md
```

It also updates:

```txt
~/Downloads/sepia/latest.json
```

## Inspect the latest demo

```bash
pixi run sepia inspect
```

Or inspect a specific session:

```bash
pixi run sepia inspect ~/Downloads/sepia/<session>
```

## Update a PR

Sepia's PR flow is optimized for GitHub's inline video renderer. GitHub only renders videos inline from `https://github.com/user-attachments/assets/...` URLs, so Sepia asks the human to upload the MP4 through the GitHub web UI.

Interactive flow:

```bash
pixi run sepia pr --attach --repo prefix-dev/basilisk --pr 18
```

This will:

1. copy the latest `demo.mp4` path to your clipboard,
2. reveal the file in Finder on macOS,
3. open the PR page when `--repo` and `--pr` are supplied,
4. ask you to paste the resulting GitHub `user-attachments` URL,
5. place the Sepia block at the top of the PR description while preserving the previous description below it.

If you already have the uploaded URL, skip the prompt:

```bash
pixi run sepia pr --repo prefix-dev/basilisk --pr 18 --video-url https://github.com/user-attachments/assets/...
```

Preview the markdown without updating GitHub:

```bash
pixi run sepia pr --dry-run --video-url https://github.com/user-attachments/assets/...
```

If `--repo` or `--pr` are omitted, Sepia uses `gh repo view` and `gh pr view` from the current working directory. Authentication uses `GH_TOKEN`, `GITHUB_TOKEN`, or `gh auth token`.

Sepia searches for this marker and updates the existing block when present, avoiding duplicate PR-description sections:

```md
<!-- sepia:pr-demo:start -->
```

## Demo config

Sepia configs are TOML and should be readable by humans and agents.

```toml
name = "windowed-browse"
description = "Windowed package and advisory browse demo"
url = "http://localhost:3001"
session = "basilisk-demo"

[capture]
output_fps = 24
default_hold_ms = 700
default_action_ms = 400

[[steps]]
name = "Search for zlib"
fill = { selector = "input[role=\"combobox\"]", text = "zlib" }
hold_ms = 1000
screenshot = true

[[steps]]
name = "Scroll package list"
scroll = { selector = ".package-list", pixels = 900 }
duration_ms = 1600
frames = 32
screenshot = true
```

Important timeline fields:

- `output_fps`: final MP4 frame rate.
- `hold_ms`: how long an important state remains visible.
- `duration_ms`: intended duration of animated actions.
- `frames`: screenshot granularity for animated actions like scrolling.

The MP4 is constant-frame-rate, while each step can choose its own capture granularity.

## Agent skill

Install the bundled Sepia skill globally for detected agents:

```bash
pixi run sepia skill install
```

Install for a specific agent id:

```bash
pixi run sepia skill install --agent pi
```

The skill tells agents to capture demos outside target repos, inspect results, iterate with the human, and only update PRs after approval.

## Packaging

Build a local conda package with pixi-build:

```bash
pixi run package-local
```

Publishing to a personal prefix.dev channel is planned after the first accepted real demo.

## Version control

This repo uses Jujutsu:

```bash
jj status
jj diff
```
