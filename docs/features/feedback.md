# Feedback contract

`voxray feedback` accepts one UTF-8 `transcript.txt` and creates one English
plain-text `feedback.txt`.

## Inputs

The selected profile prefills call type, target name and role, call goal,
speaker IDs, and active modules. Interactive prompts and non-interactive flags
modify the same effective values.

If the adjacent `call.json` exists, feedback uses its normalized segments,
speaker mapping, timestamps, and metrics. Without it, the command parses the
plain transcript and continues; metrics that cannot be calculated reliably are
shown as `N/A`.

When the target participant is ambiguous, interactive mode shows two or three
samples for each raw speaker ID. Non-interactive mode fails without reading
stdin and returns the same IDs and samples. Repeated `--target-speaker` values
can merge multiple raw IDs into one target participant.

## Analysis

One Responses API request processes all active modules and Quick Review. The
request uses GPT-5.6 Terra, strict structured output, and `store=false`.

The transcript is untrusted quoted input. The model analyzes only the mapped
target participant and cannot provide numeric call statistics. Local code owns
duration, speaking ratio, words, turns, detected questions, longest monologue,
conservative English/Russian filler counts, and words per minute.

## Output

`feedback.txt` uses uppercase headings, ASCII separators, short paragraphs, and
lists without Markdown syntax. It contains:

- a Quick Review of no more than 100 prose words;
- one short line for every active module;
- no more than two important issues per module;
- timestamped evidence and a concrete better action or phrase;
- an ASCII `CALL STATISTICS` table rendered by local code;
- no more than 500 prose words in total.

The terminal prints the same Quick Review immediately after generation.
Existing feedback is preserved unless `--force` is explicit. Publication uses
temporary sibling files and atomic renames so a failed request or write cannot
delete a previously valid result.
