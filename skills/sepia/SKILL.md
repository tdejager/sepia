---
name: sepia
description: Capture browser change videos as color-correct MP4s for PR evidence without committing binary assets. Use when an agent needs to script browser interactions, preview and run a capture plan, inspect the result, iterate with the human, and update a PR with a GitHub attachment video URL.
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

8. After approval, update the PR description inline:

   ```bash
   sepia pr --attach --repo OWNER/REPO --pr NUMBER
   ```

   The attach flow copies the MP4 path to the clipboard, asks the human to upload it through GitHub's web UI, then uses the pasted `user-attachments` URL in the PR description. Non-dry-run PR updates require a `https://github.com/user-attachments/assets/...` video URL.

## Config guidance

Prefer readable step names and explicit timeline granularity for animations. Allowed step actions are `wait_ms`, `fill`, `scroll`, `click`, and `eval`. Use at most one per step. Prefer `click` over an `eval` click when possible so Sepia can show a visible click cue. If your editor supports Taplo schema directives, point it at Sepia's schema:

```toml
#:schema /path/to/sepia/schemas/sepia-script.schema.json
```

```toml
[capture]
output_fps = 24
default_hold_ms = 700
default_action_ms = 400

[[steps]]
name = "Scroll package list"
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
