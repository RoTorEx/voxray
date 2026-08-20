# voxray

Personal macOS CLI for storing, transcribing, and reviewing calls.

Voxray has exactly three commands:

- `voxray listen` stores one recording.
- `voxray transcribe` creates one `transcript.txt` and one technical `call.json`.
- `voxray feedback` creates one English `feedback.txt` from a transcript.

There is no combined pipeline command. Each operation is explicit and can run
interactively or without stdin.

## Requirements

- macOS
- MacWhisper CLI (`mw`)
- `ffmpeg`
- an OpenAI API key for `feedback`

## Install locally

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

Set the API key through `OPENAI_API_KEY` or in `.env`:

```dotenv
OPENAI_API_KEY=...
```

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

## Listen

Interactive:

```bash
voxray listen
```

Non-interactive:

```bash
voxray listen --profile sales \
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

## JSON output

Add `--json` to a non-interactive command. Stdout contains exactly one JSON
object with the effective parameters, artifacts, and result. Diagnostics go to
stderr.

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

## Docs

- [Feedback contract](docs/features/feedback.md)
- `CHANGELOG.md` — unreleased changes
- `AGENTS.md` — contributor workflow router
