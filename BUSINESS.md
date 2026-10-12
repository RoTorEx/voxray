# Voxray Business Rules

Voxray is a local CLI for turning call recordings into an organized call
library, transcripts, and optional coaching feedback.

## Actors and outcomes

- The operator selects a configured profile and a recording.
- `inbox` stores the recording under a human-readable call name without
  overwriting an existing call.
- `transcribe` converts audio or video into a readable text transcript only.
- `feedback` evaluates the operator-selected speaker against the selected
  analysis modules.

## Core flows

1. An interactive working command opens the compact launch screen only when no
   profile was supplied. `default` is preselected there, so the operator may
   continue without making a choice. Commands that can continue to a later
   stage also show `through`, preselected to `none`; an explicit `--through`
   preselects that value while the profile is still missing. Supplying
   `--profile` is an explicit launch decision and bypasses the screen entirely;
   omitted `--through` then means `none`. Non-interactive operation never
   prompts. The launch screen keeps all fields and controls in one compact pane
   and uses explicit high-contrast colors so terminal themes cannot make the
   selected value or keyboard help unreadable. Its settings choice either uses
   the selected profile unchanged or reviews every profile value for one-run
   edits; with an explicit profile, `--edit-profile` starts that review directly.
2. Resolve explicit CLI overrides over the selected profile.
3. After profile selection, interactive mode asks only for command-specific
   choices. Profile editing is an explicit one-run action selected on the launch
   screen or preselected via `--edit-profile`.
   Any value explicitly supplied by a CLI flag is final for that run: it is not
   displayed again as an interactive choice and is not requested again during
   profile review or command-specific prompting. Interactive input is reserved
   for values the invocation did not define.
4. Store the source safely; remove it only after a requested move is verified.
   Video input is converted to the canonical `record.m4a`; the original video
   is not stored in the call unless requested. Interactive operation asks with
   the selected profile's `keep_video` value preselected. CLI overrides can
   explicitly keep or discard it for non-interactive operation.
5. Run only the requested stage unless `--through` extends the pipeline.
6. Immediately before feedback, allow one optional per-call context value. It
   may describe the situation or desired outcome, but is never stored in a
   profile. Interactive operation prompts for it; non-interactive operation
   accepts `--context`.

## Native recording handoff

A recorder may select a profile and name before capture and hand off a finalized
recording after it has been saved. Capture and original-media recovery belong to
the recorder. Voxray exposes profile discovery and durable import/transcription
jobs; integration cannot stop capture or delete a recorder original. A repeated
recording UUID resumes the same job and destination. Failed transcription retains
imported audio and stage information for retry. Feedback is a separate choice.
Transcription is a per-recording choice in the native request, independent of
the profile. Import-only requests complete without invoking the transcriber.
See [contract 1](docs/contracts/capture-jobs.md).

## Invariants

- Interactive `inbox` requires an explicit non-empty call name unless `--name`
  was supplied. Only non-interactive operation may fall back to the source
  basename.
- Existing recordings are never overwritten.
- Every stored call has one canonical `record.<audio extension>`. Video input
  therefore produces `record.m4a`, never `audio.m4a` or `record.<video extension>`.
- Human-readable output stays concise by default. Full resolved parameters are
  diagnostic output enabled by `--show-effective`; JSON output remains
  complete.
- Profiles contain reusable storage settings, including whether original video
  is retained, and analysis modules only.
  Transcription language is fixed to automatic detection. Per-call context and
  target-speaker selection belong to the feedback invocation, not the profile.
- Transcription only publishes text from the recording. It never identifies
  participants, calculates coaching metrics, or creates analysis metadata;
  those concerns begin in `feedback` after the transcript exists.
- API tokens and transcript contents are never written to operational logs.
- Configuration is parsed and validated in full before any interactive screen
  or new working command starts. Status and retry use an accepted job snapshot
  even when the current configuration is unavailable. Unknown keys and invalid values are fatal and the
  error identifies the file, field, and reason in operator-readable language.
- Published binaries and the release installer support macOS on Apple Silicon
  only. Intel macOS is not a release target.

## Code map

- `src/main.rs`: command orchestration and interactive behavior.
- `src/inbox.rs`: naming, storage planning, and safe source handling.
- `src/transcribe.rs`, `src/transcript.rs`: transcription and speaker mapping.
- `src/analysis.rs`: feedback generation.
- `src/config.rs`: profiles and configuration resolution.
