# Voxray Business Rules

Voxray is a local CLI for turning call recordings into an organized call
library, transcripts, and optional coaching feedback.

## Actors and outcomes

- The operator selects a configured profile and a recording.
- `inbox` stores the recording under a human-readable call name without
  overwriting an existing call.
- `transcribe` creates a readable transcript and structured call metadata.
- `feedback` evaluates the configured participant against the selected analysis
  modules.

## Core flows

1. Resolve explicit CLI overrides over the selected profile.
2. In interactive mode, ask only for command-specific choices. Profile editing
   is an explicit one-run action via `--edit-profile`.
3. Store the source safely; remove it only after a requested move is verified.
4. Run only the requested stage unless `--through` extends the pipeline.

## Invariants

- A typed call name determines the folder or file-sidecar basename; the source
  basename is only the empty-input fallback.
- Existing recordings are never overwritten.
- Human-readable output stays concise by default. Full resolved parameters are
  diagnostic output enabled by `--show-effective`; JSON output remains
  complete.
- Profile analysis fields remain available because transcription metadata and
  feedback need call context, evaluated-participant identity, and call goals.
- API tokens and transcript contents are never written to operational logs.

## Code map

- `src/main.rs`: command orchestration and interactive behavior.
- `src/inbox.rs`: naming, storage planning, and safe source handling.
- `src/transcribe.rs`, `src/transcript.rs`: transcription and speaker mapping.
- `src/analysis.rs`: feedback generation.
- `src/config.rs`: profiles and configuration resolution.
