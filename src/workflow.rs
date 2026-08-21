use anyhow::{Result, bail};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Listen,
    Transcribe,
    Feedback,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Listen => "listen",
            Self::Transcribe => "transcribe",
            Self::Feedback => "feedback",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Plan {
    stages: Vec<Stage>,
}

impl Plan {
    pub fn new(start: Stage, through: Option<Stage>) -> Result<Self> {
        let target = through.unwrap_or(start);
        if target < start {
            bail!(
                "Cannot continue from {} through {}; choose the same or a later stage",
                start.as_str(),
                target.as_str()
            );
        }
        let stages = [Stage::Listen, Stage::Transcribe, Stage::Feedback]
            .into_iter()
            .filter(|stage| *stage >= start && *stage <= target)
            .collect();
        Ok(Self { stages })
    }

    pub fn target(&self) -> Stage {
        *self.stages.last().expect("a workflow plan is never empty")
    }

    pub fn is_pipeline(&self) -> bool {
        self.stages.len() > 1
    }

    pub fn includes(&self, stage: Stage) -> bool {
        self.stages.contains(&stage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_single_stage_by_default() {
        let plan = Plan::new(Stage::Transcribe, None).unwrap();
        assert_eq!(plan.stages, [Stage::Transcribe]);
        assert!(!plan.is_pipeline());
    }

    #[test]
    fn plans_every_stage_through_target() {
        let plan = Plan::new(Stage::Listen, Some(Stage::Feedback)).unwrap();
        assert_eq!(
            plan.stages,
            [Stage::Listen, Stage::Transcribe, Stage::Feedback]
        );
        assert_eq!(plan.target(), Stage::Feedback);
        assert!(plan.includes(Stage::Transcribe));
    }

    #[test]
    fn rejects_backward_pipeline() {
        let error = Plan::new(Stage::Transcribe, Some(Stage::Listen)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Cannot continue from transcribe")
        );
    }
}
