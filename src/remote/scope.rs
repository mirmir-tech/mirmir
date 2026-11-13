use serde::{Deserialize, Serialize};

/// What a remote API key may do. Local Unix-socket clients need no key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Chat, embeddings, reranking, model lists, inspection, telemetry and
    /// activity.
    Use,
    /// Everything in `use`, plus loading, unloading, downloading and removing
    /// models, settings and cancellation.
    Control,
}

impl Scope {
    #[must_use]
    pub const fn allows(self, required: Self) -> bool {
        matches!((self, required), (Self::Control, _) | (Self::Use, Self::Use))
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Use => "use",
            Self::Control => "control",
        }
    }
}

/// Who may call one `Runtime` method over the TLS listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Remote(Scope),
    /// Remote access management: only over the local socket, so no key can
    /// create or revoke keys.
    LocalOnly,
}

/// The one place that assigns each `Runtime` method its minimum scope.
/// Unknown methods have no entry and are refused remotely.
#[must_use]
pub fn required(method: &str) -> Option<Access> {
    let access = match method {
        "Health" | "ListModels" | "ListActiveModels" | "ListLocalModels" | "InspectModel"
        | "SearchModels" | "Generate" | "Embed" | "Rerank" | "Telemetry" | "TelemetryHistory"
        | "GetConfiguration" | "WatchActivity" => Access::Remote(Scope::Use),
        "UpdateModelGeneration"
        | "LoadModel"
        | "UnloadModel"
        | "PullModel"
        | "RemoveModel"
        | "UpdateConfiguration"
        | "CancelOperation" => Access::Remote(Scope::Control),
        "GetRemoteAccess" | "SetRemoteAccess" | "CreateRemoteKey" | "RevokeRemoteKey" => {
            Access::LocalOnly
        },
        _ => return None,
    };
    Some(access)
}

/// The method name of a gRPC request path such as `/mirmir.v1.Runtime/Health`.
#[must_use]
pub fn method(path: &str) -> Option<&str> {
    path.strip_prefix("/mirmir.v1.Runtime/").filter(|name| !name.contains('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_runtime_method_has_an_explicit_scope() {
        let proto = include_str!("../../proto/mirmir.proto");
        let methods: Vec<&str> = proto
            .lines()
            .filter_map(|line| line.trim().strip_prefix("rpc "))
            .filter_map(|rest| rest.split('(').next())
            .collect();
        assert!(methods.len() > 20);
        for name in methods {
            assert!(required(name).is_some(), "{name} has no remote scope");
        }
        assert_eq!(required("Unknown"), None);
    }

    #[test]
    fn control_includes_use_but_not_the_reverse() {
        assert!(Scope::Control.allows(Scope::Use));
        assert!(Scope::Control.allows(Scope::Control));
        assert!(Scope::Use.allows(Scope::Use));
        assert!(!Scope::Use.allows(Scope::Control));
    }

    #[test]
    fn parses_only_runtime_paths() {
        assert_eq!(method("/mirmir.v1.Runtime/Generate"), Some("Generate"));
        assert_eq!(method("/grpc.health.v1.Health/Check"), None);
        assert_eq!(method("/mirmir.v1.Runtime/a/b"), None);
    }
}
