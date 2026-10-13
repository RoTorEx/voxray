# Native recording handoff, contract 1

BetterBetterCapture owns capture, final audio processing, original containers and
PCM recovery fragments. Voxray owns profile destinations, import and
transcription. This contract has no capture, stop, delete, move or feedback
operation. Import always retains the caller's finalized recording.

All commands are finite CLI invocations. They print one JSON envelope:
`{"status":"ok","contract_version":1,"data":...}` or
`{"status":"error","contract_version":1,"message":"..."}`. Native commands
never prompt. Integration errors before a job is accepted use the error envelope
and nonzero exit. A failed accepted stage returns the job, including its error
and artifact paths; its stage is `failed`.

- `voxray profiles`: array of `{id,calls_dir}`, including `default`.
- `voxray jobs submit --request /absolute/request.json`: validates and persists
  identity, destination and transcription model, without invoking transcription.
- `voxray jobs status --id UUID`: reads the saved job without loading config.
- `voxray jobs run --id UUID`: runs or retries unfinished import/transcription,
  without loading config or invoking paid feedback.

Request fields (unknown fields rejected):

```json
{
  "recording_id": "12345678-1234-1234-1234-123456789abc",
  "recording": "/absolute/finalized-recording.m4a",
  "recorded_at": "2026-10-09T15:00:00+02:00",
  "profile": "av eni",
  "name": "Engineering Daily",
  "transcribe": true,
  "keep_video": false
}
```

The RFC3339 offset supplies the capture's calendar date, independent of file
mtime or a collision suffix. Call names use the existing sanitization policy.
Native calls use a folder within the selected profile's `calls_dir`, with its
configured date prefix and `record.*`; `transcript.txt` is created only when the
per-recording `transcribe` choice is true. Omitted `transcribe` defaults to true
for existing requests and job files. This choice is independent of the profile
and immutable after submit. Import-only jobs never invoke MacWhisper; their
terminal stage is `stored`, and the transcript path is planned but absent.
The optional `keep_video` choice defaults to false for legacy native jobs.
For video input, true publishes a synchronized `video.<extension>` copy beside
canonical audio; false files only audio. Both choices preserve caller originals
and the private source snapshot. It freezes at submit and survives retries.
Folder mode is deliberate
for this contract; existing CLI file-mode workflows remain available.

Jobs are private directories in `~/.x-cli-voxray/jobs/UUID/` for the installed
layout, or beside a development binary's app home. State has `schema_version`,
`id`, `request`, `stage`, `recording`, `transcript`, `error` and `model`. Stages:
`queued`, `importing`, `transcribing`, `stored` (audio filed), `ready` (transcript
complete), `failed`.

Submit persists identity before work. Run makes and synchronizes a private
`source.*` copy before importing; neither source is automatically deleted. The
caller must retain its finalized recording until explicit deletion. Same UUID
and identical request are idempotent; a changed request is rejected. Each worker
holds an OS file lock, released on process death. State publication uses a synced
temporary file and one atomic rename, followed by directory synchronization.
Corrupt state is rejected and retained. Destination ownership is checked using
`.voxray-recording-id`; an unrelated folder is never adopted or overwritten.
The ownership marker is synchronized in a staging directory beside the target
before an exclusive atomic rename publishes the folder. Retry can therefore
recover reservation interruption without adopting an unrelated empty folder.

A completed import is reused after transcription failure. A transcript published
before interruption is reused. A completed job verifies its artifacts still
exist before returning. MacWhisper has a 7,200-second (two-hour) execution bound
so an unavailable local engine cannot hold a job lock indefinitely. The bridge
allows an additional shutdown margin. This is an operational bound, not a
recording limit: capture is independent, and saved audio remains for retry.

`make check` includes a real-media CLI test with a controlled transcriber:
missing `mw`, retry after config corruption, idempotence, preservation of corrupt
job state, source hashes and ffprobe playback metadata. It does not download
models or make paid API requests.
