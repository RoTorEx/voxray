use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

use crate::config::Profile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub schema_version: u32,
    pub metadata: TranscriptMetadata,
    pub speaker_mapping: BTreeMap<String, String>,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptMetadata {
    pub engine: String,
    #[serde(alias = "model")]
    pub exact_model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    pub language: String,
    pub timestamps: bool,
    #[serde(alias = "speakers")]
    pub diarization: bool,
    #[serde(default)]
    pub transcription_duration_ms: u128,
    #[serde(default)]
    pub source_timestamp_unit: String,
    #[serde(default = "seconds_unit")]
    pub normalized_timestamp_unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_data: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub raw_speaker_id: String,
    pub participant_id: String,
    #[serde(default)]
    pub participant_name: String,
    pub start_seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_seconds: Option<f64>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    pub duration_seconds: f64,
    pub participants: BTreeMap<String, ParticipantMetrics>,
    pub precision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ParticipantMetrics {
    pub words: usize,
    pub word_ratio: f64,
    pub speaking_seconds: f64,
    pub speaking_ratio: f64,
    pub turns: usize,
    pub questions: usize,
    pub average_turn_words: f64,
    pub longest_turn_words: usize,
    #[serde(default)]
    pub fillers: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words_per_minute: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpeakerSample {
    pub id: String,
    pub samples: Vec<String>,
}

#[derive(Debug)]
pub struct SpeakerMappingRequired {
    pub speakers: Vec<SpeakerSample>,
}

impl std::fmt::Display for SpeakerMappingRequired {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Speaker mapping is required; retry with --target-speaker or --subject-speaker"
        )
    }
}

impl std::error::Error for SpeakerMappingRequired {}

impl Transcript {
    pub fn from_macwhisper_json(
        value: &Value,
        model: &str,
        language: &str,
        profile: &Profile,
        transcription_duration_ms: u128,
        interactive: bool,
    ) -> Result<Self> {
        let source_segments = value
            .get("segments")
            .and_then(Value::as_array)
            .or_else(|| value.as_array())
            .context("MacWhisper JSON does not contain a segments array")?;
        let (scale, source_timestamp_unit) = timestamp_scale(value, source_segments);
        let mut segments = Vec::new();
        for (index, item) in source_segments.iter().enumerate() {
            let text = string_field(item, &["text", "content"])
                .unwrap_or_default()
                .trim()
                .to_string();
            if text.is_empty() {
                continue;
            }
            let raw_speaker_id = string_field(
                item,
                &[
                    "speaker",
                    "speaker_name",
                    "speakerName",
                    "speaker_id",
                    "speakerId",
                ],
            )
            .unwrap_or_else(|| "Speaker 1".to_string());
            let start_seconds = timestamp_field(item, &["start", "start_time", "startTime"], scale)
                .unwrap_or(index as f64);
            let end_seconds = timestamp_field(item, &["end", "end_time", "endTime"], scale);
            segments.push(Segment {
                raw_speaker_id,
                participant_id: String::new(),
                participant_name: String::new(),
                start_seconds,
                end_seconds,
                text,
            });
        }
        if segments.is_empty() {
            bail!("MacWhisper JSON contains no usable transcript segments");
        }
        let mut transcript = Self {
            schema_version: 2,
            metadata: TranscriptMetadata {
                engine: "macwhisper".to_string(),
                exact_model: model.to_string(),
                engine_version: string_field(value, &["version", "engine_version", "appVersion"]),
                language: language.to_string(),
                timestamps: true,
                diarization: true,
                transcription_duration_ms,
                source_timestamp_unit,
                normalized_timestamp_unit: seconds_unit(),
                provider_data: Some(value.clone()),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
            speaker_mapping: BTreeMap::new(),
            segments,
        };
        transcript.assign_speakers(profile, interactive)?;
        Ok(transcript)
    }

    pub fn from_legacy_text(content: &str, profile: &Profile, interactive: bool) -> Result<Self> {
        let mut segments = parse_readable_text(content, profile);
        if segments.is_empty() {
            segments = parse_old_text(content);
        }
        if segments.is_empty() {
            bail!("Transcript has no recognized speaker/timestamp/text segments");
        }
        let mut transcript = Self {
            schema_version: 2,
            metadata: TranscriptMetadata {
                engine: "legacy-text-import".to_string(),
                exact_model: "unknown".to_string(),
                engine_version: None,
                language: profile.source_language.clone(),
                timestamps: true,
                diarization: true,
                transcription_duration_ms: 0,
                source_timestamp_unit: "formatted_time".to_string(),
                normalized_timestamp_unit: seconds_unit(),
                provider_data: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
            speaker_mapping: BTreeMap::new(),
            segments,
        };
        transcript.assign_speakers(profile, interactive)?;
        Ok(transcript)
    }

    pub fn render_text(&self) -> String {
        let mut output = String::new();
        for segment in &self.segments {
            let name = if segment.participant_name.trim().is_empty() {
                &segment.raw_speaker_id
            } else {
                &segment.participant_name
            };
            output.push_str(&format!(
                "[{}] {}\n{}\n\n",
                format_timestamp(segment.start_seconds),
                name,
                segment.text
            ));
        }
        output
    }

    pub fn metrics(&self, _target: &str) -> Metrics {
        let mut participants: BTreeMap<String, ParticipantMetrics> = BTreeMap::new();
        let total_words: usize = self.segments.iter().map(|s| word_count(&s.text)).sum();
        let mut total_seconds = 0.0;
        let mut current_participant = String::new();
        let mut current_turn_words = 0;
        for (index, segment) in self.segments.iter().enumerate() {
            let words = word_count(&segment.text);
            let next_start = self.segments.get(index + 1).map(|next| next.start_seconds);
            let end = segment
                .end_seconds
                .or(next_start)
                .unwrap_or(segment.start_seconds);
            let seconds = (end - segment.start_seconds).max(0.0);
            total_seconds += seconds;
            let starts_new_turn = current_participant != segment.participant_id;
            if starts_new_turn {
                if !current_participant.is_empty()
                    && let Some(previous) = participants.get_mut(&current_participant)
                {
                    previous.longest_turn_words =
                        previous.longest_turn_words.max(current_turn_words);
                }
                current_participant = segment.participant_id.clone();
                current_turn_words = words;
            } else {
                current_turn_words += words;
            }
            let metrics = participants
                .entry(segment.participant_id.clone())
                .or_default();
            metrics.words += words;
            metrics.speaking_seconds += seconds;
            metrics.questions += segment.text.matches('?').count();
            if starts_new_turn {
                metrics.turns += 1;
            }
        }
        if !current_participant.is_empty()
            && let Some(last) = participants.get_mut(&current_participant)
        {
            last.longest_turn_words = last.longest_turn_words.max(current_turn_words);
        }
        for metrics in participants.values_mut() {
            metrics.word_ratio = ratio(metrics.words as f64, total_words as f64);
            metrics.speaking_ratio = ratio(metrics.speaking_seconds, total_seconds);
            metrics.average_turn_words = average(metrics.words as f64, metrics.turns as f64);
        }
        for (participant, metrics) in &mut participants {
            let text = self
                .segments
                .iter()
                .filter(|segment| &segment.participant_id == participant)
                .map(|segment| segment.text.to_lowercase())
                .collect::<Vec<_>>()
                .join(" ");
            metrics.fillers = count_fillers(&text);
            metrics.words_per_minute = (metrics.speaking_seconds > 0.0).then(|| {
                (metrics.words as f64 / metrics.speaking_seconds * 60.0 * 10.0).round() / 10.0
            });
        }
        Metrics {
            duration_seconds: self
                .segments
                .last()
                .and_then(|segment| segment.end_seconds)
                .unwrap_or_else(|| {
                    self.segments
                        .last()
                        .map_or(0.0, |segment| segment.start_seconds)
                }),
            participants,
            precision: "estimated_from_transcript_timestamps".to_string(),
        }
    }

    fn assign_speakers(&mut self, profile: &Profile, interactive: bool) -> Result<()> {
        let speakers: BTreeSet<String> = self
            .segments
            .iter()
            .map(|segment| segment.raw_speaker_id.clone())
            .collect();
        let target_speakers = if !profile.subject_speakers.is_empty() {
            profile.subject_speakers.clone()
        } else if let Some(named) = speakers
            .iter()
            .find(|speaker| speaker.eq_ignore_ascii_case(&profile.subject_name))
        {
            vec![named.clone()]
        } else if speakers.len() == 1 {
            speakers.iter().cloned().collect()
        } else if interactive {
            print_speaker_samples(&self.segments, &speakers);
            let options: Vec<String> = speakers.iter().cloned().collect();
            let selected = inquire::MultiSelect::new(
                &format!("Which speaker IDs belong to {}?", profile.subject_name),
                options,
            )
            .with_help_message("Select multiple IDs if diarization split one person")
            .prompt()
            .map_err(|error| anyhow::anyhow!("Failed to select target speakers: {error}"))?;
            if selected.is_empty() {
                bail!("At least one target speaker must be selected");
            }
            selected
        } else {
            return Err(SpeakerMappingRequired {
                speakers: speaker_samples(&self.segments, &speakers),
            }
            .into());
        };
        for speaker in &speakers {
            let participant = if target_speakers.contains(speaker) {
                "target".to_string()
            } else {
                format!("participant-{}", slug(speaker))
            };
            self.speaker_mapping.insert(speaker.clone(), participant);
        }
        for segment in &mut self.segments {
            segment.participant_id = self
                .speaker_mapping
                .get(&segment.raw_speaker_id)
                .cloned()
                .unwrap_or_else(|| "participant-unknown".to_string());
            segment.participant_name = if segment.participant_id == "target" {
                profile.subject_name.clone()
            } else {
                segment.raw_speaker_id.clone()
            };
        }
        Ok(())
    }
}

fn print_speaker_samples(segments: &[Segment], speakers: &BTreeSet<String>) {
    eprintln!("\nSpeaker samples:");
    for speaker in speakers {
        eprintln!("\n{speaker}:");
        for segment in segments
            .iter()
            .filter(|segment| &segment.raw_speaker_id == speaker)
            .take(3)
        {
            let sample: String = segment.text.chars().take(120).collect();
            eprintln!("  [{}] {sample}", format_timestamp(segment.start_seconds));
        }
    }
    eprintln!();
}

fn speaker_samples(segments: &[Segment], speakers: &BTreeSet<String>) -> Vec<SpeakerSample> {
    speakers
        .iter()
        .map(|speaker| SpeakerSample {
            id: speaker.clone(),
            samples: segments
                .iter()
                .filter(|segment| &segment.raw_speaker_id == speaker)
                .take(3)
                .map(|segment| {
                    format!(
                        "[{}] {}",
                        format_timestamp(segment.start_seconds),
                        segment.text.chars().take(120).collect::<String>()
                    )
                })
                .collect(),
        })
        .collect()
}

fn parse_readable_text(content: &str, profile: &Profile) -> Vec<Segment> {
    let blocks: Vec<&str> = content.split("\n\n").collect();
    let mut segments = Vec::new();
    for block in blocks {
        let mut lines = block.lines();
        let Some(header) = lines.next().map(str::trim) else {
            continue;
        };
        if !header.starts_with('[') {
            continue;
        }
        let Some(close) = header.find(']') else {
            continue;
        };
        let Some(start_seconds) = parse_timestamp(&header[1..close]) else {
            continue;
        };
        let speaker = header[close + 1..].trim().trim_end_matches(':').trim();
        let text = lines.collect::<Vec<_>>().join("\n").trim().to_string();
        if speaker.is_empty() || text.is_empty() {
            continue;
        }
        let target = speaker.eq_ignore_ascii_case(&profile.subject_name);
        segments.push(Segment {
            raw_speaker_id: speaker.to_string(),
            participant_id: if target { "target" } else { "" }.to_string(),
            participant_name: speaker.to_string(),
            start_seconds,
            end_seconds: None,
            text,
        });
    }
    segments
}

fn parse_old_text(content: &str) -> Vec<Segment> {
    let lines: Vec<&str> = content.lines().collect();
    let mut segments = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let speaker = lines[index].trim();
        if !speaker.to_ascii_lowercase().starts_with("speaker ") {
            index += 1;
            continue;
        }
        let timestamp = lines.get(index + 1).map(|line| line.trim()).unwrap_or("");
        let text = lines.get(index + 2).map(|line| line.trim()).unwrap_or("");
        if let Some(start_seconds) = parse_timestamp(timestamp)
            && !text.is_empty()
        {
            segments.push(Segment {
                raw_speaker_id: speaker.to_string(),
                participant_id: String::new(),
                participant_name: String::new(),
                start_seconds,
                end_seconds: None,
                text: text.to_string(),
            });
            index += 3;
        } else {
            index += 1;
        }
    }
    segments
}

fn string_field(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_str))
        .map(ToString::to_string)
}

fn timestamp_field(value: &Value, names: &[&str], scale: f64) -> Option<f64> {
    names.iter().find_map(|name| {
        let field = value.get(*name)?;
        field
            .as_f64()
            .map(|number| number * scale)
            .or_else(|| field.as_str().and_then(parse_timestamp))
    })
}

fn timestamp_scale(root: &Value, segments: &[Value]) -> (f64, String) {
    if let Some(unit) = string_field(root, &["timestamp_unit", "timestampUnit", "timeUnit"]) {
        if unit.to_ascii_lowercase().contains("milli") || unit.eq_ignore_ascii_case("ms") {
            return (0.001, unit);
        }
        return (1.0, unit);
    }
    let mut starts: Vec<f64> = segments
        .iter()
        .filter_map(|item| number_field(item, &["start", "start_time", "startTime"]))
        .collect();
    starts.sort_by(f64::total_cmp);
    let mut deltas: Vec<f64> = starts
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .filter(|delta| *delta > 0.0)
        .collect();
    deltas.sort_by(f64::total_cmp);
    let median_delta = deltas.get(deltas.len() / 2).copied().unwrap_or_default();
    let maximum = starts.last().copied().unwrap_or_default();
    if maximum > 86_400.0 || median_delta > 300.0 {
        (0.001, "milliseconds_inferred".to_string())
    } else {
        (1.0, "seconds_inferred".to_string())
    }
}

fn number_field(value: &Value, names: &[&str]) -> Option<f64> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_f64))
}

fn parse_timestamp(value: &str) -> Option<f64> {
    let value = value.trim().trim_matches(['[', ']']);
    let parts: Vec<&str> = value.split(':').collect();
    match parts.as_slice() {
        [minutes, seconds] => {
            Some(minutes.parse::<f64>().ok()? * 60.0 + seconds.parse::<f64>().ok()?)
        }
        [hours, minutes, seconds] => Some(
            hours.parse::<f64>().ok()? * 3600.0
                + minutes.parse::<f64>().ok()? * 60.0
                + seconds.parse::<f64>().ok()?,
        ),
        _ => value.parse().ok(),
    }
}

pub fn format_timestamp(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        (total % 3600) / 60,
        total % 60
    )
}

fn seconds_unit() -> String {
    "seconds".to_string()
}

fn slug(value: &str) -> String {
    let slug: String = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    if slug.is_empty() {
        "unknown".to_string()
    } else {
        slug
    }
}

fn word_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .count()
}

fn count_token(text: &str, needle: &str) -> usize {
    text.split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|word| *word == needle)
        .count()
}

fn count_fillers(text: &str) -> usize {
    const FILLERS: &[&str] = &[
        "uh",
        "um",
        "erm",
        "like",
        "yeah",
        "so",
        "basically",
        "actually",
        "ну",
        "эм",
        "ээ",
        "короче",
        "типа",
        "значит",
    ];
    FILLERS.iter().map(|filler| count_token(text, filler)).sum()
}

fn ratio(value: f64, total: f64) -> f64 {
    if total == 0.0 {
        0.0
    } else {
        (value / total * 1000.0).round() / 10.0
    }
}

fn average(value: f64, count: f64) -> f64 {
    if count == 0.0 {
        0.0
    } else {
        (value / count * 10.0).round() / 10.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn profile() -> Profile {
        Profile {
            inbox_dir: PathBuf::from("/tmp"),
            calls_dir: PathBuf::from("/tmp"),
            date_format: None,
            mode: None,
            modules: vec!["sales".to_string()],
            call_type: "sales".to_string(),
            subject_name: "Alex".to_string(),
            subject_role: "seller".to_string(),
            source_language: "en".to_string(),
            call_goal: String::new(),
            subject_speakers: vec!["Speaker 1".to_string()],
        }
    }

    #[test]
    fn imports_legacy_transcript_and_computes_metrics() {
        let input = "Speaker 1\n00:01\nUh hello?\n\nSpeaker 2\n00:03\nHello.\n\nSpeaker 1\n00:05\nSo let us begin.\n";
        let transcript = Transcript::from_legacy_text(input, &profile(), false).unwrap();
        let metrics = transcript.metrics("target");
        assert_eq!(transcript.segments.len(), 3);
        assert_eq!(metrics.participants["target"].questions, 1);
        assert_eq!(metrics.participants["target"].fillers, 2);
        assert_eq!(metrics.participants["target"].longest_turn_words, 4);
        assert!(transcript.render_text().starts_with("[00:00:01] Alex"));
    }

    #[test]
    fn converts_macwhisper_milliseconds_to_seconds() {
        let value = serde_json::json!({"segments": [
            {"speaker": "Speaker 1", "start": 394000, "end": 398000, "text": "Hello"},
            {"speaker": "Speaker 2", "start": 399000, "end": 401000, "text": "Hi"}
        ]});
        let transcript = Transcript::from_macwhisper_json(
            &value,
            "whisper-large-v3",
            "en",
            &profile(),
            100,
            false,
        )
        .unwrap();
        assert_eq!(transcript.segments[0].start_seconds, 394.0);
        assert_eq!(
            transcript.metadata.source_timestamp_unit,
            "milliseconds_inferred"
        );
        assert_eq!(
            transcript.render_text(),
            "[00:06:34] Alex\nHello\n\n[00:06:39] Speaker 2\nHi\n\n"
        );
    }
}
