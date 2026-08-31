# voxray

Personal macOS CLI for storing, transcribing, and reviewing calls.

Voxray has three workflow commands:

- `voxray inbox` stores one recording from an inbox or explicit path.
- `voxray transcribe` creates one `transcript.txt` from audio or video.
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

Verify the installed version with either command:

```bash
voxray version
voxray --version
```

## Configuration

Create a minimal starter configuration interactively:

```bash
voxray setup-config
```

Without `~/.x-cli-voxray/config.toml`, working commands refuse to start and
point to `setup-config`. If a config already exists, Enter keeps it; explicit
confirmation is required before replacement. Runtime files are:

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

When `modules` is non-empty in any configured profile, an OpenAI token is
required before working commands can start. Configurations without analysis
modules can use `inbox` and `transcribe` without a token.

`voxray.log` contains command start/end status and duration, errors, artifact
paths and source actions, transcription paths, and analysis model/module timing.
It never logs API tokens, transcript contents, or model responses. Entries older
than 30 days are removed automatically when a command starts.

Profiles prefill command parameters. Every working profile value has the same
CLI override in interactive and non-interactive operation. Resolution order is:

```text
CLI flag -> selected profile -> [default] -> built-in safe default
```

Every interactive working command starts with one compact launch screen.
`Profile` defaults to `default`; `Through` defaults to `none` when the command
can continue to a later pipeline stage. Arrow keys change either value and
Enter accepts the ready defaults, so neither choice is mandatory. Supplied
`--profile` and `--through` values preselect their menu items instead of
bypassing the screen. The selected profile is then used without walking through
all of its values. Set `Settings` to `review & edit` to print the complete
profile and then override any value for one run; `--edit-profile` preselects
that choice.
`--non-interactive` never prompts and is the mode for scripts and LLM-driven
operation. Interactive `inbox` always asks for a non-empty call name when
`--name` was not supplied; there is no basename default to accept accidentally.

Resolved profile, destination, and command-specific values are kept out of the
normal human-readable output. Add `--show-effective` to print them before each
stage for diagnostics; `--json` always includes them in the structured result.

`setup-config` includes a safe `[profiles.dummy]` containing every profile
field. Its paths are placeholders and its empty `modules` list keeps analysis
disabled until the profile is deliberately configured.

### Configuration reference

`[default]` supplies the fallback profile. `[profiles.<name>]` uses the same
fields for named profiles:

| Option | Meaning | CLI override |
| --- | --- | --- |
| `inbox_dir` | Directory searched for source recordings | `--inbox-dir` |
| `calls_dir` | Destination library for recordings and derived artifacts | `--calls-dir` |
| `date_format` | `strftime` prefix for call names; `""` disables the prefix | `--date-format` |
| `mode` | Storage layout: `"folder"` or `"file"` | `--mode` |
| `keep_video` | Retain original video alongside extracted `record.m4a` | `--keep-video` / `--discard-video` |
| `modules` | Analysis modules: `sales`, `interview`, `english`, `communication`; `[]` disables analysis | `--module` (repeatable) |

`inbox_dir` and `calls_dir` are required in every profile. Other built-in
defaults are: `date_format = "%Y-%m-%d %H-%M"`, `mode = "folder"`,
`keep_video = false`, `modules = []`.
A named profile is independent; it does not inherit
missing required paths from `[default]`.

Voxray validates the complete configuration before opening the interactive
screen or running a command. Unknown keys, unknown modules, malformed model
identifiers, invalid date formats, empty required paths, and invalid analysis
settings stop startup with the file location and a readable explanation.

Transcription always uses automatic language detection. Feedback accepts an
optional per-call `--context`; it is never stored in a profile.

`[transcription]` options:

| Option | Meaning | Default |
| --- | --- | --- |
| `model` | Exact model identifier passed to MacWhisper CLI | `whisperkit:openai_whisper-large-v3` |

`[analysis]` options:

| Option | Meaning | Default |
| --- | --- | --- |
| `model` | OpenAI Responses API model | `gpt-5.6-terra` |
| `reasoning_effort` | Optional model reasoning effort | `"medium"` |
| `api_url` | Responses API endpoint or compatible override | `https://api.openai.com/v1/responses` |

The report language is fixed to English. Legacy `feedback` keys remain readable
as aliases for `modules` and `[analysis]`, but new configs should use the
canonical names above.

Common overrides include:

```text
--profile
--inbox-dir
--calls-dir
--date-format
--mode file|folder
--target-speaker (repeatable, per call)
--module (repeatable)
--context (per feedback run)
```

## Inbox

Interactive:

```bash
voxray inbox
```

Preselect a profile in the menu and override its values for one run when needed:

```bash
voxray inbox --profile sales --edit-profile --show-effective
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
Video input always produces the canonical `record.m4a`; the original video is
not stored by default. Interactive mode asks what to do with it, preselected
from the profile's `keep_video` setting. `--keep-video` and `--discard-video`
override that setting; retained video is stored as `video.<ext>`.

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
mapping, participant identity, metrics, and `call.json` are not part of
transcription. The saved text keeps technical `Speaker 1/2/...` labels.
Personal feedback resolves its target and creates analysis metadata afterward.

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
  --context "Prepare for a procurement objection and agree on the next step" \
  --non-interactive
```

Feedback accepts only a plain transcript. If `call.json` is adjacent, Voxray
uses its structured segments and deterministic metrics. Otherwise it parses the
TXT and marks unavailable metrics as `N/A`. Interactive feedback asks for
optional context immediately before analysis; pressing Enter leaves it empty.
One request handles every active module with `store=false`; output is English
plain text, not Markdown.

## Pipeline

Run from `inbox` through transcription:

```bash
voxray inbox --through transcribe
```

Run the complete pipeline from recording storage through feedback:

```bash
voxray inbox --through feedback --context "Focus on the renewal decision"
```

Continue from an existing recording through feedback:

```bash
voxray transcribe --recording "/path/call.record.m4a" --through feedback \
  --context "Focus on the renewal decision"
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
  record.<audio ext>
  video.<ext>     # optional original video
  transcript.txt
  feedback.txt
  call.json
```

File mode:

```text
Call Name.record.<audio ext>
Call Name.video.<ext>     # optional original video
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
