# Personal Google Drive archive

Voxray uploads a completed call after the last requested stage. Each call gets
one folder under the literal mirror of its configured local calls_dir. Devices
with the same relative library paths use the same hierarchy. Uploads do not change sharing:
share the parent in Google Drive; its permissions govern uploaded children.
Recordings from different devices remain independent calls, even with equal names.

## Configuration and mirrored paths

Set one top-level switch before the TOML tables:

```toml
sync_gogle_drive = true

[profiles.project]
inbox_dir = "/Users/you/Documents/Calls Inbox"
calls_dir = "/Users/you/Documents/Calls/26-10 Aveni"
```

The switch defaults to false and applies to every profile. `sync_google_drive`
is accepted as a spelling alias. The existing `calls_dir` is the only routing
source. No Drive ID, extra root, profile route or per-call destination is configured.
For example, the local directory above maps literally to
`My Drive/Calls/26-10 Aveni/<call>`. Names, prefixes, punctuation and deeper nesting
are preserved. Paths below the system Documents directory are relative to that
directory; other home paths are relative to the user home (for example Desktop).
Absolute paths outside home retain their named components beneath My Drive.
Parent traversal is rejected. Existing folders are reused by exact name and parent;
ambiguous duplicate folder names fail visibly. Preview never creates missing folders.

`--google-drive` and `--no-google-drive` override only the switch for one run.
The recorder likewise offers a per-call checkbox and a read-only derived path.
There is no `--drive-folder` or editable ID. Discovery is local and needs no OAuth.
Accepted jobs freeze the local library path; already accepted legacy jobs retain
their saved ID route. Old configuration routes must be migrated rather than ignored.
Credentials remain at the normal private app-home location; an existing optional
`google_drive.credentials_file` still selects a private credential file, not a route.

Upload includes only recognized call artifacts: recording, optional retained
video, transcript.txt, feedback.txt and call.json (with their file-mode names
when applicable). It never recursively uploads unrelated files, logs, credentials,
recovery fragments or private job state. Text artifacts are uploaded as files,
without Google Docs conversion. An inbox-only run may upload media without text.

An import-only native job or `inbox` without a later stage may archive only the
recording. This is an intentional option, not an error. In BetterBetterCapture,
leave Transcript on to archive the processed transcript alongside retained media.
Transcript only is off for every new capture and removes media only after verified
transcription and any requested archive; feedback remains a separate CLI choice.

## Browser login on each computer

```sh
voxray google-auth login
voxray google-auth status
voxray google-auth logout
```

Login opens Google consent in the system browser. Choose the personal account
that can write to the configured Calls folder and review the requested Drive
permission. No gcloud installation, service account, or manual token transfer is
needed. Each device receives its own refresh token. Login verifies the account
and writable My Drive root before
saving access. Mirrored destination folders are checked when uploading.

The callback listens only on a random 127.0.0.1 port, uses PKCE S256 and a random
state, and times out after 180 seconds to bound an abandoned login while allowing
account selection and consent. Declining consent, partial grants, timeout and
failed verification preserve existing credentials. Login requires browser
interaction and rejects --non-interactive. --json remains available; browser
instructions go to stderr and credentials never appear in output or logs.

Status refreshes access in memory and checks the account and writable My Drive root.
Missing credentials report not connected; expired/revoked access or inaccessible
folders return an error. Logout deletes only the configured local credential
file; it never revokes the Google grant, disconnects other devices, or deletes
Drive files. Removing the app in Google Account settings can affect all devices.
Auth commands work before setup-config; an existing configuration is validated.

Credentials use client_id, client_secret and refresh_token in a private 0600 JSON
file. google_drive.credentials_file overrides the default app-home path,
~/.toolbelt/voxray/google-drive-oauth.json for an installed binary. Login replaces
this file atomically after verification. Existing compatible credentials remain
usable. setup-config and ordinary upload-disabled workflows never log in.

### Local Desktop client configuration

Each computer keeps its own Desktop OAuth client JSON at
~/.toolbelt/voxray/google-drive-client.json (google-drive-client.json under app
home for a custom installation). Create/download a Desktop client from the
Voxray project's Google Auth Platform Clients page, save it privately with mode
0600, then run google-auth login. Separate device-named Desktop clients make
machines easy to identify; each device obtains its own user refresh token.
An agent can perform this one-time local setup without transferring secrets
through chat. Status and logout do not require the Desktop client file.

Client configuration stays local. Builds and releases never embed it, read it at
build time, or transfer it to GitHub Actions. No repository secret is required.
Never commit client JSON or user credentials. A fresh installation needs local
Desktop client setup before its first browser login. No gcloud is required.
setup-config does not create either kind of credential.

Personal Drive requires user OAuth: service accounts cannot own personal Drive
files. Literal folder mirroring requests https://www.googleapis.com/auth/drive,
which permits reading and changing the entire Drive. A drive.file grant does not
cover an arbitrary existing folder without a Google Picker authorization flow.
The user must explicitly review and grant this permission in Google's browser
consent. For personal long-running use, configure the app appropriately outside
Testing; Drive refresh tokens in Testing expire after seven days. Public use may
require Google's verification; personal-use exceptions do not remove consent.

Google references: [service-account limits](https://developers.google.com/workspace/drive/api/guides/about-shareddrives),
[desktop OAuth and PKCE](https://developers.google.com/identity/protocols/oauth2/native-app),
[Testing token lifetime](https://support.google.com/cloud/answer/15549945).

## Explicit transcript-only retention

`--transcript-only` is available on inbox (with --through transcribe/feedback)
and transcribe. It is deliberately never a profile default. It retains text
artifacts, including any feedback/metadata, and excludes media from upload. It also removes previously uploaded call media
from active Drive storage (moves it to Drive Trash), after verifying the cloud
transcript. Local and cloud retention must agree.
It conflicts with --keep-video. Media is removed only after all requested
stages and any requested upload succeed. With --copy, inbox preserves the
external source; standalone transcribe deletes the explicitly selected recording.

New transcriptions write a checksum proof alongside the transcript. Removal
requires saved, synchronized, nonempty UTF-8 text and unchanged recording and
transcript checksums. Older transcripts without a proof require --force to
transcribe again. This verifies artifact integrity, **not recognition accuracy or
semantic completeness**. Review recognition quality before choosing this option
for important calls. Files changed during transcription cannot receive a proof.

Native jobs additionally remove their private source copy. The recorder may then
remove its own finalized media, original containers and recovery audio, only
when the completed job reports media_removed=true and the user explicitly chose
Transcript only for that recording. The CLI never deletes recorder-owned files.

## Failure and retry

Missing credentials, denied folder access, network failures, checksum mismatches
and transcription failures preserve local media. Native jobs snapshot the route
and credential path at submit; retry with `voxray jobs run --id UUID` or the
recorder's Retry Voxray button. Adding credentials at the same saved path is
sufficient; changing the current profile cannot redirect an accepted job.

After an ordinary CLI pipeline upload failure, run transcribe on the exact
stored recording with the same profile/Drive flags (and --transcript-only if
selected). Existing text is reused without invoking transcription again. Do not
repeat inbox import for that same call. A feedback-only retry can also upload
all recognized artifacts for its call.

Upload receipts (`.voxray-drive.json` in folder mode, `.<name>.drive.json` in
file mode) persist preallocated remote IDs before creation. Retries recover lost
responses, reuse those IDs and update changed artifacts after verifying name
and parent. Every uploaded file is read back and checked by size and MD5.
An OS file lock prevents concurrent upload of the same local call. Keep receipts
with the call: deleting them may create a second remote folder. Moving, renaming
or trashing remote artifacts causes an explicit error. Changing a call's parent
requires deliberately preserving/removing the old receipt before a second copy.
Explicit Transcript only removes remote media uploaded by an earlier run;
ordinary uploads preserve it.

## Manual synchronization of existing calls

Enable the global sync_gogle_drive switch, then use explicit
commands. They do not transcribe, generate feedback, delete media or start a
background watcher. Preview reads metadata and checksums without creating
folders, receipts or downloads:

```sh
voxray sync push --profile project --dry-run
voxray sync push --profile project
voxray sync pull --profile project --dry-run
voxray sync pull --profile project
voxray sync pull --profile project --call "2026-10-09 Daily"
```

Both folder and file layouts are supported, including calls with transcripts
but no original recording. Transfer includes recognized recording/video files,
transcript, feedback, call metadata and legacy recap text. Unrelated files,
credentials, logs, hidden recording IDs and recovery fragments stay excluded.
The global switch must enable Drive. Each command still traverses only its selected profile.

Push uploads additions and local changes. Pull discovers call folders in the
selected destination and downloads additions and remote changes. Existing
`.voxray-drive.json` / file-layout receipts and locks are shared with pipeline
uploads: repeat runs reuse IDs instead of creating another call folder.
Successful transfers store an MD5 baseline. Older receipts acquire a baseline
only when local and remote content agree; differing untracked versions are
conflicts, never presumed safe to overwrite.

Neither direction infers deletions from missing files. Explicit Transcript only
is persisted as `.voxray-sync.json` in folder layout or `.<call>.sync.json` in
file layout. The version-1 object is `{"version":1,"retention":"transcript_only"}`.
This small system file is synchronized before artifact decisions in either
direction. It is a monotonic deletion intent for recognized call media, not a
credential, analysis manifest or additional routing configuration. It remains
present after removal so an offline device cannot upload or download old audio
again. Ordinary text updates retain the existing three-way checksum baselines
and conflict copies; a missing file alone never authorizes removal.

Cleanup verifies a nonempty local transcript against the cloud size and MD5,
checks media identities and prior baselines, then moves cloud media to Trash and
removes local media. The intent and receipts make interrupted cleanup retryable.
Conflicting transcripts or changed media preserve audio and report a failure.
Previews do not write intent or delete files; `remove_media` reports planned
retention work. Unsupported state versions stop synchronization. Clients older
than this contract must be updated before synchronizing this library: they cannot
be made to honor a state format they do not understand.

 Missing previously verified cloud
objects cause push to stop rather than silently recreating a moved/deleted
record. Pull leaves local-only files in place. Single-sided changes belonging
to the other direction are reported as `remote_changed` or `local_changed`.
Changed names, parents, duplicate cloud names and symbolic-link destinations
are rejected. Downloads verify size/MD5 before atomic publication; uploads
recheck content and use a conditional update when Drive provides an ETag.
Converted native Google documents are not silently exported or substituted for
plain-text artifacts.

If both sides differ from their last shared baseline, the original local file
and the cloud file remain unchanged. A checksum-named
`FILE.conflict-drive-HASH` sibling preserves the downloaded cloud version.
Repeating the operation reuses the same verified conflict copy. The command
reports the path and exits with status 1; `--json` keeps the action list and
`status=conflict`. A conflict preview also exits 1 and creates no copy.

Resolve conflicts deliberately: preserve the local candidate, accept the cloud
version locally and run pull to establish agreement. Then, if desired, replace
that agreed file with the reviewed local/merged candidate and push it. The tool
does not make a content-merge decision for the operator. Pipeline reprocessing
also refuses to overwrite a remotely changed artifact without reconciliation.

Recorder controls and accepted native-job snapshots still override current
profile defaults. Reselect a profile in an already-open recorder to apply newly
configured Drive defaults; changing configuration does not redirect an accepted
job or force an existing checkbox selection.

## Migrating an existing ID-based archive

Finish active captures and accepted jobs before changing the installed recorder,
CLI or local configuration. Set the one top-level sync_gogle_drive switch and
remove old profile drive tables and google_drive.folder_id. Keep existing local
calls_dir values unchanged. The recorder and CLI must both use the mirror contract.

Preserve existing Drive folder IDs when correcting names to match the local tree;
for example rename Aveni to 26-10 Aveni rather than creating a second archive.
If a previously misrouted call must move, inspect its current parent, move that
same folder, read back the new parent, then update only its receipt parent while
preserving file IDs and MD5 baselines. Do not change accepted job snapshots or
remove receipts to force another upload. Resolve ambiguous duplicates explicitly.
Run sync push with --dry-run for each profile after migration; existing artifacts
must agree without upload or conflict. Routing migration alone must not delete cloud files. For historical completed
Transcript only jobs, migrate their explicitly saved retention choice to the
versioned sync state after validating their transcript proof; then run the
current profile's sync push and verify sync pull preview no longer offers audio.
Do not infer that choice from missing local audio. Recorder originals remain
owned by the recorder and are cleaned only through its verified job handoff.
