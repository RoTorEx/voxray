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

Profile menus follow named-profile declaration order in the TOML configuration,
with `default` first. Parsing and saving preserve that order; native discovery
returns the same sequence so recorder menus do not apply their own sorting.

## Native recording handoff

A recorder may select a profile and name before capture and hand off a finalized
recording after it has been saved. Capture and original-media recovery belong to
the recorder. Voxray exposes profile discovery and durable import/transcription
jobs; integration cannot stop capture or delete a recorder original. A repeated
recording UUID resumes the same job and destination. Failed transcription retains
imported audio and stage information for retry. Feedback is a separate choice.
Transcription is a per-recording choice in the native request, independent of
the profile. Import-only requests complete without invoking the transcriber. Native requests
may enable `keep_video` to retain video alongside canonical audio; legacy native
requests remain audio-only. Recorder originals are preserved in either case.
See [contract 1](docs/contracts/capture-jobs.md).

## Invariants

- Interactive `inbox` requires an explicit non-empty call name unless `--name`
  was supplied. Only non-interactive operation may fall back to the source
  basename.
- Existing recordings are never overwritten.
- Every media-retaining stored call has one canonical `record.<audio extension>`. Video input
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

## Optional personal Drive archive and text retention

The operator can collect calls from multiple devices in one personal Google
Drive folder, route projects to separate folders, and share folders using
Google Drive. Upload is explicitly optional, defaults off, and runs after the
last requested successful stage. One top-level sync_gogle_drive boolean controls
all profiles; one-run and recorder booleans may override it. The existing calls_dir
is the sole route: mirror local names and nesting literally beneath My Drive,
relative to Documents when inside it, otherwise relative to home or the named
components of an external absolute path. No configured IDs, extra roots or per-call
routes are introduced. Duplicate remote names fail rather than selecting arbitrarily.
Credentials use personal user OAuth,
not service accounts. No account is connected until separately configured.

`voxray google-auth login` connects each device through browser consent, without
gcloud or manual user-token transfer. Each device stores its Desktop client
configuration locally; builds never embed or upload that configuration. Login uses PKCE and a state-validated loopback
callback, verifies the account and shared destination, then atomically replaces
private credentials. A declined, failed, or timed-out login preserves prior access.
`google-auth status` verifies current access; `google-auth logout` removes only
this device's credential file and never revokes access on other devices. Auth
commands can bootstrap before setup-config; an existing config is always validated.

The recording, transcription and feedback stages can run separately or as one CLI
pipeline. Drive archiving follows the last requested successful stage; recording-only
archiving is supported when transcription is explicitly disabled. Native discovery
exposes the derived mirror path and video-retention default to recording applications.

Transcript-only is always a per-call opt-in, never a configuration default.
It means text-only retention everywhere, including media uploaded by earlier runs.
A synced .voxray-sync.json intent prevents stale devices from restoring media.
Only a verified saved transcript and successful requested upload allow media
removal. This verifies file integrity, not speech-recognition completeness.
Voxray never deletes recorder originals; the recorder owns that explicit cleanup
once its completed native job confirms media removal. Default runs preserve media.
Manual `sync push` and `sync pull` transfer existing call artifacts for one
selected profile with the global switch enabled without invoking transcription or analysis.
They support both storage layouts and text-only historical calls. Preview is
read-only; real transfers share pipeline receipts/locks and record content
baselines. Neither direction infers deletions from missing files. Explicit Transcript only
intent is the exception: a versioned per-call sync state is mirrored to every
device and removes recognized call media locally and from active Drive storage
after transcript integrity and synchronization checks. Concurrently edited versions
are preserved as a local original, unchanged cloud object and deterministic
cloud-conflict copy, with an actionable report and nonzero exit. An unknown
baseline never authorizes overwriting differing content. Pipeline uploads must
also protect remote edits. No continuous watcher or automatic merge is added.
See [Google Drive and retention](docs/features/google-drive.md).

## Local product usage evidence

Record meaningful workflow outcomes locally, with bounded storage and no source
content. [Usage logging](docs/usage-logging.md) owns the event and retention
contract. Successful API calls, prepared artifacts and user-reported outcomes
must not be presented as broader business success.
