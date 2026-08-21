# voxray

Personal macOS CLI for storing, transcribing, and reviewing calls.

Voxray has three commands:

- `voxray inbox` stores one recording from an inbox or explicit path.
- `voxray transcribe` creates one `transcript.txt` and one technical `call.json`.
- `voxray feedback` creates one English `feedback.txt` from a transcript.

Each command runs independently by default. `inbox` and `transcribe` can also
continue through a later stage with `--through`.

## Requirements

- macOS
- MacWhisper CLI (`mw`)
- `ffmpeg`
- an OpenAI API key for `feedback`

## Install

Install the latest GitHub Release:

```bash
curl -fsSL https://raw.githubusercontent.com/RoTorEx/voxray/main/scripts/install.sh | sh
```

The installer verifies the archive checksum, installs the binary as
`~/.x-cli-voxray/bin/voxray`, preserves an existing config, and adds the `bin`
directory to `PATH` in `~/.zshrc` or `~/.bashrc`. Restart the shell, then verify
it:

```bash
voxray --version
```

Later updates are one command:

```bash
voxray update
```

For a development build instead:

```bash
make install-local
```

Verify the installed version with `voxray --version`.

## Configuration

Copy `config.example.toml` beside the installed binary as `config.toml`, then
replace the example paths and profile values. Runtime files are:

```text
~/.x-cli-voxray/config.toml
~/.x-cli-voxray/.env
~/.x-cli-voxray/voxray.log
```

Save the OpenAI API token interactively. Input is hidden and confirmed; the
token is stored as `~/.x-cli-voxray/openai-api-key` with mode `0600`:

```bash
voxray setup-ai-token
```

If a token is already configured, Enter keeps it; explicit confirmation is
required before replacement.

`OPENAI_API_KEY` and the legacy `.env` file are also supported:

```dotenv
OPENAI_API_KEY=...
```

`voxray.log` contains command start/end status and duration, errors, artifact
paths and source actions, transcription paths, and analysis model/module timing.
It never logs API tokens, transcript contents, or model responses. Entries older
than 30 days are removed automatically when a command starts.

Profiles prefill command parameters. Every working profile value has the same
CLI override in interactive and non-interactive operation. Resolution order is:

```text
CLI flag -> selected profile -> [default] -> built-in safe default
```

Interactive operation shows every effective profile value, allows it to be
edited, then shows the input, destination, and command-specific values before
running. Enter accepts the displayed value.

Common overrides include:

```text
--profile
--inbox-dir
--calls-dir
--date-format
--mode file|folder
--call-type
--subject-name
--subject-role
--source-language
--call-goal
--subject-speaker (repeatable)
--module (repeatable)
```

## Inbox

Interactive:

```bash
voxray inbox
```

Non-interactive:

```bash
voxray inbox --profile sales \
  --recording "/path/call.m4a" \
  --name "Atlas IQ — Tomas" \
  --copy --non-interactive
```

`--copy` is the default. `--move` deletes the source only after the target has
been closed and its size verified. Existing recordings are never overwritten.

## Transcribe

Interactive:

```bash
voxray transcribe
```

Non-interactive:

```bash
voxray transcribe --profile sales \
  --recording "/path/Atlas IQ — Tomas.record.m4a" \
  --non-interactive
```

The exact configured model is `whisperkit:openai_whisper-large-v3`. Existing
derived output is preserved unless `--force` is explicit. Ambiguous speaker
mapping is shown interactively; non-interactive output fails immediately with
speaker IDs and samples.

## Feedback

Interactive:

```bash
voxray feedback
```

Non-interactive:

```bash
voxray feedback --profile sales \
  --transcript "/path/Atlas IQ — Tomas.transcript.txt" \
  --target-speaker "Speaker 1" \
  --non-interactive
```

Feedback accepts only a plain transcript. If `call.json` is adjacent, Voxray
uses its structured segments and deterministic metrics. Otherwise it parses the
TXT and marks unavailable metrics as `N/A`. One request handles every active
module with `store=false`; output is English plain text, not Markdown.

## Pipeline

Run from `inbox` through transcription:

```bash
voxray inbox --through transcribe
```

Run the complete pipeline from recording storage through feedback:

```bash
voxray inbox --through feedback
```

Continue from an existing recording through feedback:

```bash
voxray transcribe --recording "/path/call.record.m4a" --through feedback
```

Each completed stage passes its exact output path to the next stage. Pipelines
stop on the first error and keep artifacts already published by successful
stages. Existing derived artifacts are preserved; use the standalone stage with
`--force` when replacement is intentional.

## JSON output

Add `--json` to a non-interactive command. Stdout contains exactly one JSON
object with the effective parameters, artifacts, and result. Pipeline output
also includes its endpoint and per-stage results. Diagnostics go to stderr.

## Files

Folder mode:

```text
Call Name/
  record.<ext>
  audio.m4a       # only for derived audio
  transcript.txt
  feedback.txt
  call.json
```

File mode:

```text
Call Name.record.<ext>
Call Name.audio.m4a
Call Name.transcript.txt
Call Name.feedback.txt
Call Name.call.json
```

## Development

```bash
make build
make check
make install-local
```

## Release

Prepare a release from a clean `main` branch:

```bash
make release
```

Enter the exact `MAJOR.MINOR.PATCH` version when prompted. The command runs the
checks, updates `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md`, creates the
dedicated version-bump commit, and creates the matching annotated `vX.Y.Z` tag.

After reviewing the commit and tag, publish them:

```bash
make release-push
```

The pushed tag builds macOS Apple Silicon and Intel archives in GitHub Actions
and publishes them, their SHA-256 checksums, and `voxray-install.sh` to the
matching GitHub Release. CI runs `make check` on pushes and pull requests;
ordinary pushes to `main` do not publish a release.

## Docs

- [Feedback contract](docs/features/feedback.md)
- `CHANGELOG.md` — unreleased changes
- `AGENTS.md` — contributor workflow router
