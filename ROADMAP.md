# Product Roadmap

Voxray should turn each completed sales call into one evidence-backed behavior
change for the next call. The roadmap prioritizes observed coaching value over
additional reports or infrastructure.

## Now: validate the existing feedback

Run the current feedback workflow on three recent, meaningful sales calls of
different types, ideally discovery, demo, and follow-up or negotiation. Do not
change the prompt or feedback contract before these runs are reviewed.

For each call:

- read the Quick Review and confirm its meaning is clear within 30 seconds;
- verify the two timestamps most important to its conclusion;
- decide whether the main failure was identified correctly;
- decide whether the proposed next-call action or phrase is useful;
- confirm that seller behavior is distinguished from product or process
  limitations.

Exit condition:

- three fresh `feedback.txt` files have been reviewed;
- the observed weaknesses are recorded from evidence, not assumptions;
- one shared focus is selected for the next calls, with no parallel coaching
  goals.

This phase is operational validation. It does not authorize product-code or
prompt changes by itself.

## Next: fix only demonstrated friction

Use the three-call evidence to choose the smallest necessary change. Likely
candidates are:

- make the existing full sales pipeline easier to invoke when feedback is
  useful but routinely skipped;
- persist target-speaker selection before the AI request so a failed request
  can resume without asking again;
- handle small Quick Review length overruns locally instead of discarding an
  otherwise valid analysis;
- revise the feedback contract only when the real runs show that its diagnosis
  or next-call action is not useful.

Preserve these invariants:

- one command can complete `inbox -> transcribe -> feedback`;
- one provider attempt is made per invocation, and a successful feedback result
  is never requested again without explicit force;
- deterministic metrics remain code-owned;
- user-facing artifacts remain `transcript.txt`, `feedback.txt`, and, if later
  needed, `progress.txt`;
- `call.json` remains the internal machine contract;
- transcript and previous successful feedback artifacts survive later-stage
  failures.

Exit condition: the demonstrated blocker is removed and the same three-call
workflow succeeds without duplicated work or lost artifacts.

## Later: close the learning loop after five calls

Only after at least five real calls have used one active focus:

- store a stable `next_focus`, category, evidence, creation time, and rubric
  version in `call.json`;
- expose the latest successful focus read-only to Sales Copilot as `MY FOCUS`;
- assess the previous focus on the next applicable call as applied, partially
  applied, not observed, or not applicable, with timestamped evidence;
- consider a five-call `progress.txt` review that makes one decision: keep the
  current focus or replace it.

Do not replace a focus because of one unsuccessful call. Keep only one primary
focus active at a time.

## Deliberate deferrals

Do not build real-time coaching, a GUI or dashboard, a new database, vector
search, RAG, multi-agent feedback, multiple provider adapters, CRM or email
mutations, cloud sync, voice fingerprinting, or a generic workflow engine.

When the three-call workflow is reliable and useful, stop development and use
Voxray daily for at least two weeks before expanding the product.
