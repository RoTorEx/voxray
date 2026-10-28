# Personal Google Drive archive

Voxray uploads a completed call after the last requested stage. Each call gets
one folder under the configured Google Drive folder. Different devices can use
the same parent folder or project-specific folders. Uploads do not change sharing:
share the parent in Google Drive; its permissions govern uploaded children.
Recordings from different devices remain independent calls, even with equal names.

## Configuration and overrides

```toml
[default.drive]
enabled = false

[profiles.project.drive]
enabled = true
folder_id = "YOUR_PROJECT_FOLDER_ID"

[google_drive]
folder_id = "YOUR_CALLS_FOLDER_ID"
credentials_file = "/absolute/private/google-drive-oauth.json"
```

Add these tables to existing profiles (which still require inbox_dir and
calls_dir). Drive defaults are off. Destination precedence is: one-run folder
ID override, selected profile's `drive.folder_id`, shared `google_drive.folder_id`,
then the authenticated user's My Drive root (`root`). A shared folder such as
`My Drive/Calls` supplies a common destination without repeating its ID in every
profile. Set a profile route to an existing subfolder ID for project-specific
organization. Each call still gets its own folder beneath the selected destination.
Named profiles do not inherit the default profile's route or upload switch.
The shared destination does not enable uploading by itself. `voxray profiles` advertises
the selected defaults to recording applications. Folder IDs come from the
`/folders/ID` part of a Drive folder URL, not from a local filesystem path.

`--google-drive` enables upload, `--no-google-drive` disables a configured
default, and `--drive-folder ID` overrides the destination and enables upload.
These flags apply to inbox, transcribe, and feedback, including pipelines:

```sh
voxray inbox --profile project --recording /path/call.m4a --name Daily \
  --through feedback --google-drive --non-interactive
voxray transcribe --profile project --recording /path/Call/record.m4a \
  --drive-folder PROJECT_FOLDER_ID --non-interactive
```

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
and shared folder (or My Drive root when no shared folder is configured) before
saving access. Profile-specific destinations are checked when uploading.

The callback listens only on a random 127.0.0.1 port, uses PKCE S256 and a random
state, and times out after 180 seconds to bound an abandoned login while allowing
account selection and consent. Declining consent, partial grants, timeout and
failed verification preserve existing credentials. Login requires browser
interaction and rejects --non-interactive. --json remains available; browser
instructions go to stderr and credentials never appear in output or logs.

Status refreshes access in memory and checks the account and shared destination.
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
files. The current folder-ID workflow requests https://www.googleapis.com/auth/drive,
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
artifacts, including any feedback/metadata, and excludes media from upload.
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
Uploads do not delete remote media uploaded by an earlier run.
