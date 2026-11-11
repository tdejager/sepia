---
name: sepia
description: Capture browser change videos for PR evidence without committing binary assets. Use when scripting browser interactions, reviewing recordings with the human, or publishing an approved recording to a GitHub PR.
---

# Sepia

Use Sepia when you need browser-based PR evidence as a video.

## Rules

- Never write generated recording assets into the target source repository.
- Prefer output under `~/Downloads/sepia/`.
- Use named browser sessions.
- Inspect the generated video before reporting success.
- Ask the human what should change if the video is not right.
- Only update the PR after the human accepts the inspected output.

## Workflow

1. Start the application under test.
2. Write a readable Sepia TOML script, such as `demo.toml`, outside the target repo or in a safe examples area of the Sepia repo.
3. Preview the capture plan before recording:

   ```bash
   sepia run demo.toml --plan
   ```

4. Run the recording:

   ```bash
   sepia run demo.toml
   ```

   In an interactive terminal Sepia opens the inspect page automatically when the run finishes. Under agents, CI, or piped output it stays quiet.

5. Open the review UI if it did not open automatically:

   ```bash
   sepia inspect
   ```

6. Iterate on the script until the human accepts the result.
7. Preview the latest recording's PR block locally if needed:

   ```bash
   sepia pr --dry-run
   ```

   This needs only the latest recording: it does not invoke `gh`, require credentials, or contact GitHub.

8. After human approval, publish the latest recording and update the PR description:

   ```bash
   sepia pr --repo OWNER/REPO --pr NUMBER
   ```

   Publishing requires GitHub CLI 2.99.0 or newer, normal `gh` authentication with an OAuth token or PAT, and repository write/push access. Sepia uses native GitHub CLI attachments and replaces its marked block at the top of the PR description while preserving the rest. Omitted repo/PR arguments use `gh`'s current-directory context; use `--output-root DIR` to select another recording output root.

   Authentication, unsupported CI credentials, and upload errors propagate without a fallback. An error can occur after an upload or PR update: inspect the PR before deciding whether to retry.

## Config guidance

Prefer readable step names, a pinned viewport, explicit timeline granularity for animations, and selector waits for async UI. Sepia overlays each step name in the video by default so the reviewer knows what to notice; set `capture.show_step_labels = false` only when the overlay would hide the UI being demonstrated. Allowed step actions are `wait_ms`, `fill`, `scroll`, `click`, and `eval`. Use at most one per step. `wait_for = { selector = "..." }` is a precondition, not an action. Sepia also implicitly waits for `fill`, `scroll`, and `click` targets. Prefer `click` over an `eval` click when possible so Sepia can show a visible click cue. If your editor supports Taplo schema directives, point it at Sepia's schema:

```toml
#:schema /path/to/sepia/schemas/sepia-script.schema.json
```

```toml
[capture]
output_fps = 24
default_hold_ms = 700
default_action_ms = 400
show_step_labels = true

[browser]
width = 1440
height = 1000

[[steps]]
name = "Scroll package list"
wait_for = { selector = ".package-list" }
scroll = { selector = ".package-list", pixels = 900 }
duration_ms = 1600
frames = 32
screenshot = true

[[steps]]
name = "Open details"
click = { selector = "a.details" }
hold_ms = 1000
screenshot = true
```
