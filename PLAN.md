# Sepia plan

Sepia is an agent-native PR demo capture tool. It records browser UI evidence as GitHub-friendly MP4 videos by driving browser actions, capturing PNG frames, and assembling a constant-frame-rate video. Generated artifacts stay outside source repos by default.

## Product principles

- [ ] Keep generated demo assets out of target repositories.
- [ ] Prefer readable abstractions over shell-wrapper sprawl.
- [ ] Prefer readable configs, timelines, inspect pages, PR comments, and error messages.
- [ ] Build small MVP features with seams for future backends.
- [ ] Use `jj` for version-control workflows.
- [ ] Use pixi-build from the start for packaging.

## MVP command surface

- [x] `sepia run demo.toml` captures a scripted browser demo.
- [x] `sepia inspect` opens the latest capture.
- [x] `sepia inspect <session-dir>` opens a specific capture.
- [x] `sepia pr --dry-run` generates local PR markdown without invoking `gh` or requiring credentials.
- [x] `sepia pr [--repo OWNER/REPO] [--pr N] [--output-root DIR]` publishes the latest recording after human approval.
- [x] `sepia skill install` installs the bundled skill globally using the Rust `skill` crate.
- [x] `sepia skill list` lists installed skills.
- [x] `sepia skill remove` removes the bundled skill.

## Pixi and packaging

- [x] Create a separate repo at `/Users/tdejager/development/sepia`.
- [x] Initialize with `jj git init`.
- [x] Add pixi-build Rust package metadata.
- [x] Add `ffmpeg` as runtime dependency.
- [x] Add local build/test/check tasks.
- [x] Verify local conda package build with `pixi publish --target-dir dist/conda`.
- [ ] Publish to a personal prefix.dev channel.

## Architecture

- [x] `BrowserBackend` abstraction.
  - [x] `AgentBrowserBackend` MVP implementation.
  - [ ] Future: native CDP backend.
- [x] `VideoEncoder` abstraction.
  - [x] `FfmpegCliEncoder` MVP implementation.
  - [ ] Future: FFmpeg library binding backend (`ffmpeg-next`, `rsmpeg`, etc.).
- [x] `TimelineCompiler` abstraction.
  - [x] Compile human config steps into explicit frame plans.
  - [x] Allow per-step granularity, duration, holds, and explicit frame counts.
- [x] Native GitHub CLI attachments with `gh` 2.99.0 or newer.
  - [x] Delegate authentication and video uploads to `gh`.
  - [x] Render a local MP4 reference for native rewriting during publication.
  - [x] Propagate failures without retry or fallback, warning about possible partial changes.
- [x] PR body block updater.
  - [x] Find existing Sepia marker block.
  - [x] Update if present, create at top if missing.

## Timeline granularity

- [x] Keep MP4 output constant-frame-rate.
- [x] Let config choose `capture.output_fps`, defaulting to 24.
- [x] Let ordinary actions use readable `hold_ms` defaults.
- [x] Let animated actions, especially scrolls, set `duration_ms` and `frames`.
- [x] Record holds as live footage, so animations keep playing on screen.
- [x] Generate `timeline.json` and human-readable `timeline.md`.

Example intent:

```toml
[capture]
output_fps = 24
default_hold_ms = 700

[[steps]]
name = "Scroll package list"
scroll = { selector = ".package-list", pixels = 900 }
duration_ms = 1600
frames = 32
screenshot = true
```

## Capture MVP

- [x] Parse `demo.toml` into readable typed config.
- [x] Create session under `~/Downloads/sepia/<timestamp-name>/`.
- [x] Drive named `agent-browser` session.
- [x] Pin the browser viewport before navigation.
- [x] Wait for action selectors before fill, scroll, and click.
- [x] Support `wait_for` step preconditions for async UI.
- [x] Record PNG frames from the browser's DevTools screencast, in real time at `output_fps`.
- [x] Capture key screenshots under `steps/`.
- [x] Encode `demo.mp4` using ffmpeg CLI.
- [x] Write `latest.json` pointing at the most recent capture.
- [x] Print final video and inspect paths.

## Inspection UI

- [x] Generate static `inspect.html`.
- [x] Show video at top.
- [x] Show step list, screenshots, frame counts, paths, and config summary.
- [x] Include one copyable `sepia pr` command, gated by human approval.
- [x] `sepia inspect` opens latest capture with platform opener.

## PR workflow

- [x] Generate `pr-comment.md` for every session.
- [x] Persist `session.json` metadata for PR updates.
- [x] Use stable hidden markers:
  - `<!-- sepia:pr-demo:start -->`
  - `<!-- sepia:pr-demo:end -->`
- [x] Resolve the target PR with `gh pr view`, using explicit repo/PR arguments when supplied.
- [x] Require GitHub CLI 2.99.0 or newer, normal `gh` OAuth/PAT authentication, and repository write/push access.
- [x] Publish the latest recording through native GitHub CLI attachments.
- [x] Keep dry-runs local-only, requiring neither `gh` nor credentials.
- [x] Report authentication, unsupported CI credentials, and upload failures directly; inspect the PR before retrying a possibly partial update.
- [x] Upsert the marked Sepia block at the top of the PR description while preserving the previous description.

## Skill workflow

- [x] Bundle `skills/sepia/SKILL.md` into the binary.
- [x] Use the Rust `skill` crate for native global install.
- [x] Default to detected installed agents.
- [x] Support explicit `--agent` filters.
- [x] Use copy mode for embedded skill install.
- [x] Test with temp `HOME`/XDG paths only.
- [x] Suggest `sepia skill install` when detected agents are missing the bundled skill.

## Automated tests

- [x] Unit test config parsing and validation.
- [x] Unit test timeline frame-plan compilation.
- [x] Unit test session naming and `latest.json` read/write.
- [x] Unit test ffmpeg argument generation.
- [x] Unit test inspect HTML rendering.
- [x] Unit test PR markdown rendering and marker detection.
- [x] Unit test skill install target selection.
- [x] Integration test `sepia run` with fake `agent-browser` and fake `ffmpeg` on `PATH`.
- [x] Integration test `sepia inspect` with fake opener.
- [x] Integration test `sepia pr --dry-run` with no GitHub calls.
- [x] `pixi run check` runs fmt, clippy, and tests.

## First acceptance target

- [x] From a Basilisk dev server, run:

```bash
sepia run examples/basilisk-windowed-browse.toml
sepia inspect
```

- [ ] Confirm the MP4 has readable scroll/action pacing and acceptable browser UI colors.
- [x] Run `sepia pr --dry-run` and review generated markdown.
- [ ] Run `sepia pr` only after human approval.
