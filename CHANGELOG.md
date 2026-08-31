# Project Changelog

Tracks real product progress.

## [Unreleased]

### Fixed

- Reworked the interactive launch screen with a compact single-pane layout and
  explicit high-contrast colors that remain readable across terminal themes.
- Added an interactive `Settings` choice for reviewing and overriding all
  selected profile values before a run.
- Made video ingestion publish extracted audio as the canonical `record.m4a`
  without retaining the source video by default. Profiles, the interactive
  video prompt, and CLI flags can explicitly keep or discard the original.

## [0.4.1] - 2026-08-30

### Changed

- Removed profile-level call type, participant identity and role, transcription
  language, and call goal. Transcription now always uses automatic language
  detection, while feedback accepts optional per-call context interactively or
  through `--context`.
- Made startup reject unknown configuration keys and invalid values before
  opening the TUI, with errors that identify the exact field and reason.

## [0.4.0] - 2026-08-30

### Changed

- Routed direct Cargo and IDE build output to
  `~/construction_side/voxray/target`.
- Fixed video inbox processing with newer ffmpeg versions by explicitly
  selecting the M4A output container for derived audio temporary files.
- Added a compact single-pane launch screen to every interactive working
  command, with `default` profile and `none` pipeline continuation preselected;
  `--profile` and `--through` now preselect values instead of bypassing it.

## [0.3.4] - 2026-08-24

## [0.3.3] - 2026-08-24

### Changed

- Simplified interactive commands: profile values are edited only with
  `--edit-profile`, and verbose effective-parameter previews are printed only
  with `--show-effective`.
- Made the interactive call-name prompt require an explicit non-empty value so
  it reliably becomes the folder or sidecar basename.
- Reduced transcription to producing `transcript.txt` from media. Participant
  mapping, coaching metrics, and `call.json` now begin in `feedback`.
- Removed the unused `languages`, `transcription.timestamps`, and
  `transcription.speakers` configuration keys and their stale documentation.

## [0.3.2] - 2026-08-22

### Added

- Added `voxray version` to print the installed CLI version without requiring a
  config or API token; `voxray --version` remains supported.

## [0.3.1] - 2026-08-22

### Changed

- Made `setup-config` include a safe dummy profile with every supported profile
  field and documented the complete configuration interface.
- Removed persistent speaker mapping from profiles; speaker IDs are now selected
  for each call interactively or with repeated `--target-speaker` flags.
- Kept feedback within its 500-word contract by counting only rendered text and
  showing one highest-priority issue per module; extra findings remain in
  `call.json`.
- Rounded speaking-time percentages to one decimal and normalized evidence
  quotes so rendered reports contain exactly one quote pair.

## [0.3.0] - 2026-08-22

### Changed

- Added interactive `setup-config`; working commands now require an explicit
  root config, and configurations with analysis modules also require an API
  token at startup.
- Hardened `make release-push` to verify `main` and the current version tag
  before pushing.

## [0.2.3] - 2026-08-22

## [0.2.2] - 2026-08-22

## [0.2.1] - 2026-08-22

### Changed

- Added interactive `setup-ai-token` with hidden confirmation and atomic `0600`
  storage, plus automatic 30-day log retention.
- Renamed the recording-entry command and pipeline stage from `listen` to
  `inbox`; the removed `listen` spelling is no longer accepted.
- Made public GitHub Release installation token-free and documented the
  one-command installer, which stores the binary under
  `~/.x-cli-voxray/bin/voxray` and adds that directory to the user's shell
  `PATH`.

## [0.2.0] - 2026-08-22

### Added

- Added push/PR CI, checksum-verified GitHub Release installation, and
  `voxray update` for future releases.
- Added a guarded `make release` / `make release-push` flow with exact semantic
  version input, dedicated version commits and annotated tags, plus tag-driven
  GitHub Releases for macOS Apple Silicon and Intel archives.
- Added `--through` pipelines so `listen` can continue through `transcribe` or
  `feedback`, and `transcribe` can continue through `feedback` using exact
  artifacts produced by each preceding stage.
- Added matching interactive and non-interactive parameter surfaces: profiles
  prefill every effective value, while command-line flags can override each one.
- Added strict single-object JSON output with effective parameters and artifacts.
- Added both folder and file sidecar layouts with deterministic recording,
  transcript, feedback, and `call.json` names.
- Added conservative English/Russian filler metrics and a locally rendered ASCII
  call-statistics table.

### Changed

- Fixed feedback output language to English and removed the redundant profile
  setting and command-line override.
- Reduced the public product to three independent commands: `listen`,
  `transcribe`, and `feedback`.
- Made `feedback` accept only transcript TXT and produce one English plain-text
  `feedback.txt` from one GPT-5.6 Terra request with `store=false`.
- Made `transcribe` accept a recording directly and publish one readable
  `transcript.txt` plus one structured `call.json`.
- Moved OpenAI key loading to `OPENAI_API_KEY` or the local `.env` file.
- Made existing derived artifacts opt-in replaceable through `--force`, while
  recordings remain collision-safe.

### Removed

- Removed `process`, public stages/rebuild controls, glossary configuration,
  Markdown feedback, split analysis artifacts, and interactive key setup.
