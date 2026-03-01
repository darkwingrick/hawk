use crate::FeatureFlag;

pub struct NotebookFeatureFlag;

impl FeatureFlag for NotebookFeatureFlag {
    const NAME: &'static str = "notebooks";
}

pub struct PanicFeatureFlag;

impl FeatureFlag for PanicFeatureFlag {
    const NAME: &'static str = "panic";
}

pub struct AgentV2FeatureFlag;

impl FeatureFlag for AgentV2FeatureFlag {
    const NAME: &'static str = "agent-v2";

    fn enabled_for_staff() -> bool {
        true
    }
}

/// A feature flag for granting access to beta ACP features.
///
/// We reuse this feature flag for new betas, so don't delete it if it is not currently in use.
pub struct AcpBetaFeatureFlag;

impl FeatureFlag for AcpBetaFeatureFlag {
    const NAME: &'static str = "acp-beta";
}

pub struct AgentSharingFeatureFlag;

impl FeatureFlag for AgentSharingFeatureFlag {
    const NAME: &'static str = "agent-sharing";
}

pub struct HideAgentPanelFeatureFlag;

impl FeatureFlag for HideAgentPanelFeatureFlag {
    const NAME: &'static str = "hide-agent-panel";
}

pub struct HideDebugButtonFeatureFlag;

impl FeatureFlag for HideDebugButtonFeatureFlag {
    const NAME: &'static str = "hide-debug-button";
}

pub struct SubagentsFeatureFlag;

impl FeatureFlag for SubagentsFeatureFlag {
    const NAME: &'static str = "subagents";

    fn enabled_for_staff() -> bool {
        true
    }
}

pub struct DiffReviewFeatureFlag;

impl FeatureFlag for DiffReviewFeatureFlag {
    const NAME: &'static str = "diff-review";

    fn enabled_for_staff() -> bool {
        false
    }
}

pub struct GitGraphFeatureFlag;

impl FeatureFlag for GitGraphFeatureFlag {
    const NAME: &'static str = "git-graph";
}

pub struct StreamingEditFileToolFeatureFlag;

impl FeatureFlag for StreamingEditFileToolFeatureFlag {
    const NAME: &'static str = "streaming-edit-file-tool";

    fn enabled_for_staff() -> bool {
        false
    }
}

pub struct CollabFeatureFlag;

impl FeatureFlag for CollabFeatureFlag {
    const NAME: &'static str = "collab";

    fn enabled_for_staff() -> bool {
        true
    }

    fn enabled_for_all() -> bool {
        true
    }
}

pub struct RemotesFeatureFlag;

impl FeatureFlag for RemotesFeatureFlag {
    const NAME: &'static str = "remotes";

    fn enabled_for_staff() -> bool {
        false
    }

    fn enabled_for_all() -> bool {
        false
    }
}

pub struct TelemetryFeatureFlag;

impl FeatureFlag for TelemetryFeatureFlag {
    const NAME: &'static str = "telemetry";

    fn enabled_for_staff() -> bool {
        false
    }

    fn enabled_for_all() -> bool {
        false
    }
}

pub struct CrashReportingFeatureFlag;

impl FeatureFlag for CrashReportingFeatureFlag {
    const NAME: &'static str = "crash-reporting";

    fn enabled_for_staff() -> bool {
        false
    }

    fn enabled_for_all() -> bool {
        false
    }
}

pub struct GoogleAuthFeatureFlag;

impl FeatureFlag for GoogleAuthFeatureFlag {
    const NAME: &'static str = "google-auth";

    fn enabled_for_staff() -> bool {
        false
    }

    fn enabled_for_all() -> bool {
        false
    }
}
