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

1. Every interactive working command starts with one compact launch screen.
   `default` is the preselected profile, so the operator may continue without
   making a profile choice. Commands that can continue to a later stage also
   show `through`, preselected to `none`. Supplied `--profile` and `--through`
   values preselect their menu items; they never suppress the screen.
   Non-interactive operation never prompts.
2. Resolve explicit CLI overrides over the selected profile.
3. After profile selection, interactive mode asks only for command-specific
   choices. Profile editing is an explicit one-run action via `--edit-profile`.
4. Store the source safely; remove it only after a requested move is verified.
5. Run only the requested stage unless `--through` extends the pipeline.
6. Immediately before feedback, allow one optional per-call context value. It
   may describe the situation or desired outcome, but is never stored in a
   profile. Interactive operation prompts for it; non-interactive operation
   accepts `--context`.

## Invariants

- Interactive `inbox` requires an explicit non-empty call name unless `--name`
  was supplied. Only non-interactive operation may fall back to the source
  basename.
- Existing recordings are never overwritten.
- Human-readable output stays concise by default. Full resolved parameters are
  diagnostic output enabled by `--show-effective`; JSON output remains
  complete.
- Profiles contain reusable storage settings and analysis modules only.
  Transcription language is fixed to automatic detection. Per-call context and
  target-speaker selection belong to the feedback invocation, not the profile.
- Transcription only publishes text from the recording. It never identifies
  participants, calculates coaching metrics, or creates analysis metadata;
  those concerns begin in `feedback` after the transcript exists.
- API tokens and transcript contents are never written to operational logs.
- Configuration is parsed and validated in full before any interactive screen
  or working command starts. Unknown keys and invalid values are fatal and the
  error identifies the file, field, and reason in operator-readable language.

## Code map

- `src/main.rs`: command orchestration and interactive behavior.
- `src/inbox.rs`: naming, storage planning, and safe source handling.
- `src/transcribe.rs`, `src/transcript.rs`: transcription and speaker mapping.
- `src/analysis.rs`: feedback generation.
- `src/config.rs`: profiles and configuration resolution.
