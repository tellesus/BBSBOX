// SPDX-License-Identifier: AGPL-3.0-or-later
//! Compiled kind/purpose/extension gates precede the feature's current causal/resource policy.
use crate::{CompleteEvent, Dependency, Extension, MAX_DEPENDENCIES, Role, token};
use bbsbox_credentials::Purpose;
use std::collections::BTreeSet;

pub struct KindRule {
    pub kind: String,
    pub schema: u16,
    pub actor: Role,
    pub purpose: Option<Purpose>,
}
pub struct Registry {
    rules: Vec<KindRule>,
    required_extensions: BTreeSet<u16>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny(&'static str),
    Missing(Vec<Dependency>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Authorized,
    Denied(&'static str),
    Missing(Vec<Dependency>),
    UnsupportedMinor(u16),
    UnsupportedKind,
    UnsupportedSchema(u16),
    UnsupportedExtension(u16),
}
pub trait Authority {
    fn action(&self, event: &CompleteEvent) -> Decision;
    fn extension(&self, _event: &CompleteEvent, _extension: &Extension) -> Decision {
        Decision::Deny("extension_handler_missing")
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidRegistry;
impl std::fmt::Display for InvalidRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid, duplicate or mismatched event kind rules")
    }
}
impl std::error::Error for InvalidRegistry {}
fn outcome(decision: Decision) -> Outcome {
    match decision {
        Decision::Allow => Outcome::Authorized,
        Decision::Deny(reason) => Outcome::Denied(reason),
        Decision::Missing(mut missing) => {
            if missing.is_empty() || missing.len() > MAX_DEPENDENCIES {
                return Outcome::Denied("invalid_missing_dependencies");
            }
            missing.sort_unstable();
            missing.dedup();
            Outcome::Missing(missing)
        }
    }
}
impl Registry {
    pub fn new(
        rules: Vec<KindRule>,
        required_extensions: Vec<u16>,
    ) -> Result<Self, InvalidRegistry> {
        let mut unique = BTreeSet::new();
        for rule in &rules {
            if !token(&rule.kind, 64)
                || rule.schema == 0
                || (rule.actor == Role::PersonDevice) != rule.purpose.is_some()
                || !unique.insert((rule.kind.as_str(), rule.schema, rule.actor))
            {
                return Err(InvalidRegistry);
            }
        }
        let extensions: BTreeSet<_> = required_extensions.iter().copied().collect();
        if extensions.len() != required_extensions.len() || extensions.contains(&0) {
            return Err(InvalidRegistry);
        }
        Ok(Self {
            rules,
            required_extensions: extensions,
        })
    }
    /// This is permission to reduce, not content integration/relevance/credit. The caller still commits domain work.
    pub fn authorize(&self, event: &CompleteEvent, authority: &dyn Authority) -> Outcome {
        let metadata = event.metadata();
        if metadata.minor != 0 {
            return Outcome::UnsupportedMinor(metadata.minor);
        }
        for extension in &metadata.extensions {
            if extension.required && !self.required_extensions.contains(&extension.id) {
                return Outcome::UnsupportedExtension(extension.id);
            }
        }
        if !self.rules.iter().any(|rule| rule.kind == metadata.kind) {
            return Outcome::UnsupportedKind;
        }
        if !self
            .rules
            .iter()
            .any(|rule| rule.kind == metadata.kind && rule.schema == metadata.schema)
        {
            return Outcome::UnsupportedSchema(metadata.schema);
        }
        let Some(rule) = self.rules.iter().find(|rule| {
            rule.kind == metadata.kind
                && rule.schema == metadata.schema
                && rule.actor == metadata.actor.role
        }) else {
            return Outcome::Denied("wrong_actor_role");
        };
        if let Some(purpose) = rule.purpose {
            let Some(credential) = event.commitment().credential() else {
                return Outcome::Denied("missing_device_delegation");
            };
            if !credential.body().purposes().contains(purpose) {
                return Outcome::Denied("purpose_not_delegated");
            }
        }
        for extension in &metadata.extensions {
            if self.required_extensions.contains(&extension.id) {
                if !extension.required {
                    return Outcome::Denied("required_extension_downgraded");
                }
                let checked = outcome(authority.extension(event, extension));
                if checked != Outcome::Authorized {
                    return checked;
                }
            }
        }
        outcome(authority.action(event))
    }
}
