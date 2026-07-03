---
name: sepia
description: Capture browser change videos as GitHub-friendly MP4s for PR evidence without committing binary assets. Use when an agent needs to script browser interactions, preview and run a capture plan, inspect the result, iterate with the human, and update a PR with a GitHub attachment video URL.
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
7. For a PR dry-run after the human has a GitHub attachment URL, run:

   ```bash
   sepia pr --dry-run --video-url https://github.com/user-attachments/assets/...
   ```

8. After approval, update the PR description inline with one of these flows:

   ```bash
   sepia pr --attach --repo OWNER/REPO --pr NUMBER
   ```

   `--attach` reveals `demo.mp4`, opens the PR page when repo and PR are known, asks the human to upload the file through GitHub's web UI, then uses the pasted `user-attachments` URL in the PR description.

   ```bash
   sepia pr --grab --repo OWNER/REPO --pr NUMBER
   ```

   `--grab` is useful when the human already dragged `demo.mp4` into the PR description and saved it. Sepia reads the PR description, finds the fresh `user-attachments` URL, removes the raw URL line, and wraps the video in the Sepia block. Non-dry-run PR updates require a `https://github.com/user-attachments/assets/...` video URL.

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
