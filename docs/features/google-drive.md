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
folder_id = "YOUR_GOOGLE_DRIVE_FOLDER_ID"

[profiles.project.drive]
enabled = true
folder_id = "YOUR_PROJECT_FOLDER_ID"

[google_drive]
credentials_file = "/absolute/private/google-drive-oauth.json"
```

Add these tables to existing profiles (which still require inbox_dir and
calls_dir). Drive defaults are off. Profiles have independent routes; named
profiles do not inherit the default profile's route. `voxray profiles` advertises
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

## Credentials: intentionally separate from the MVP setup

No Google login or network request occurs while upload is disabled. Config and
profile discovery work without credentials. No credentials are bundled or
created by setup-config. Browser authorization is not implemented in this MVP.

Personal Google Drive requires **user OAuth**, not a standalone service account:
service accounts have no storage quota and cannot own personal Drive files.
Create a Google Cloud desktop OAuth client, enable Drive API, and obtain user
consent with offline access. Until a folder picker/authorization flow is added,
the credential must grant Drive access to the selected existing parent folder;
a full `https://www.googleapis.com/auth/drive` grant supports arbitrary existing
folder IDs. A `drive.file` grant alone can access only files/folders explicitly
authorized to that application, not every pasted folder ID.

The private JSON file contains `client_id`, `client_secret`, and `refresh_token`.
Store it with mode 0600 and keep it outside repositories. If credentials_file
is omitted, Voxray reads `google-drive-oauth.json` from its app home
(`~/.toolbelt/voxray/` for the installed binary). Access tokens are refreshed in
memory and never saved in jobs, logs, or upload receipts. Each device needs
credentials authorized for the same personal account. Authentication and real
Drive uploads must still be tested after credentials are supplied.

Google references: [service-account limits](https://developers.google.com/workspace/drive/api/guides/about-shareddrives),
[desktop OAuth](https://developers.google.com/identity/protocols/oauth2/native-app),
[file uploads](https://developers.google.com/workspace/drive/api/guides/manage-uploads).

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
