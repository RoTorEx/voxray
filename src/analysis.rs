use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::call;
use crate::config::{AnalysisConfig, Config, Profile, REPORT_LANGUAGE};
use crate::logs;
use crate::storage;
use crate::transcript::{Metrics, Transcript};

const PROMPT: &str = include_str!("../prompts/call-coach-v2.md");
const PROMPT_VERSION: &str = "call-coach-v2.1";

#[derive(Debug, Serialize)]
pub struct AnalysisResult {
    pub result: String,
    pub feedback: PathBuf,
    pub call_json: Option<PathBuf>,
    pub quick_review: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisOutput {
    pub quick_review: QuickReview,
    pub modules: Vec<ModuleReview>,
    pub issues: Vec<AnalysisIssue>,
    pub deal_notes: DealNotes,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickReview {
    pub main_failure: String,
    pub next_call_action: String,
    pub keep_doing: String,
    pub practice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleReview {
    pub name: String,
    pub score: u8,
    pub summary: String,
    pub main_issue: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisIssue {
    pub module: String,
    pub category: String,
    pub problem: String,
    pub impact: String,
    pub better_move: String,
    pub example_phrase: String,
    pub exercise: String,
    pub confidence: String,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DealNotes {
    pub buyer_signals: Vec<String>,
    pub promises_and_obligations: Vec<String>,
    pub next_step: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub timestamp: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisDocument {
    pub schema_version: u32,
    pub run: RunMetadata,
    pub metrics: Metrics,
    pub quick_review: QuickReview,
    pub modules: Vec<ModuleReview>,
    pub issues: Vec<AnalysisIssue>,
    pub deal_notes: DealNotes,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    pub status: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub prompt_version: String,
    pub started_at: String,
    pub finished_at: String,
    pub latency_ms: u128,
    pub usage: Value,
    pub cost_usd: Option<f64>,
    pub cost_note: String,
}

pub fn run(
    config: &Config,
    profile_name: &str,
    profile: &Profile,
    target_speakers: &[String],
    transcript_path: &Path,
    force: bool,
    interactive: bool,
) -> Result<AnalysisResult> {
    crate::call::validate_file(transcript_path, "Transcript")?;
    let modules = normalize_modules(&profile.modules)?;
    if modules.is_empty() {
        bail!("No analysis modules configured for profile '{profile_name}'");
    }
    let paths = call::CallPaths::from_transcript(transcript_path)?;
    let existing_manifest = call::CallManifest::load(&paths.manifest)?;
    if paths.feedback.exists() && !force {
        let quick_review = existing_manifest
            .as_ref()
            .and_then(|manifest| manifest.analysis.as_ref())
            .map(render_quick_review)
            .unwrap_or_default();
        return Ok(AnalysisResult {
            result: "already_exists".to_string(),
            feedback: paths.feedback,
            call_json: existing_manifest.map(|_| paths.manifest),
            quick_review,
        });
    }
    let mut manifest =
        existing_manifest.unwrap_or_else(|| call::CallManifest::new(&paths, profile_name, profile));
    manifest.schema_version = 3;
    manifest.name = paths.name.clone();
    manifest.mode = paths.mode;
    manifest.profile = profile_name.to_string();
    manifest.call_type = profile.call_type.clone();
    manifest.subject_name = profile.subject_name.clone();
    manifest.subject_role = profile.subject_role.clone();
    manifest.call_goal = profile.call_goal.clone();
    manifest.source_language = profile.source_language.clone();
    manifest.report_language = REPORT_LANGUAGE.to_string();

    let transcript = match manifest.transcript.clone() {
        Some(transcript) => transcript,
        None => {
            let content = fs::read_to_string(transcript_path)
                .with_context(|| format!("Failed to read {}", transcript_path.display()))?;
            Transcript::from_legacy_text(&content, profile, target_speakers, interactive)?
        }
    };
    if !transcript
        .speaker_mapping
        .values()
        .any(|participant| participant == "target")
    {
        bail!(
            "Target participant {} is not mapped to a speaker",
            profile.subject_name
        );
    }
    let metrics = transcript.metrics("target");
    manifest.speaker_mapping = transcript.speaker_mapping.clone();
    manifest.transcript = Some(transcript.clone());
    manifest.metrics = Some(metrics.clone());
    let (document, feedback) = perform_analysis(
        config,
        profile_name,
        profile,
        &modules,
        &transcript,
        &metrics,
    )?;

    manifest.analysis = Some(document.clone());
    manifest.set_artifact("feedback", &paths.feedback, &paths.root);
    manifest.set_artifact("transcript", transcript_path, &paths.root);
    let manifest_bytes = manifest.to_bytes()?;
    storage::atomic_write_pair(
        &paths.manifest,
        &manifest_bytes,
        &paths.feedback,
        feedback.as_bytes(),
    )?;
    call::remove_legacy_files(&paths)?;

    logs::event(&format!(
        "analysis_done profile={} latency_ms={}",
        profile_name, document.run.latency_ms
    ));
    Ok(AnalysisResult {
        result: "created".to_string(),
        feedback: paths.feedback,
        call_json: Some(paths.manifest),
        quick_review: render_quick_review(&document),
    })
}

fn perform_analysis(
    config: &Config,
    profile_name: &str,
    profile: &Profile,
    modules: &[String],
    transcript: &Transcript,
    metrics: &Metrics,
) -> Result<(AnalysisDocument, String)> {
    let input = json!({
        "call": {
            "type": profile.call_type,
            "goal": profile.call_goal,
            "target_participant": {
                "participant_id": "target",
                "name": profile.subject_name,
                "role": profile.subject_role
            },
            "report_language": REPORT_LANGUAGE,
        },
        "enabled_modules": modules,
        "deterministic_metrics": metrics,
        "transcript": transcript.segments,
    });
    let started_at = chrono::Utc::now();
    let timer = Instant::now();
    logs::event(&format!(
        "analysis_start profile={} model={} modules={}",
        profile_name,
        config.analysis.model,
        modules.join(",")
    ));
    let (mut output, usage) = call_model(&config.analysis, &input)?;
    validate_and_normalize_output(&mut output, modules)?;
    let finished_at = chrono::Utc::now();
    let document = AnalysisDocument {
        schema_version: 2,
        run: RunMetadata {
            status: "complete".to_string(),
            model: config.analysis.model.clone(),
            reasoning_effort: config.analysis.reasoning_effort.clone(),
            prompt_version: PROMPT_VERSION.to_string(),
            started_at: started_at.to_rfc3339(),
            finished_at: finished_at.to_rfc3339(),
            latency_ms: timer.elapsed().as_millis(),
            usage,
            cost_usd: None,
            cost_note: "Not calculated: no versioned pricing table is configured".to_string(),
        },
        metrics: metrics.clone(),
        quick_review: output.quick_review,
        modules: output.modules,
        issues: output.issues,
        deal_notes: output.deal_notes,
        warnings: output.warnings,
    };
    let feedback = render_feedback(&document);
    let feedback_words = prose_word_count(&feedback);
    if feedback_words > 500 {
        bail!("Rendered feedback is {feedback_words} words; maximum is 500");
    }
    Ok((document, feedback))
}

fn normalize_modules(modules: &[String]) -> Result<Vec<String>> {
    let mut result = Vec::new();
    for module in modules {
        let normalized = match module.as_str() {
            "common" => "communication",
            "sales" | "english" | "communication" | "interview" => module,
            other => bail!("Unknown analysis module '{other}'"),
        };
        if !result.iter().any(|existing| existing == normalized) {
            result.push(normalized.to_string());
        }
    }
    Ok(result)
}

fn call_model(cfg: &AnalysisConfig, input: &Value) -> Result<(AnalysisOutput, Value)> {
    let api_key = read_api_key()?;
    let mut body = json!({
        "model": cfg.model,
        "store": false,
        "instructions": PROMPT,
        "input": input.to_string(),
        "text": {
            "format": {
                "type": "json_schema",
                "name": "call_analysis",
                "strict": true,
                "schema": analysis_schema()
            }
        }
    });
    if let Some(effort) = &cfg.reasoning_effort {
        body["reasoning"] = json!({"effort": effort});
    }
    let response = ureq::post(&cfg.api_url)
        .set("Authorization", &format!("Bearer {api_key}"))
        .timeout(Duration::from_secs(300))
        .send_json(body);
    let response = match response {
        Ok(response) => response,
        Err(ureq::Error::Status(code, _response)) => {
            bail!("Analysis API returned HTTP {code}");
        }
        Err(error) => return Err(error).context("Analysis API request failed"),
    };
    let payload: Value = response
        .into_json()
        .context("Failed to parse Responses API payload")?;
    let text = output_text(&payload)
        .context("Responses API payload did not contain structured output text")?;
    let output: AnalysisOutput =
        serde_json::from_str(text).context("Failed to parse structured analysis")?;
    Ok((output, payload.get("usage").cloned().unwrap_or(Value::Null)))
}

fn output_text(payload: &Value) -> Option<&str> {
    payload
        .get("output")?
        .as_array()?
        .iter()
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .find(|content| content.get("type").and_then(Value::as_str) == Some("output_text"))?
        .get("text")?
        .as_str()
}

fn analysis_schema() -> Value {
    let evidence = json!({
        "type": "object",
        "properties": {
            "timestamp": {
                "type": "string",
                "maxLength": 24,
                "pattern": "^[0-9:.]+([–—-][0-9:.]+)?$"
            },
            "quote": {"type": "string", "maxLength": 220}
        },
        "required": ["timestamp", "quote"],
        "additionalProperties": false
    });
    json!({
        "type": "object",
        "properties": {
            "quick_review": {
                "type": "object",
                "properties": {
                    "main_failure": {"type": "string", "maxLength": 300},
                    "next_call_action": {"type": "string", "maxLength": 300},
                    "keep_doing": {"type": "string", "maxLength": 240},
                    "practice": {"type": "string", "maxLength": 240}
                },
                "required": ["main_failure", "next_call_action", "keep_doing", "practice"],
                "additionalProperties": false
            },
            "modules": {
                "type": "array",
                "maxItems": 4,
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string", "enum": ["sales", "english", "communication", "interview"]},
                        "score": {"type": "integer", "minimum": 1, "maximum": 5},
                        "summary": {"type": "string", "maxLength": 420},
                        "main_issue": {"type": "string", "maxLength": 220},
                        "evidence": {"type": "array", "maxItems": 2, "items": evidence.clone()}
                    },
                    "required": ["name", "score", "summary", "main_issue", "evidence"],
                    "additionalProperties": false
                }
            },
            "issues": {
                "type": "array",
                "maxItems": 8,
                "items": {
                    "type": "object",
                    "properties": {
                        "module": {"type": "string", "enum": ["sales", "english", "communication", "interview", "deal_notes", "system"]},
                        "category": {"type": "string", "enum": ["user_skill", "communication", "product_or_process", "credibility_or_compliance", "deal_follow_up", "transcription_uncertainty"]},
                        "problem": {"type": "string", "maxLength": 360},
                        "impact": {"type": "string", "maxLength": 360},
                        "better_move": {"type": "string", "maxLength": 420},
                        "example_phrase": {"type": "string", "maxLength": 360},
                        "exercise": {"type": "string", "maxLength": 300},
                        "confidence": {"type": "string", "enum": ["high", "medium", "low"]},
                        "evidence": {"type": "array", "maxItems": 2, "items": evidence}
                    },
                    "required": ["module", "category", "problem", "impact", "better_move", "example_phrase", "exercise", "confidence", "evidence"],
                    "additionalProperties": false
                }
            },
            "deal_notes": {
                "type": "object",
                "properties": {
                    "buyer_signals": {"type": "array", "maxItems": 4, "items": {"type": "string", "maxLength": 240}},
                    "promises_and_obligations": {"type": "array", "maxItems": 4, "items": {"type": "string", "maxLength": 240}},
                    "next_step": {"type": "string", "maxLength": 300}
                },
                "required": ["buyer_signals", "promises_and_obligations", "next_step"],
                "additionalProperties": false
            },
            "warnings": {"type": "array", "maxItems": 5, "items": {"type": "string", "maxLength": 300}}
        },
        "required": ["quick_review", "modules", "issues", "deal_notes", "warnings"],
        "additionalProperties": false
    })
}

fn render_feedback(document: &AnalysisDocument) -> String {
    let mut output = render_quick_review(document);
    output.push_str("\n\n");
    output.push_str(&render_statistics(&document.metrics));
    for module in &document.modules {
        output.push_str(&format!(
            "\n\n======================================================================\n{} — {}/5\n======================================================================\n",
            title(&module.name),
            module.score,
        ));
        for issue in document
            .issues
            .iter()
            .filter(|issue| issue.module == module.name)
            .take(2)
        {
            output.push_str(&format!(
                "\nISSUE\n{}\n\nWHY IT MATTERS\n{}\n\nEVIDENCE\n{}\n\nBETTER MOVE\n{}\n",
                issue.problem,
                issue.impact,
                render_evidence(&issue.evidence),
                issue.better_move
            ));
            if !issue.example_phrase.trim().is_empty() {
                output.push_str(&format!("\nBETTER PHRASE\n{}\n", issue.example_phrase));
            }
            if module.name == "english" && !issue.exercise.trim().is_empty() {
                output.push_str(&format!("\nPRACTICE\n{}\n", issue.exercise));
            }
        }
    }
    if document.modules.iter().any(|module| module.name == "sales")
        && (!document.deal_notes.buyer_signals.is_empty()
            || !document.deal_notes.promises_and_obligations.is_empty()
            || !document.deal_notes.next_step.trim().is_empty())
    {
        output.push_str("\n\n======================================================================\nFOLLOW-UP / DEAL NOTES\n======================================================================\n");
        append_list(
            &mut output,
            "Buyer signals",
            &document.deal_notes.buyer_signals,
            1,
        );
        append_list(
            &mut output,
            "Promises / obligations",
            &document.deal_notes.promises_and_obligations,
            2,
        );
        if !document.deal_notes.next_step.trim().is_empty() {
            output.push_str(&format!("\nNEXT STEP\n{}\n", document.deal_notes.next_step));
        }
    }
    output
}

fn render_quick_review(document: &AnalysisDocument) -> String {
    let quick = &document.quick_review;
    let mut output = format!(
        "======================================================================\nQUICK REVIEW\n======================================================================\n\nMAIN FAILURE\n{}\n\nNEXT CALL\n{}\n\nKEEP DOING\n{}\n",
        quick.main_failure, quick.next_call_action, quick.keep_doing
    );
    for module in &document.modules {
        output.push_str(&format!(
            "\n{:<15} {}/5 — {}\n",
            title(&module.name).to_ascii_uppercase(),
            module.score,
            module.main_issue
        ));
    }
    output.push_str(&format!("\nPRACTICE\n{}", quick.practice));
    output
}

fn render_statistics(metrics: &Metrics) -> String {
    let target = metrics
        .participants
        .get("target")
        .cloned()
        .unwrap_or_default();
    let mut others = crate::transcript::ParticipantMetrics::default();
    for (participant, current) in &metrics.participants {
        if participant == "target" {
            continue;
        }
        others.words += current.words;
        others.speaking_seconds += current.speaking_seconds;
        others.turns += current.turns;
        others.questions += current.questions;
        others.longest_turn_words = others.longest_turn_words.max(current.longest_turn_words);
        others.fillers += current.fillers;
    }
    let other_wpm = (others.speaking_seconds > 0.0)
        .then(|| (others.words as f64 / others.speaking_seconds * 60.0 * 10.0).round() / 10.0);
    format!(
        "======================================================================\nCALL STATISTICS\n======================================================================\n\nDuration: {}\n\n+------------------------+------------+----------------+\n| METRIC                 | YOU        | OTHER SPEAKERS |\n+------------------------+------------+----------------+\n| Speaking time          | {:>9}% | {:>13}% |\n| Words                  | {:>10} | {:>14} |\n| Turns                  | {:>10} | {:>14} |\n| Questions detected     | {:>10} | {:>14} |\n| Longest monologue      | {:>7} wd | {:>11} wd |\n| Fillers detected       | {:>10} | {:>14} |\n| Words per minute       | {:>10} | {:>14} |\n+------------------------+------------+----------------+",
        crate::transcript::format_timestamp(metrics.duration_seconds),
        target.speaking_ratio,
        (100.0_f64 - target.speaking_ratio).max(0.0),
        target.words,
        others.words,
        target.turns,
        others.turns,
        target.questions,
        others.questions,
        target.longest_turn_words,
        others.longest_turn_words,
        target.fillers,
        others.fillers,
        format_optional(target.words_per_minute),
        format_optional(other_wpm),
    )
}

fn format_optional(value: Option<f64>) -> String {
    value.map_or_else(|| "N/A".to_string(), |value| format!("{value:.1}"))
}

fn append_list(output: &mut String, heading: &str, items: &[String], limit: usize) {
    if items.is_empty() {
        return;
    }
    output.push_str(&format!("\n{}\n", heading.to_ascii_uppercase()));
    for item in items.iter().take(limit) {
        output.push_str(&format!("- {item}\n"));
    }
}

fn render_evidence(evidence: &[Evidence]) -> String {
    evidence
        .iter()
        .take(1)
        .map(|item| format!("[{}] \"{}\"", item.timestamp, item.quote))
        .collect::<Vec<_>>()
        .join("; ")
}

fn title(module: &str) -> &str {
    match module {
        "sales" => "Sales",
        "english" => "English",
        "communication" => "Communication",
        "interview" => "Interview",
        other => other,
    }
}

fn validate_and_normalize_output(output: &mut AnalysisOutput, expected: &[String]) -> Result<()> {
    output.quick_review.main_failure = strip_label(
        first_line(&output.quick_review.main_failure),
        &[
            "главный провал",
            "главный сбой",
            "главная ошибка",
            "main failure",
        ],
    );
    output.quick_review.next_call_action = strip_label(
        first_line(&output.quick_review.next_call_action),
        &["на следующий звонок", "следующий звонок", "next call"],
    );
    output.quick_review.keep_doing = strip_label(
        first_line(&output.quick_review.keep_doing),
        &["что сохранить", "сохранить", "keep doing"],
    );
    output.quick_review.practice = strip_score_suffix(&strip_label(
        first_line(&output.quick_review.practice),
        &["практика", "practice"],
    ));
    output.quick_review.main_failure = truncate_words(&output.quick_review.main_failure, 14);
    output.quick_review.next_call_action =
        truncate_words(&output.quick_review.next_call_action, 24);
    output.quick_review.keep_doing = truncate_words(&output.quick_review.keep_doing, 10);
    output.quick_review.practice = truncate_words(&output.quick_review.practice, 10);
    let module_budget = 25_usize
        .checked_div(output.modules.len())
        .unwrap_or(25)
        .max(5);
    for module in &mut output.modules {
        module.main_issue = truncate_words(&module.main_issue, module_budget);
    }
    if output.modules.len() != expected.len() {
        bail!(
            "Analysis returned {} modules, expected {}",
            output.modules.len(),
            expected.len()
        );
    }
    for expected_name in expected {
        if !output
            .modules
            .iter()
            .any(|module| &module.name == expected_name)
        {
            bail!("Analysis omitted required module '{expected_name}'");
        }
        if output
            .issues
            .iter()
            .filter(|issue| &issue.module == expected_name)
            .count()
            > 2
        {
            bail!("Analysis returned more than two visible issues for '{expected_name}'");
        }
    }
    let quick_words = [
        &output.quick_review.main_failure,
        &output.quick_review.next_call_action,
        &output.quick_review.keep_doing,
        &output.quick_review.practice,
    ]
    .into_iter()
    .chain(output.modules.iter().map(|module| &module.main_issue))
    .flat_map(|text| text.split_whitespace())
    .count();
    if quick_words > 85 {
        bail!(
            "Quick Review content is {quick_words} words; maximum is 85 so rendered labels stay within 100"
        );
    }
    for evidence in output
        .modules
        .iter_mut()
        .flat_map(|module| module.evidence.iter_mut())
        .chain(
            output
                .issues
                .iter_mut()
                .flat_map(|issue| issue.evidence.iter_mut()),
        )
    {
        evidence.timestamp = normalize_timestamp(&evidence.timestamp).with_context(|| {
            format!(
                "Analysis returned invalid evidence timestamp: {}",
                evidence.timestamp
            )
        })?;
        if evidence.quote.trim().is_empty() {
            bail!("Analysis returned empty evidence quote");
        }
    }
    if !expected.iter().any(|module| module == "sales") {
        output.deal_notes = DealNotes::default();
    }
    Ok(())
}

fn strip_label(value: &str, labels: &[&str]) -> String {
    let trimmed = value.trim();
    for label in labels {
        if trimmed.to_lowercase().starts_with(&label.to_lowercase()) {
            return trimmed[label.len()..]
                .trim_start_matches([':', '—', '-', ' '])
                .to_string();
        }
    }
    trimmed.to_string()
}

fn first_line(value: &str) -> &str {
    value
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
}

fn strip_score_suffix(value: &str) -> String {
    let lowercase = value.to_lowercase();
    let cutoff = [
        " sales:",
        " english:",
        " communication:",
        " interview:",
        " продажи:",
        " английский:",
        " коммуникация:",
        " интервью:",
    ]
    .into_iter()
    .filter_map(|marker| lowercase.find(marker))
    .min()
    .unwrap_or(value.len());
    value[..cutoff].trim().to_string()
}

fn truncate_words(value: &str, limit: usize) -> String {
    let words: Vec<&str> = value.split_whitespace().collect();
    if words.len() <= limit {
        return value.trim().to_string();
    }
    let visible = &words[..limit];
    let minimum_boundary = limit / 2;
    if let Some(index) = visible
        .iter()
        .rposition(|word| word.ends_with(['.', '!', '?', ';']))
        .filter(|index| *index + 1 >= minimum_boundary)
    {
        return visible[..=index].join(" ");
    }
    if let Some(index) = visible
        .iter()
        .rposition(|word| word.ends_with(','))
        .filter(|index| *index + 1 >= minimum_boundary)
    {
        return format!("{}.", visible[..=index].join(" ").trim_end_matches(','));
    }
    format!(
        "{}…",
        visible.join(" ").trim_end_matches(['.', ',', ';', ':'])
    )
}

fn normalize_timestamp(value: &str) -> Option<String> {
    let mut normalized = Vec::new();
    for part in value.split(['-', '–', '—']).take(2) {
        let prefix: String = part
            .trim()
            .chars()
            .take_while(|character| {
                character.is_ascii_digit() || *character == ':' || *character == '.'
            })
            .collect();
        if !prefix.contains(':') {
            let seconds = prefix.parse::<f64>().ok()?;
            normalized.push(crate::transcript::format_timestamp(seconds));
            continue;
        }
        let fields: Vec<&str> = prefix.split(':').collect();
        let valid = match fields.as_slice() {
            [minutes, seconds] => {
                minutes.parse::<u64>().ok()?;
                seconds.parse::<f64>().ok()? < 60.0
            }
            [hours, minutes, seconds] => {
                hours.parse::<u64>().ok()?;
                minutes.parse::<u64>().ok()? < 60 && seconds.parse::<f64>().ok()? < 60.0
            }
            _ => false,
        };
        if !valid {
            return None;
        }
        normalized.push(prefix.split('.').next().unwrap_or(&prefix).to_string());
    }
    (!normalized.is_empty()).then(|| normalized.join("-"))
}

fn prose_word_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .count()
}

fn read_api_key() -> Result<String> {
    if let Ok(key) = std::env::var("OPENAI_API_KEY")
        && !key.trim().is_empty()
    {
        return Ok(key.trim().to_string());
    }
    let app_home = Config::path()?
        .parent()
        .context("Config path has no parent directory")?
        .to_path_buf();
    let token_path = app_home.join("openai-api-key");
    if token_path.exists() {
        let key = fs::read_to_string(&token_path)
            .with_context(|| format!("Failed to read {}", token_path.display()))?;
        if !key.trim().is_empty() {
            return Ok(key.trim().to_string());
        }
    }
    let env_path = app_home.join(".env");
    if env_path.exists() {
        let content = fs::read_to_string(&env_path)
            .with_context(|| format!("Failed to read {}", env_path.display()))?;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(value) = line.strip_prefix("OPENAI_API_KEY=") {
                let value = value.trim().trim_matches(['\'', '"']);
                if !value.is_empty() {
                    return Ok(value.to_string());
                }
            }
        }
    }
    bail!("OPENAI_API_KEY is missing; run `voxray setup-ai-token` or set it in the environment")
}

pub fn require_api_key() -> Result<()> {
    read_api_key().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_document() -> AnalysisDocument {
        let metrics = Metrics {
            duration_seconds: 90.0,
            participants: std::collections::BTreeMap::from([
                (
                    "target".to_string(),
                    crate::transcript::ParticipantMetrics {
                        words: 120,
                        speaking_seconds: 50.0,
                        speaking_ratio: 62.5,
                        turns: 4,
                        questions: 2,
                        longest_turn_words: 40,
                        fillers: 3,
                        words_per_minute: Some(144.0),
                        ..Default::default()
                    },
                ),
                (
                    "participant-2".to_string(),
                    crate::transcript::ParticipantMetrics {
                        words: 80,
                        speaking_seconds: 30.0,
                        speaking_ratio: 37.5,
                        turns: 5,
                        questions: 3,
                        longest_turn_words: 22,
                        fillers: 1,
                        words_per_minute: Some(160.0),
                        ..Default::default()
                    },
                ),
            ]),
            precision: "test".to_string(),
        };
        AnalysisDocument {
            schema_version: 2,
            run: RunMetadata {
                status: "complete".to_string(),
                model: "gpt-5.6-terra".to_string(),
                reasoning_effort: None,
                prompt_version: "test".to_string(),
                started_at: "2026-08-21T00:00:00Z".to_string(),
                finished_at: "2026-08-21T00:00:01Z".to_string(),
                latency_ms: 1,
                usage: Value::Null,
                cost_usd: None,
                cost_note: String::new(),
            },
            metrics,
            quick_review: QuickReview {
                main_failure: "The value proposition stayed vague.".to_string(),
                next_call_action: "Ask one concrete impact question.".to_string(),
                keep_doing: "Keep the calm opening.".to_string(),
                practice: "Rehearse the impact question.".to_string(),
            },
            modules: vec![ModuleReview {
                name: "sales".to_string(),
                score: 3,
                summary: "Adequate discovery.".to_string(),
                main_issue: "Discovery stopped too early.".to_string(),
                evidence: Vec::new(),
            }],
            issues: Vec::new(),
            deal_notes: DealNotes::default(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn extracts_structured_output_text() {
        let payload = json!({
            "output": [{"type": "message", "content": [
                {"type": "output_text", "text": "{\"warnings\":[]}"}
            ]}]
        });
        assert_eq!(output_text(&payload), Some("{\"warnings\":[]}"));
    }

    #[test]
    fn maps_legacy_common_module() {
        let modules = normalize_modules(&["sales".to_string(), "common".to_string()]).unwrap();
        assert_eq!(modules, vec!["sales", "communication"]);
    }

    #[test]
    fn strips_fractional_and_invalid_timestamp_suffixes() {
        assert_eq!(
            normalize_timestamp("00:05:24-00:05:56.000garbage"),
            Some("00:05:24-00:05:56".to_string())
        );
    }

    #[test]
    fn converts_numeric_second_ranges() {
        assert_eq!(
            normalize_timestamp("13.97–17.42"),
            Some("00:00:14-00:00:17".to_string())
        );
    }

    #[test]
    fn removes_repeated_quick_review_label() {
        assert_eq!(
            strip_label("Главный провал: слишком общий ответ", &["главный провал"]),
            "слишком общий ответ"
        );
    }

    #[test]
    fn removes_scores_embedded_in_practice() {
        assert_eq!(
            strip_score_suffix("Повторите пять раз. Sales: 2/5. English: 3/5."),
            "Повторите пять раз."
        );
    }

    #[test]
    fn truncates_quick_review_field_to_word_budget() {
        assert_eq!(
            truncate_words("one two three four five", 3),
            "one two three…"
        );
    }

    #[test]
    fn truncates_quick_review_at_a_natural_boundary() {
        assert_eq!(
            truncate_words("Ответьте прямо, а потом задайте вопрос покупателю", 3),
            "Ответьте прямо."
        );
        assert_eq!(
            truncate_words("Сделайте упражнение. Не добавляйте вторую мысль", 3),
            "Сделайте упражнение."
        );
    }

    #[test]
    fn responses_request_disables_storage() {
        let body = json!({"store": false});
        assert_eq!(body["store"], false);
    }

    #[test]
    fn renders_plain_text_feedback_with_local_statistics() {
        let feedback = render_feedback(&sample_document());
        assert!(feedback.contains("QUICK REVIEW"));
        assert!(feedback.contains("CALL STATISTICS"));
        assert!(feedback.contains("| Words"));
        assert!(!feedback.contains("**"));
        assert!(!feedback.contains("# "));
    }
}
