# Local usage evidence

The product records meaningful attempts and outcomes for local product analysis
and troubleshooting. A launch or a successful command is not automatically a
successful business outcome. Coverage: recording import, verified transcripts, feedback and durable capture-job stages; command outcomes and elapsed time.

Default destination: `voxray.log` under the product home. Existing home/config/event
root overrides still determine the destination. No remote collector is used.

Records carry timezone-aware timestamps, a schema/version where available,
operation or run correlation, safe counts and categorized results. Source content,
full paths, task/post text, credentials, raw arguments and external response bodies
must not enter the product log. Ordinary CLI output remains unchanged. Version,
help, setup and health actions must be excluded from productive-use counts.

Retention is 30 days with five 2 MiB segments (10 MiB total). Age cleanup runs during writes;
inactive tools cannot grow but may retain old files until next use. Rotation is
serialized across writers. A normal usage-log failure is reported once and does
not change the action's result. Never treat an incomplete run as a successful
operation or infer zero usage from missing logging coverage. Duration measures
elapsed work, not time saved.

Product logs are distinct from business state, cost ledgers and recovery evidence.
Never apply this retention policy to source files, recordings, queues, retry state,
calendar state, Hop ranking history or financial usage records. Existing legacy
logs outside the new stream stop growing under the new build; they are not
silently treated as business state or deleted by broad wildcard cleanup.

Verification uses `make check` and isolated installed commands. Live publication,
paid model calls and collection/calendar mutations retain their existing explicit
operation boundaries. On-demand analysis can read JSON Lines directly; no service,
telemetry platform or aggregate database is required.

The pre-existing text log is preserved once as `voxray.legacy.log`; new records
use structured JSONL. This fixed migration file is outside automatic pruning.
