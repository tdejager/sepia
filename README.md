![Sepia banner](assets/banner-rounded.png)

# Sepia

Sepia records short, repeatable browser change videos for PRs. You describe the path through your app, Sepia captures an MP4, and you can add the result to a GitHub PR.

Generated files go to a separate Sepia output directory by default, so recording assets do not end up in the repo you are reviewing.

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
2. Ask the agent to capture the PR video.
3. Review the page opened by `sepia inspect`.
4. Approve changes, or ask the agent to adjust the script and rerun.
5. When it looks good, ask the agent to add it to the PR.

Example prompt:

```text
Use Sepia to capture a short browser change video for this PR.

The app is already running at <URL>. Show the main user-facing change from this PR, keep the script readable, and write any generated recording files outside this repository. After recording, open the Sepia inspection page and tell me what to review. Do not update the PR until I approve the video.
```

If the video looks good:

```text
The Sepia video looks good. Update PR <NUMBER> in <OWNER/REPO> with it. Use the interactive attach flow if you need me to upload the MP4 to GitHub first.
```

Agents can install the bundled Sepia skill with:

```bash
sepia skill install
```

Agent-facing rules live in [`skills/sepia/SKILL.md`](skills/sepia/SKILL.md).

## Add the video to a PR

GitHub shows uploaded videos inline when they use a `github.com/user-attachments/assets/...` URL. Sepia helps with that flow:

```bash
sepia pr --attach --repo OWNER/REPO --pr 123
```

This reveals the latest `demo.mp4` in your file manager, opens the PR page when `--repo` and `--pr` are supplied, asks you for the uploaded GitHub video URL, and updates the PR description with a Sepia block.

Prefer fewer prompts? Drag `demo.mp4` into the PR description yourself, save the PR, then let Sepia find that fresh GitHub attachment URL and wrap it in the Sepia block:

```bash
sepia pr --grab --repo OWNER/REPO --pr 123
```

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
2. Write a Sepia TOML file that opens the app and performs the important actions.
3. Run the recording.
4. Inspect the output.
5. If the video looks right, use it in the PR.

Try it now against a public site — no app or login required. This records the
Hacker News front page, scrolls the ranked list, and opens a numbered result:

```bash
sepia run examples/hacker-news-browse.toml
```

More ready-to-run examples live in [`examples/`](examples):

| Example | What it records |
| --- | --- |
| `hacker-news-browse.toml` | Scroll the HN front page and open a numbered result |
| `wikipedia-search.toml` | Search Wikipedia and browse the cuttlefish article |
| `crates-io-search.toml` | Search crates.io and open the top crate |
| `marginalia-search.toml` | Search the independent Marginalia engine and open a result |

From a checkout you can also run each via its Pixi task: `pixi run run-wikipedia`,
`pixi run run-crates-io`, `pixi run run-marginalia`, `pixi run run-hacker-news`.

In an interactive terminal, `run` opens the inspect page for you when it
finishes (pass `--no-open` to skip it, or run `sepia inspect` yourself later).
Under an agent or in CI it stays quiet and never opens a browser.

Preview the compiled plan as a tree — timings, frames, and pacing — without
recording anything (needs no `agent-browser` or `ffmpeg`):

```bash
sepia run examples/hacker-news-browse.toml --plan
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
<summary>Write a script file</summary>

A Sepia script is TOML. Keep it readable: name the steps after what the reviewer should notice.

Sepia ships a schema at [`schemas/sepia-script.schema.json`](schemas/sepia-script.schema.json). Editors powered by Taplo/Even Better TOML can use it with a top-of-file directive:

```toml
#:schema ./schemas/sepia-script.schema.json
```

```toml
name = "windowed-browse"
description = "Windowed package and advisory browse"
url = "http://localhost:3001"
session = "basilisk-demo"

[capture]
output_fps = 24
default_hold_ms = 700
default_action_ms = 400
show_step_labels = true

[browser]
width = 1440
height = 1000

[[steps]]
name = "Initial packages page"
wait_ms = 1000
hold_ms = 1200
screenshot = true

[[steps]]
name = "Scroll package list"
wait_for = { selector = ".package-list" }
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
- `fill`: fill an input matched by a selector and show a visible focus cue.
- `scroll`: scroll an element matched by a selector.
- `click`: click an element matched by a selector and show a visible click cue.
- `eval`: run JavaScript in the page.

Sepia overlays each step name in the video by default so reviewers know what to notice; set `capture.show_step_labels = false` if you need a clean recording. Use `[browser]` to pin the viewport; Sepia defaults to `1440x1000` for deterministic recordings. Add `wait_for = { selector = "..." }` when a step depends on async UI. Sepia also implicitly waits for `fill`, `scroll`, and `click` targets before acting.

Each step should use at most one action. Prefer `click` over an `eval` click when possible so viewers can see what was clicked. Use `hold_ms`, `duration_ms`, and `frames` when the viewer needs more time to see what happened. Sepia also rejects unknown TOML fields at runtime, so schema validation and `sepia run` agree on the allowed keys. Invalid scripts are reported with source spans that point at the problematic TOML.

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

Use this before sharing the video. It is easier to fix the script and rerun than to explain a confusing PR video.

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
