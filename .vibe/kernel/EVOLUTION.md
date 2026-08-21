# Evolution Contract

Read this file when a user corrects or explains agent behavior, instructions are
unclear or conflicting, the same mistake repeats, a kernel improvement is being
considered, or `make vibe-pull` installs a new kernel version.

## The loop

The kernel and its children improve through a controlled feedback loop:

```text
observe -> repair locally -> propose with evidence -> review and generalize
        -> release -> pull and adopt -> observe again
```

The loop is self-healing, not self-authorizing. Agents preserve knowledge and
propose changes, but the kernel never silently rewrites itself or child product
truth.

## 1. Repair locally first

- Complete the current child task without waiting for a kernel decision.
- Preserve the user's explanation or correction in the correct local source of
  truth during the same task.
- Update code and protecting tests when behavior changed.
- Resolve local contradictions instead of adding another overlapping rule.

This prevents a meta-process from blocking the product work that produced the
evidence.

## 2. Decide whether feedback belongs in the kernel

A kernel proposal is justified when at least one is true:

- the same failure or correction appeared in multiple tasks or projects;
- the lesson applies broadly to agent workflow rather than product behavior;
- one occurrence exposed a high-impact safety, data-loss, security, or release
  risk;
- the user explicitly asks to make the behavior shared.

Keep product rules, domain terminology, architecture, UI behavior, and local
delivery details in the child. Do not generalize merely because a rule could be
worded generically.

## 3. Write a self-contained rule proposal

A substantive rule or proposal must make these clear, in prose or structured
fields:

- **Trigger:** when the rule applies;
- **Action:** what the agent must do;
- **Reason:** which failure it prevents and why the result is worse without it;
- **Evidence:** the correction, incident, repeated pattern, or test behind it;
- **Scope:** where it applies and important exceptions;
- **Verification:** how compliance or benefit can be checked;
- **Removal condition:** what evidence would justify narrowing or deleting it.

Do not paste raw conversations, secrets, private payloads, or unnecessary
project details. A future agent must understand the rule without access to the
originating session.

From a child project, append a proposal to `<KERNEL_SOURCE>/FEEDBACK.md`, where
`<KERNEL_SOURCE>` is the path stored in `.vibe/KERNEL_SOURCE`. Do not edit parent
core files from the child. If filesystem access prevents the append, return a
ready-to-append proposal in the final report; the local task still completes.

## 4. Review in the parent

When changing kernel rules:

1. Read active `FEEDBACK.md` entries and the cited evidence.
2. Search current rules for duplicates, complements, contradictions, and a more
   general rule that already covers the case.
3. Prefer clarifying, merging, or deleting rules over adding another bullet.
4. Accept only the smallest reusable behavioral change.
5. Update kernel business rationale, tests, routing, changelog, and version as
   required.
6. Mark the feedback decision and retain its reasoning in Git history.

A proposal is evidence, not authority. The parent may accept, narrow, reject, or
remove a rule.

## 5. Release, pull, and adopt

- Shared rule changes are distributed only through a released kernel version.
- `make vibe-pull` copies every current `core/*.md`, the kernel `VERSION`, and
  managed routing; it removes obsolete managed runtime files.
- After a version change, read the changed kernel files and the adoption notes
  below before continuing work.
- Apply safe, relevant local instruction or documentation repairs in the same
  task. Do not invent product decisions or perform unrelated migrations.
- Commit the pulled kernel diff and any required local adoption as one explicit
  maintenance task.

## 6. Re-evaluate rules

Later incidents are evidence about existing rules. If a rule is ignored,
misread, duplicated, too broad, or produces worse outcomes, send that evidence
through the same loop. Useful rules may be clarified; unsupported rules should
be narrowed or removed.

Historical LLM sessions may be used during a dedicated audit to reconstruct
provenance and repeated failures. Normal child work must not depend on session
history access, and history must never be copied into instructions wholesale.

## Adoption notes

### 1.1.1

- No project migration is required.
- Run `make vibe-pull` to give child agents the stable interactive release
  invocation contract.
- Existing project release tooling remains valid when `make release` prompts
  for the exact version; agents must no longer encode that version in the shell
  command.

### 1.1.0

- Existing children should run `make vibe-pull` to receive this contract and the
  dynamic runtime sync behavior.
- Apply the local knowledge-capture rule going forward; no bulk rewrite of old
  project documentation is required.
- When an existing rule is next changed, make its trigger and rationale
  self-contained using this contract.
