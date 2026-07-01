![Sepia banner](assets/banner-rounded.png)

# Sepia

Sepia records short, repeatable browser demos for PRs. You describe the path through your app, Sepia captures a color-correct MP4, and you can add the result to a GitHub PR.

Generated files go to a separate Sepia output directory by default, so demo assets do not end up in the repo you are reviewing.

## Install

Install globally with Pixi:

```bash
pixi global install -c https://prefix.dev/tim -c conda-forge sepia
sepia --help
```

Package page: <https://prefix.dev/channels/tim/packages/sepia>

Want the latest from `main`?

```bash
pixi global install --git https://github.com/tdejager/sepia.git --branch main
```

## Use Sepia with an agent

Most Sepia runs are easiest through an agent:

1. Start the app you want to record.
2. Ask the agent to capture the PR demo.
3. Review the page opened by `sepia inspect`.
4. Approve changes, or ask the agent to adjust the script and rerun.
5. When it looks good, ask the agent to add it to the PR.

Example prompt:

```text
Use Sepia to capture a short browser demo for this PR.

The app is already running at <URL>. Show the main user-facing change from this PR, keep the script readable, and write any generated demo files outside this repository. After recording, open the Sepia inspection page and tell me what to review. Do not update the PR until I approve the demo.
```

If the demo looks good:

```text
The Sepia demo looks good. Update PR <NUMBER> in <OWNER/REPO> with the demo. Use the interactive attach flow if you need me to upload the MP4 to GitHub first.
```

Agents can install the bundled Sepia skill with:

```bash
sepia skill install
```

Agent-facing rules live in [`skills/sepia/SKILL.md`](skills/sepia/SKILL.md).

## Add the demo to a PR

GitHub shows uploaded videos inline when they use a `github.com/user-attachments/assets/...` URL. Sepia helps with that flow:

```bash
sepia pr --attach --repo OWNER/REPO --pr 123
```

This copies the latest `demo.mp4` path, opens the PR page, asks you for the uploaded GitHub video URL, and updates the PR description with a Sepia block.

Already have the uploaded URL?

```bash
sepia pr --repo OWNER/REPO --pr 123 --video-url https://github.com/user-attachments/assets/...
```

Preview the markdown without changing GitHub:

```bash
sepia pr --dry-run --video-url https://github.com/user-attachments/assets/...
```

If `--repo` or `--pr` are omitted, Sepia tries to read them from the current directory using the GitHub CLI. Authentication uses `GH_TOKEN`, `GITHUB_TOKEN`, or `gh auth token`.

<details>
<summary>Run Sepia manually</summary>

1. Start the app you want to record.
2. Write a demo file that opens the app and performs the important actions.
3. Run the demo.
4. Inspect the output.
5. If it looks right, use it in the PR.

Example:

```bash
sepia run examples/basilisk-windowed-browse.toml
sepia inspect
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

Sepia tracks the latest session, so `sepia inspect` can open the most recent run without another path. Use `--output-root <dir>` if you want to choose where sessions are written.

</details>

<details>
<summary>Write a demo file</summary>

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

</details>

<details>
<summary>Inspect a run</summary>

Open the latest run:

```bash
sepia inspect
```

Open a specific run:

```bash
sepia inspect <session-dir>
```

Use this before sharing the video. It is easier to fix the demo and rerun than to explain a confusing PR video.

</details>

<details>
<summary>Development</summary>

Install from a local checkout:

```bash
pixi global install --path .
```

Reinstall after local changes:

```bash
pixi global install --path . --force-reinstall
```

Set up the repo:

```bash
pixi install
```

Useful commands:

```bash
pixi run sepia --help
pixi run test
pixi run check
```

This repo uses Jujutsu:

```bash
jj status
jj diff
```

</details>
