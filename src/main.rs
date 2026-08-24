use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::Parser;
use inquire::{
    Confirm, Select, Text,
    validator::{ErrorMessage, Validation},
};
use serde::Serialize;
use serde_json::{Value, json};

mod analysis;
mod call;
mod cli;
mod config;
mod inbox;
mod logs;
mod media;
mod setup;
mod setup_config;
mod storage;
mod transcribe;
mod transcript;
mod update;
mod workflow;

use cli::{Cli, Commands, InboxArgs, ProfileArgs, TranscribeArgs};
use config::{Config, Mode, Profile};
use workflow::{Plan, Stage};

#[derive(Debug, Serialize)]
struct CommandOutcome {
    status: &'static str,
    command: &'static str,
    result: String,
    effective: Value,
    artifacts: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quick_review: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    through: Option<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    steps: Vec<WorkflowStep>,
}

#[derive(Debug, Serialize)]
struct WorkflowStep {
    command: &'static str,
    result: String,
    effective: Value,
    artifacts: BTreeMap<String, String>,
}

struct TranscriptionInput {
    recording: PathBuf,
    media: PathBuf,
}

#[derive(Clone, Copy)]
struct Presentation {
    interactive: bool,
    show_effective: bool,
}

fn main() {
    let started = Instant::now();
    let raw_args: Vec<String> = std::env::args().collect();
    let wants_json = raw_args.iter().any(|arg| arg == "--json");
    logs::command_start(&raw_args);

    let cli = match Cli::try_parse_from(&raw_args) {
        Ok(cli) => cli,
        Err(error) if error.use_stderr() => {
            if wants_json {
                println!(
                    "{}",
                    json!({"status":"error","command":"cli","message":error.to_string()})
                );
            } else {
                let _ = error.print();
            }
            std::process::exit(2);
        }
        Err(error) => {
            let _ = error.print();
            return;
        }
    };
    let json_output = cli.json;
    let result = run(cli);
    match result {
        Ok(outcome) => {
            logs::info(&format!(
                "command_end command={} result={} duration={:.3}s",
                outcome.command,
                outcome.result,
                started.elapsed().as_secs_f64()
            ));
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&outcome).expect("outcome is serializable")
                );
            } else {
                print_human_outcome(&outcome);
            }
        }
        Err(error) => {
            logs::error(&format!(
                "command_end error=\"{}\" duration={:.3}s",
                error,
                started.elapsed().as_secs_f64()
            ));
            if json_output {
                let mut payload = json!({
                    "status": "error",
                    "command": raw_args.get(1).cloned().unwrap_or_else(|| "cli".to_string()),
                    "message": error.to_string(),
                });
                if let Some(mapping) = error.downcast_ref::<transcript::SpeakerMappingRequired>() {
                    payload["code"] = json!("SPEAKER_MAPPING_REQUIRED");
                    payload["speakers"] = json!(mapping.speakers);
                    payload["hint"] = json!("Retry with repeated --target-speaker");
                }
                println!("{payload}");
            } else {
                eprintln!("Error: {error:#}");
            }
            std::process::exit(1);
        }
    }
}

fn run(cli: Cli) -> Result<CommandOutcome> {
    if matches!(&cli.command, Commands::Version) {
        return Ok(CommandOutcome {
            status: "ok",
            command: "version",
            result: env!("CARGO_PKG_VERSION").to_string(),
            effective: json!({}),
            artifacts: BTreeMap::new(),
            quick_review: None,
            through: None,
            steps: Vec::new(),
        });
    }
    if matches!(&cli.command, Commands::Update) {
        let binary = update::run()?;
        return Ok(CommandOutcome {
            status: "ok",
            command: "update",
            result: "installed latest release".to_string(),
            effective: json!({"source": "latest GitHub Release"}),
            artifacts: BTreeMap::from([("binary".to_string(), binary.display().to_string())]),
            quick_review: None,
            through: None,
            steps: Vec::new(),
        });
    }
    if matches!(&cli.command, Commands::SetupAiToken) {
        if cli.non_interactive {
            bail!("setup-ai-token is interactive only");
        }
        let setup = setup::run()?;
        return Ok(CommandOutcome {
            status: "ok",
            command: "setup-ai-token",
            result: if setup.changed {
                "saved OpenAI API token"
            } else {
                "kept existing OpenAI API token"
            }
            .to_string(),
            effective: json!({"permissions": "0600"}),
            artifacts: BTreeMap::from([(
                "token_file".to_string(),
                setup.path.display().to_string(),
            )]),
            quick_review: None,
            through: None,
            steps: Vec::new(),
        });
    }
    if matches!(&cli.command, Commands::SetupConfig) {
        if cli.non_interactive {
            bail!("setup-config is interactive only");
        }
        let setup = setup_config::run()?;
        return Ok(CommandOutcome {
            status: "ok",
            command: "setup-config",
            result: if setup.changed {
                "created starter configuration"
            } else {
                "kept existing configuration"
            }
            .to_string(),
            effective: json!({"permissions": "0600"}),
            artifacts: BTreeMap::from([("config".to_string(), setup.path.display().to_string())]),
            quick_review: None,
            through: None,
            steps: Vec::new(),
        });
    }
    let config = Config::load()?;
    if config.analysis_enabled() {
        analysis::require_api_key()?;
    }
    let presentation = Presentation {
        interactive: !cli.non_interactive,
        show_effective: cli.show_effective,
    };
    let edit_profile = cli.edit_profile;
    match cli.command {
        Commands::Inbox(args) => {
            let plan = Plan::new(Stage::Inbox, args.through.map(Into::into))?;
            let target_speakers = args.profile.target_speakers.clone();
            let (profile_name, profile) = effective_profile(
                &config,
                args.profile.clone(),
                presentation.interactive,
                edit_profile,
            )?;
            run_from_inbox(
                &config,
                &profile_name,
                &profile,
                &target_speakers,
                args,
                plan,
                presentation,
            )
        }
        Commands::Transcribe(args) => {
            let plan = Plan::new(Stage::Transcribe, args.through.map(Into::into))?;
            let target_speakers = args.profile.target_speakers.clone();
            let (profile_name, profile) = effective_profile(
                &config,
                args.profile.clone(),
                presentation.interactive,
                edit_profile,
            )?;
            run_from_transcribe(
                &config,
                &profile_name,
                &profile,
                &target_speakers,
                args,
                plan,
                presentation,
            )
        }
        Commands::Feedback(args) => {
            let target_speakers = args.profile.target_speakers.clone();
            let (profile_name, profile) = effective_profile(
                &config,
                args.profile,
                presentation.interactive,
                edit_profile,
            )?;
            let transcript = resolve_transcript(
                args.transcript,
                &profile.calls_dir,
                presentation.interactive,
            )?;
            let force = prompt_force(
                args.force,
                presentation.interactive,
                "Replace existing feedback?",
            )?;
            run_feedback_step(
                &config,
                &profile_name,
                &profile,
                &target_speakers,
                transcript,
                force,
                presentation,
            )
        }
        Commands::Update => unreachable!("update returns before configuration is loaded"),
        Commands::SetupAiToken => {
            unreachable!("setup-ai-token returns before configuration is loaded")
        }
        Commands::SetupConfig => {
            unreachable!("setup-config returns before configuration is loaded")
        }
        Commands::Version => unreachable!("version returns before configuration is loaded"),
    }
}

fn run_from_inbox(
    config: &Config,
    profile_name: &str,
    profile: &Profile,
    target_speakers: &[String],
    args: InboxArgs,
    plan: Plan,
    presentation: Presentation,
) -> Result<CommandOutcome> {
    let source = resolve_recording(args.recording, &profile.inbox_dir, presentation.interactive)?;
    let default_name = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("recording");
    let name = match args.name {
        Some(name) => name,
        None if presentation.interactive => prompt_call_name()?,
        None => default_name.to_string(),
    };
    let move_source = if presentation.interactive && !args.r#move && !args.copy {
        Select::new("Source action:", vec!["copy", "move"])
            .prompt()
            .map_err(|error| anyhow::anyhow!("Failed to select source action: {error}"))?
            == "move"
    } else {
        args.r#move
    };

    let (inbox_outcome, recording) = run_inbox_step(
        profile_name,
        profile,
        source,
        name,
        move_source,
        presentation.show_effective,
    )?;
    let mut outcomes = vec![inbox_outcome];
    let mut transcript = None;

    if plan.includes(Stage::Transcribe) {
        let (outcome, path) = run_transcribe_step(
            config,
            profile_name,
            profile,
            recording,
            false,
            presentation,
        )?;
        outcomes.push(outcome);
        transcript = Some(path);
    }
    if plan.includes(Stage::Feedback) {
        let transcript = transcript.context("feedback requires a transcript")?;
        outcomes.push(run_feedback_step(
            config,
            profile_name,
            profile,
            target_speakers,
            transcript,
            false,
            presentation,
        )?);
    }
    Ok(finish_workflow(Stage::Inbox, &plan, outcomes))
}

fn run_from_transcribe(
    config: &Config,
    profile_name: &str,
    profile: &Profile,
    target_speakers: &[String],
    args: TranscribeArgs,
    plan: Plan,
    presentation: Presentation,
) -> Result<CommandOutcome> {
    let recording =
        resolve_recording(args.recording, &profile.calls_dir, presentation.interactive)?;
    let input = TranscriptionInput {
        media: recording.clone(),
        recording,
    };
    let force = prompt_force(
        args.force,
        presentation.interactive,
        "Replace existing transcript?",
    )?;
    let (transcribe_outcome, transcript) =
        run_transcribe_step(config, profile_name, profile, input, force, presentation)?;
    let mut outcomes = vec![transcribe_outcome];
    if plan.includes(Stage::Feedback) {
        outcomes.push(run_feedback_step(
            config,
            profile_name,
            profile,
            target_speakers,
            transcript,
            false,
            presentation,
        )?);
    }
    Ok(finish_workflow(Stage::Transcribe, &plan, outcomes))
}

fn run_inbox_step(
    profile_name: &str,
    profile: &Profile,
    source: PathBuf,
    name: String,
    move_source: bool,
    show_effective: bool,
) -> Result<(CommandOutcome, TranscriptionInput)> {
    let plan = inbox::plan(&source, &name, profile)?;
    let effective = effective_json(
        profile_name,
        profile,
        json!({
            "recording": source,
            "name": name,
            "source_action": if move_source {"move"} else {"copy"},
            "destination": plan.recording,
            "derived_audio": plan.derived_audio,
        }),
    );
    preview_effective(show_effective, "inbox", &effective);
    let result = inbox::run(&source, &name, move_source, profile)?;
    let transcription_input = TranscriptionInput {
        recording: result.recording.clone(),
        media: result.transcription_input().to_path_buf(),
    };
    let mut artifacts = BTreeMap::from([(
        "recording".to_string(),
        result.recording.display().to_string(),
    )]);
    if let Some(path) = result.derived_audio {
        artifacts.insert("audio".to_string(), path.display().to_string());
    }
    Ok((
        step_outcome("inbox", "created".to_string(), effective, artifacts, None),
        transcription_input,
    ))
}

fn run_transcribe_step(
    config: &Config,
    profile_name: &str,
    profile: &Profile,
    input: TranscriptionInput,
    force: bool,
    presentation: Presentation,
) -> Result<(CommandOutcome, PathBuf)> {
    let paths = call::CallPaths::from_recording(&input.recording)?;
    let effective = effective_json(
        profile_name,
        profile,
        json!({
            "recording": input.recording,
            "transcription_input": input.media,
            "force": force,
            "model": config.transcription.model,
            "transcript": paths.transcript,
        }),
    );
    preview_effective(presentation.show_effective, "transcribe", &effective);
    let result = transcribe::run(
        &input.recording,
        &input.media,
        profile,
        transcribe::TranscribeOptions {
            model: &config.transcription.model,
            force,
        },
    )?;
    let transcript = result.transcript.clone();
    let artifacts = BTreeMap::from([(
        "transcript".to_string(),
        result.transcript.display().to_string(),
    )]);
    Ok((
        step_outcome("transcribe", result.result, effective, artifacts, None),
        transcript,
    ))
}

fn run_feedback_step(
    config: &Config,
    profile_name: &str,
    profile: &Profile,
    target_speakers: &[String],
    transcript: PathBuf,
    force: bool,
    presentation: Presentation,
) -> Result<CommandOutcome> {
    let paths = call::CallPaths::from_transcript(&transcript)?;
    let effective = effective_json(
        profile_name,
        profile,
        json!({
            "transcript": transcript,
            "force": force,
            "model": config.analysis.model,
            "store": false,
            "feedback": paths.feedback,
            "call_json": paths.manifest,
        }),
    );
    preview_effective(presentation.show_effective, "feedback", &effective);
    let result = analysis::run(
        config,
        profile_name,
        profile,
        target_speakers,
        &transcript,
        force,
        presentation.interactive,
    )?;
    let mut artifacts = BTreeMap::from([(
        "feedback".to_string(),
        result.feedback.display().to_string(),
    )]);
    if let Some(path) = result.call_json {
        artifacts.insert("call_json".to_string(), path.display().to_string());
    }
    let quick_review = (!result.quick_review.is_empty()).then_some(result.quick_review);
    Ok(step_outcome(
        "feedback",
        result.result,
        effective,
        artifacts,
        quick_review,
    ))
}

fn step_outcome(
    command: &'static str,
    result: String,
    effective: Value,
    artifacts: BTreeMap<String, String>,
    quick_review: Option<String>,
) -> CommandOutcome {
    CommandOutcome {
        status: "success",
        command,
        result,
        effective,
        artifacts,
        quick_review,
        through: None,
        steps: Vec::new(),
    }
}

fn finish_workflow(start: Stage, plan: &Plan, outcomes: Vec<CommandOutcome>) -> CommandOutcome {
    if !plan.is_pipeline() {
        return outcomes
            .into_iter()
            .next()
            .expect("a workflow always has an outcome");
    }
    let mut artifacts = BTreeMap::new();
    let quick_review = outcomes
        .last()
        .and_then(|outcome| outcome.quick_review.clone());
    let steps = outcomes
        .into_iter()
        .map(|outcome| {
            artifacts.extend(outcome.artifacts.clone());
            WorkflowStep {
                command: outcome.command,
                result: outcome.result,
                effective: outcome.effective,
                artifacts: outcome.artifacts,
            }
        })
        .collect();
    CommandOutcome {
        status: "success",
        command: start.as_str(),
        result: "completed".to_string(),
        effective: json!({
            "start": start,
            "through": plan.target(),
        }),
        artifacts,
        quick_review,
        through: Some(plan.target().as_str()),
        steps,
    }
}

fn effective_profile(
    config: &Config,
    args: ProfileArgs,
    interactive: bool,
    edit_profile: bool,
) -> Result<(String, Profile)> {
    let profile_name = match args.profile {
        Some(name) => name,
        None if interactive => pick_profile(config)?,
        None => "default".to_string(),
    };
    let base = config.profile((profile_name != "default").then_some(profile_name.as_str()))?;
    let mut profile = base.clone();
    if let Some(value) = args.inbox_dir {
        profile.inbox_dir = value;
    }
    if let Some(value) = args.calls_dir {
        profile.calls_dir = value;
    }
    if let Some(value) = args.date_format {
        profile.date_format = Some(value);
    }
    if let Some(value) = args.mode {
        profile.mode = Some(value.into());
    }
    if let Some(value) = args.call_type {
        profile.call_type = value;
    }
    if let Some(value) = args.subject_name {
        profile.subject_name = value;
    }
    if let Some(value) = args.subject_role {
        profile.subject_role = value;
    }
    if let Some(value) = args.source_language {
        profile.source_language = value;
    }
    if let Some(value) = args.call_goal {
        profile.call_goal = value;
    }
    if !args.modules.is_empty() {
        profile.modules = args.modules;
    }

    if interactive && edit_profile {
        profile = prompt_profile(profile)?;
    }
    profile.mode.get_or_insert(Mode::Folder);
    validate_profile(&profile)?;
    Ok((profile_name, profile))
}

fn prompt_profile(mut profile: Profile) -> Result<Profile> {
    profile.inbox_dir = PathBuf::from(prompt_text(
        "inbox_dir",
        &profile.inbox_dir.display().to_string(),
    )?);
    profile.calls_dir = PathBuf::from(prompt_text(
        "calls_dir",
        &profile.calls_dir.display().to_string(),
    )?);
    profile.date_format = Some(prompt_text(
        "date_format",
        profile.date_format.as_deref().unwrap_or("%Y-%m-%d %H-%M"),
    )?);
    let mode = prompt_text(
        "mode (file/folder)",
        match profile.mode.unwrap_or_default() {
            Mode::File => "file",
            Mode::Folder => "folder",
        },
    )?;
    profile.mode = Some(match mode.as_str() {
        "file" => Mode::File,
        "folder" => Mode::Folder,
        _ => bail!("mode must be file or folder"),
    });
    profile.call_type = prompt_text("call_type", &profile.call_type)?;
    profile.subject_name = prompt_text("subject_name", &profile.subject_name)?;
    profile.subject_role = prompt_text("subject_role", &profile.subject_role)?;
    profile.source_language = prompt_text("source_language", &profile.source_language)?;
    profile.call_goal = prompt_text("call_goal", &profile.call_goal)?;
    profile.modules = split_list(&prompt_text(
        "modules (comma-separated)",
        &profile.modules.join(", "),
    )?);
    Ok(profile)
}

fn prompt_text(label: &str, default: &str) -> Result<String> {
    Text::new(label)
        .with_default(default)
        .prompt()
        .map_err(|error| anyhow::anyhow!("Failed to read {label}: {error}"))
}

fn prompt_call_name() -> Result<String> {
    Text::new("Call name")
        .with_validator(|value: &str| {
            if normalize_call_name_input(value).is_none() {
                Ok(Validation::Invalid(ErrorMessage::Custom(
                    "Call name is required.".to_string(),
                )))
            } else {
                Ok(Validation::Valid)
            }
        })
        .prompt()
        .map_err(|error| anyhow::anyhow!("Failed to read Call name: {error}"))
        .and_then(|value| {
            normalize_call_name_input(&value).context("Call name unexpectedly became empty")
        })
}

fn normalize_call_name_input(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn validate_profile(profile: &Profile) -> Result<()> {
    if profile.calls_dir.as_os_str().is_empty() {
        bail!("calls_dir is required");
    }
    if profile.inbox_dir.as_os_str().is_empty() {
        bail!("inbox_dir is required");
    }
    if profile.subject_name.trim().is_empty() {
        bail!("subject_name is required");
    }
    if profile.source_language.trim().is_empty() {
        bail!("source_language is required");
    }
    Ok(())
}

fn pick_profile(config: &Config) -> Result<String> {
    let labels = config.profile_labels();
    Select::new("Profile:", labels)
        .prompt()
        .map_err(|error| anyhow::anyhow!("Failed to select profile: {error}"))
}

fn resolve_recording(
    value: Option<PathBuf>,
    directory: &Path,
    interactive: bool,
) -> Result<PathBuf> {
    match value {
        Some(path) => resolve_path(directory, path, "Recording"),
        None if interactive => pick_path(directory, true),
        None => bail!(
            "Missing --recording. Example: voxray transcribe --profile NAME --recording /path/call.m4a --non-interactive"
        ),
    }
}

fn resolve_transcript(
    value: Option<PathBuf>,
    directory: &Path,
    interactive: bool,
) -> Result<PathBuf> {
    match value {
        Some(path) => resolve_path(directory, path, "Transcript"),
        None if interactive => pick_path(directory, false),
        None => bail!(
            "Missing --transcript. Example: voxray feedback --profile NAME --transcript /path/transcript.txt --non-interactive"
        ),
    }
}

fn resolve_path(base: &Path, path: PathBuf, label: &str) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path
    } else {
        base.join(path)
    };
    call::validate_file(&path, label)?;
    Ok(path)
}

fn pick_path(directory: &Path, media: bool) -> Result<PathBuf> {
    let mut paths = Vec::new();
    collect_paths(directory, media, &mut paths)?;
    if paths.is_empty() {
        bail!("No matching files found in {}", directory.display());
    }
    paths.sort();
    let labels: Vec<String> = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    let selected = Select::new(if media { "Recording:" } else { "Transcript:" }, labels)
        .prompt()
        .map_err(|error| anyhow::anyhow!("Failed to select input: {error}"))?;
    paths
        .into_iter()
        .find(|path| path.display().to_string() == selected)
        .context("Selected path disappeared")
}

fn collect_paths(directory: &Path, media: bool, output: &mut Vec<PathBuf>) -> Result<()> {
    if !directory.is_dir() {
        bail!("Directory does not exist: {}", directory.display());
    }
    for entry in std::fs::read_dir(directory)
        .with_context(|| format!("Failed to read {}", directory.display()))?
    {
        let path = entry?.path();
        if path.is_dir() {
            for child in std::fs::read_dir(&path)? {
                let child = child?.path();
                if child.is_file()
                    && if media {
                        media::is_media_file(&child)
                    } else {
                        is_transcript(&child)
                    }
                {
                    output.push(child);
                }
            }
        } else if path.is_file()
            && if media {
                media::is_media_file(&path)
            } else {
                is_transcript(&path)
            }
        {
            output.push(path);
        }
    }
    Ok(())
}

fn is_transcript(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name == "transcript.txt" || name.ends_with(".transcript.txt"))
}

fn prompt_force(current: bool, interactive: bool, label: &str) -> Result<bool> {
    if !interactive {
        return Ok(current);
    }
    Confirm::new(label)
        .with_default(current)
        .prompt()
        .map_err(|error| anyhow::anyhow!("Failed to read force setting: {error}"))
}

fn effective_json(profile_name: &str, profile: &Profile, command: Value) -> Value {
    json!({
        "profile": profile_name,
        "values": {
            "inbox_dir": profile.inbox_dir,
            "calls_dir": profile.calls_dir,
            "date_format": profile.date_format,
            "mode": profile.mode,
            "call_type": profile.call_type,
            "subject_name": profile.subject_name,
            "subject_role": profile.subject_role,
            "source_language": profile.source_language,
            "report_language": config::REPORT_LANGUAGE,
            "call_goal": profile.call_goal,
            "modules": profile.modules,
        },
        "command": command,
    })
}

fn preview_effective(enabled: bool, command: &str, effective: &Value) {
    if !enabled {
        return;
    }
    eprintln!("\nEffective {command} parameters:");
    if let Ok(text) = serde_json::to_string_pretty(effective) {
        eprintln!("{text}\n");
    }
}

fn print_human_outcome(outcome: &CommandOutcome) {
    if outcome.command == "version" {
        println!("voxray {}", outcome.result);
        return;
    }
    println!("{}: {}", outcome.command, outcome.result);
    for step in &outcome.steps {
        println!("{}: {}", step.command, step.result);
    }
    for (name, path) in &outcome.artifacts {
        println!("{name}: {path}");
    }
    if let Some(quick_review) = &outcome.quick_review {
        println!("\n{quick_review}");
    }
}

#[cfg(test)]
mod main_tests {
    use super::normalize_call_name_input;

    #[test]
    fn interactive_call_name_requires_explicit_non_empty_input() {
        assert_eq!(normalize_call_name_input(""), None);
        assert_eq!(normalize_call_name_input("   "), None);
        assert_eq!(
            normalize_call_name_input("  Sync with Nastya  "),
            Some("Sync with Nastya".to_string())
        );
    }
}
