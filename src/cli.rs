use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::workflow::Stage;

#[derive(Debug, Parser)]
#[command(name = "voxray")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Store, transcribe, and review call recordings")]
pub struct Cli {
    /// Never prompt or read stdin
    #[arg(long, global = true)]
    pub non_interactive: bool,

    /// Write exactly one JSON object to stdout
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Install the latest GitHub Release
    Update,

    /// Securely save the OpenAI API token
    SetupAiToken,

    /// Copy or move one recording into the calls library
    #[command(
        after_help = "Interactive: voxray inbox\nNon-interactive: voxray inbox --profile sales --recording /path/call.m4a --non-interactive"
    )]
    Inbox(InboxArgs),

    /// Create transcript.txt and call.json beside one recording
    #[command(
        after_help = "Interactive: voxray transcribe\nNon-interactive: voxray transcribe --profile sales --recording /path/call.record.m4a --non-interactive"
    )]
    Transcribe(TranscribeArgs),

    /// Create one English feedback.txt from one transcript.txt
    #[command(
        after_help = "Interactive: voxray feedback\nNon-interactive: voxray feedback --profile sales --transcript /path/call.transcript.txt --non-interactive"
    )]
    Feedback(FeedbackArgs),
}

#[derive(Debug, Clone, Args, Default)]
pub struct ProfileArgs {
    /// Profile whose values prefill the effective settings
    #[arg(short, long)]
    pub profile: Option<String>,

    /// Override the profile inbox directory
    #[arg(long)]
    pub inbox_dir: Option<PathBuf>,

    /// Override the profile calls directory
    #[arg(long)]
    pub calls_dir: Option<PathBuf>,

    /// Override the strftime date prefix; use an empty value for no prefix
    #[arg(long)]
    pub date_format: Option<String>,

    /// Override file/folder storage mode
    #[arg(long, value_enum)]
    pub mode: Option<ModeArg>,

    /// Override call context used by feedback
    #[arg(long)]
    pub call_type: Option<String>,

    /// Override the evaluated participant name
    #[arg(long)]
    pub subject_name: Option<String>,

    /// Override the evaluated participant role
    #[arg(long)]
    pub subject_role: Option<String>,

    /// Override transcription source language (normally auto)
    #[arg(long)]
    pub source_language: Option<String>,

    /// Override the call goal used by feedback
    #[arg(long)]
    pub call_goal: Option<String>,

    /// Replace profile speaker IDs; repeat to merge raw IDs into the target
    #[arg(long = "subject-speaker")]
    pub subject_speakers: Vec<String>,

    /// Replace profile feedback modules; repeat for multiple modules
    #[arg(long = "module")]
    pub modules: Vec<String>,
}

#[derive(Debug, Args)]
pub struct InboxArgs {
    #[command(flatten)]
    pub profile: ProfileArgs,

    /// Audio or video recording to store
    #[arg(long)]
    pub recording: Option<PathBuf>,

    /// Human call name; defaults to the recording basename in non-interactive mode
    #[arg(long)]
    pub name: Option<String>,

    /// Remove the source only after the target is verified
    #[arg(long, conflicts_with = "copy")]
    pub r#move: bool,

    /// Keep the source (default)
    #[arg(long, conflicts_with = "move")]
    pub copy: bool,

    /// Continue the pipeline through this stage
    #[arg(long, value_enum)]
    pub through: Option<InboxThroughArg>,
}

#[derive(Debug, Args)]
pub struct TranscribeArgs {
    #[command(flatten)]
    pub profile: ProfileArgs,

    /// Recording to transcribe
    #[arg(long)]
    pub recording: Option<PathBuf>,

    /// Safely replace existing transcript.txt and call.json
    #[arg(long)]
    pub force: bool,

    /// Continue the pipeline through this stage
    #[arg(long, value_enum)]
    pub through: Option<TranscribeThroughArg>,
}

#[derive(Debug, Args)]
pub struct FeedbackArgs {
    #[command(flatten)]
    pub profile: ProfileArgs,

    /// Plain UTF-8 transcript to analyze
    #[arg(long)]
    pub transcript: Option<PathBuf>,

    /// Raw speaker ID belonging to the target; repeat to merge IDs
    #[arg(long = "target-speaker")]
    pub target_speakers: Vec<String>,

    /// Safely replace an existing feedback.txt
    #[arg(long)]
    pub force: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ModeArg {
    Folder,
    File,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum InboxThroughArg {
    Transcribe,
    Feedback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum TranscribeThroughArg {
    Feedback,
}

impl From<InboxThroughArg> for Stage {
    fn from(value: InboxThroughArg) -> Self {
        match value {
            InboxThroughArg::Transcribe => Self::Transcribe,
            InboxThroughArg::Feedback => Self::Feedback,
        }
    }
}

impl From<TranscribeThroughArg> for Stage {
    fn from(value: TranscribeThroughArg) -> Self {
        match value {
            TranscribeThroughArg::Feedback => Self::Feedback,
        }
    }
}

impl From<ModeArg> for crate::config::Mode {
    fn from(value: ModeArg) -> Self {
        match value {
            ModeArg::Folder => crate::config::Mode::Folder,
            ModeArg::File => crate::config::Mode::File,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_removed_process_command() {
        assert!(Cli::try_parse_from(["voxray", "process"]).is_err());
    }

    #[test]
    fn parses_update_command() {
        assert!(matches!(
            Cli::try_parse_from(["voxray", "update"]).unwrap().command,
            Commands::Update
        ));
    }

    #[test]
    fn parses_setup_ai_token_command() {
        assert!(matches!(
            Cli::try_parse_from(["voxray", "setup-ai-token"])
                .unwrap()
                .command,
            Commands::SetupAiToken
        ));
    }

    #[test]
    fn profile_overrides_are_available_non_interactively() {
        let cli = Cli::try_parse_from([
            "voxray",
            "feedback",
            "--profile",
            "sales",
            "--transcript",
            "/tmp/call.transcript.txt",
            "--module",
            "sales",
            "--module",
            "english",
            "--subject-speaker",
            "Speaker 1",
            "--non-interactive",
            "--json",
        ])
        .unwrap();
        assert!(cli.non_interactive);
        assert!(cli.json);
        let Commands::Feedback(args) = cli.command else {
            panic!("expected feedback")
        };
        assert_eq!(args.profile.modules, ["sales", "english"]);
        assert_eq!(args.profile.subject_speakers, ["Speaker 1"]);
    }

    #[test]
    fn parses_inbox_through_feedback() {
        let cli = Cli::try_parse_from([
            "voxray",
            "inbox",
            "--recording",
            "/tmp/call.m4a",
            "--through",
            "feedback",
            "--non-interactive",
        ])
        .unwrap();
        let Commands::Inbox(args) = cli.command else {
            panic!("expected inbox")
        };
        assert_eq!(args.through, Some(InboxThroughArg::Feedback));
    }

    #[test]
    fn parses_transcribe_through_feedback() {
        let cli = Cli::try_parse_from([
            "voxray",
            "transcribe",
            "--recording",
            "/tmp/call.m4a",
            "--through",
            "feedback",
            "--non-interactive",
        ])
        .unwrap();
        let Commands::Transcribe(args) = cli.command else {
            panic!("expected transcribe")
        };
        assert_eq!(args.through, Some(TranscribeThroughArg::Feedback));
    }

    #[test]
    fn rejects_backward_pipeline_at_cli_boundary() {
        assert!(Cli::try_parse_from(["voxray", "transcribe", "--through", "inbox"]).is_err());
    }

    #[test]
    fn rejects_removed_listen_command() {
        assert!(Cli::try_parse_from(["voxray", "listen"]).is_err());
    }
}
