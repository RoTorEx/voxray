use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::Parser;
use inquire::{Confirm, Select, Text};
use serde::Serialize;
use serde_json::{Value, json};

mod analysis;
mod call;
mod cli;
mod config;
mod inbox;
mod listen;
mod logs;
mod storage;
mod transcribe;
mod transcript;

use cli::{Cli, Commands, ProfileArgs};
use config::{Config, Mode, Profile};

#[derive(Debug, Serialize)]
struct CommandOutcome {
    status: &'static str,
    command: &'static str,
    result: String,
    effective: Value,
    artifacts: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    quick_review: Option<String>,
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
                    payload["hint"] =
                        json!("Retry with repeated --target-speaker or --subject-speaker");
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
    let config = Config::load()?;
    let interactive = !cli.non_interactive;
    match cli.command {
        Commands::Listen(args) => {
            let (profile_name, profile) = effective_profile(&config, args.profile, interactive)?;
            let recording = resolve_recording(args.recording, &profile.inbox_dir, interactive)?;
            let default_name = recording
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("recording");
            let name = match args.name {
                Some(name) => name,
                None if interactive => prompt_text("Call name", default_name)?,
                None => default_name.to_string(),
            };
            let move_source = if interactive && !args.r#move && !args.copy {
                Select::new("Source action:", vec!["copy", "move"])
                    .prompt()
                    .map_err(|error| anyhow::anyhow!("Failed to select source action: {error}"))?
                    == "move"
            } else {
                args.r#move
            };
            let plan = listen::plan(&recording, &name, &profile)?;
            let effective = effective_json(
                &profile_name,
                &profile,
                json!({
                    "recording": recording,
                    "name": name,
                    "source_action": if move_source {"move"} else {"copy"},
                    "destination": plan.recording,
                    "derived_audio": plan.derived_audio,
                }),
            );
            show_effective("listen", &effective);
            let result = listen::run(&recording, &name, move_source, &profile)?;
            let mut artifacts = BTreeMap::new();
            artifacts.insert(
                "recording".to_string(),
                result.recording.display().to_string(),
            );
            if let Some(path) = result.derived_audio {
                artifacts.insert("audio".to_string(), path.display().to_string());
            }
            Ok(CommandOutcome {
                status: "success",
                command: "listen",
                result: "created".to_string(),
                effective,
                artifacts,
                quick_review: None,
            })
        }
        Commands::Transcribe(args) => {
            let (profile_name, profile) = effective_profile(&config, args.profile, interactive)?;
            let recording = resolve_recording(args.recording, &profile.calls_dir, interactive)?;
            let force = prompt_force(args.force, interactive, "Replace existing transcript?")?;
            let paths = call::CallPaths::from_recording(&recording)?;
            let effective = effective_json(
                &profile_name,
                &profile,
                json!({
                    "recording": recording,
                    "force": force,
                    "model": config.transcription.model,
                    "transcript": paths.transcript,
                    "call_json": paths.manifest,
                }),
            );
            show_effective("transcribe", &effective);
            let result = transcribe::run(
                &recording,
                &profile_name,
                &profile,
                &config.transcription.model,
                force,
                interactive,
            )?;
            let artifacts = BTreeMap::from([
                (
                    "transcript".to_string(),
                    result.transcript.display().to_string(),
                ),
                (
                    "call_json".to_string(),
                    result.call_json.display().to_string(),
                ),
            ]);
            Ok(CommandOutcome {
                status: "success",
                command: "transcribe",
                result: result.result,
                effective,
                artifacts,
                quick_review: None,
            })
        }
        Commands::Feedback(args) => {
            let (profile_name, mut profile) =
                effective_profile(&config, args.profile, interactive)?;
            if !args.target_speakers.is_empty() {
                profile.subject_speakers = args.target_speakers;
            }
            let transcript = resolve_transcript(args.transcript, &profile.calls_dir, interactive)?;
            let force = prompt_force(args.force, interactive, "Replace existing feedback?")?;
            let paths = call::CallPaths::from_transcript(&transcript)?;
            let effective = effective_json(
                &profile_name,
                &profile,
                json!({
                    "transcript": transcript,
                    "force": force,
                    "model": config.analysis.model,
                    "store": false,
                    "feedback": paths.feedback,
                    "call_json": paths.manifest,
                }),
            );
            show_effective("feedback", &effective);
            let result = analysis::run(
                &config,
                &profile_name,
                &profile,
                &transcript,
                force,
                interactive,
            )?;
            let mut artifacts = BTreeMap::from([(
                "feedback".to_string(),
                result.feedback.display().to_string(),
            )]);
            if let Some(path) = result.call_json {
                artifacts.insert("call_json".to_string(), path.display().to_string());
            }
            Ok(CommandOutcome {
                status: "success",
                command: "feedback",
                result: result.result,
                effective,
                artifacts,
                quick_review: (!result.quick_review.is_empty()).then_some(result.quick_review),
            })
        }
    }
}

fn effective_profile(
    config: &Config,
    args: ProfileArgs,
    interactive: bool,
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
    if !args.subject_speakers.is_empty() {
        profile.subject_speakers = args.subject_speakers;
    }
    if !args.modules.is_empty() {
        profile.modules = args.modules;
    }

    if interactive {
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
    profile.subject_speakers = split_list(&prompt_text(
        "subject_speakers (comma-separated)",
        &profile.subject_speakers.join(", "),
    )?);
    profile.modules = split_list(&prompt_text(
        "modules (comma-separated)",
        &profile.modules.join(", "),
    )?);
    Ok(profile)
}

fn prompt_text(label: &str, default: &str) -> Result<String> {
    Text::new(label)
        .with_initial_value(default)
        .prompt()
        .map_err(|error| anyhow::anyhow!("Failed to read {label}: {error}"))
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
                        inbox::is_media_file(&child)
                    } else {
                        is_transcript(&child)
                    }
                {
                    output.push(child);
                }
            }
        } else if path.is_file()
            && if media {
                inbox::is_media_file(&path)
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
            "subject_speakers": profile.subject_speakers,
            "modules": profile.modules,
        },
        "command": command,
    })
}

fn show_effective(command: &str, effective: &Value) {
    eprintln!("\nEffective {command} parameters:");
    if let Ok(text) = serde_json::to_string_pretty(effective) {
        eprintln!("{text}\n");
    }
}

fn print_human_outcome(outcome: &CommandOutcome) {
    println!("{}: {}", outcome.command, outcome.result);
    for (name, path) in &outcome.artifacts {
        println!("{name}: {path}");
    }
    if let Some(quick_review) = &outcome.quick_review {
        println!("\n{quick_review}");
    }
}
