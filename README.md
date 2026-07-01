# Sepia

![Sepia banner](assets/banner.jpg)

UI changes are hard to review from a PR description alone. Screenshots can miss the flow, and recorded videos are often made by hand, stored in the repo, or hard to reproduce.

Sepia helps you make a repeatable browser demo for a PR. You describe the path through the app in a small TOML file, run Sepia, inspect the result, and optionally add the demo link to a GitHub PR.

It writes generated files to a separate Sepia output directory by default, so demo artifacts do not end up in the project you are reviewing.

## Install

Install globally with Pixi:

```bash
pixi global install -c https://prefix.dev/tim -c conda-forge sepia
sepia --help
```

Package page: <https://prefix.dev/channels/tim/packages/sepia>

To install from a local checkout instead:

```bash
pixi global install --path .
```

Reinstall after local changes:

```bash
pixi global install --path . --force-reinstall
```

For development in this repo:

```bash
pixi install
```

Useful development commands:

```bash
pixi run sepia --help
pixi run test
pixi run check
```

Sepia expects `agent-browser` and `ffmpeg` to be available. The Pixi environment and package dependencies provide both.

## Basic workflow

1. Start the app you want to record.
2. Write a demo file that opens the app and performs the important actions.
3. Run the demo.
4. Inspect the output.
5. If it looks right, use it in the PR.

Example:

```bash
pixi run sepia run examples/basilisk-windowed-browse.toml
pixi run sepia inspect
```

Sepia prints the session directory when the run finishes. It contains:

```txt
<session-dir>/
  demo.mp4
  inspect.html
  summary.md
  timeline.md
  timeline.json
  pr-comment.md
  frames/
  steps/
```

Sepia also tracks the latest session, so `sepia inspect` can open the most recent run without another path. Use `--output-root <dir>` if you want to choose where sessions are written.

## Demo files

A demo file is TOML. Keep it readable: name the steps after what the reviewer should notice.

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
name = "Initial packages page"
wait_ms = 1000
hold_ms = 1200
screenshot = true

[[steps]]
name = "Scroll package list"
scroll = { selector = ".package-list", pixels = 900 }
duration_ms = 1600
frames = 32
screenshot = true

[[steps]]
name = "Search for zlib"
fill = { selector = "input[role=\"combobox\"]", text = "zlib" }
hold_ms = 1000
screenshot = true
```

Supported step actions:

- `wait_ms`: wait before capturing the state.
- `fill`: fill an input matched by a selector.
- `scroll`: scroll an element matched by a selector.
- `eval`: run JavaScript in the page.

Each step should use at most one action. Use `hold_ms`, `duration_ms`, and `frames` when the viewer needs more time to see what happened.

## Inspect a run

Open the latest run:

```bash
pixi run sepia inspect
```

Open a specific run:

```bash
pixi run sepia inspect <session-dir>
```

Use this before sharing the video. It is easier to fix the demo file and rerun than to explain a confusing demo in the PR.

## Add the demo to a PR

GitHub only displays uploaded videos inline when they use a `github.com/user-attachments/assets/...` URL. Sepia therefore asks you to upload the generated MP4 in the browser, then paste the resulting URL.

Interactive flow:

```bash
pixi run sepia pr --attach --repo OWNER/REPO --pr 123
```

This will:

1. copy the latest `demo.mp4` path to your clipboard,
2. reveal the file in Finder on macOS,
3. open the PR page when `--repo` and `--pr` are supplied,
4. ask for the uploaded GitHub video URL,
5. update the PR description with a Sepia block.

If you already have the uploaded URL:

```bash
pixi run sepia pr --repo OWNER/REPO --pr 123 --video-url https://github.com/user-attachments/assets/...
```

Preview the markdown without changing GitHub:

```bash
pixi run sepia pr --dry-run --video-url https://github.com/user-attachments/assets/...
```

If `--repo` or `--pr` are omitted, Sepia tries to read them from the current directory using the GitHub CLI. Authentication uses `GH_TOKEN`, `GITHUB_TOKEN`, or `gh auth token`.

## Instructions for LLM agents

Install the bundled skill if your agent supports skills:

```bash
pixi run sepia skill install
```

Agent rules:

- Do not write generated demo assets into the target source repo.
- Use an output directory outside the repo.
- Use a named browser session.
- Run `sepia inspect` after recording.
- Ask the human what to change if the demo is wrong or unclear.
- Only update a PR after the human approves the inspected result.

After finishing a PR, you can ask an agent something like:

```text
Use Sepia to capture a short browser demo for this PR.

The app is already running at <URL>. Show the main user-facing change from this PR, keep the script readable, and write any generated demo files outside this repository. After recording, open the Sepia inspection page and tell me what to review. Do not update the PR until I approve the demo.
```

If you want the agent to update the PR too:

```text
The Sepia demo looks good. Update PR <NUMBER> in <OWNER/REPO> with the demo. Use the interactive attach flow if you need me to upload the MP4 to GitHub first.
```

## Development notes

This repo uses Jujutsu:

```bash
jj status
jj diff
```

Build and test:

```bash
pixi run check
```
