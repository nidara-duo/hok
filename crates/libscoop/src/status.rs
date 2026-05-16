// Copyright (c) 2026 nidara-duo
// Licensed under the Apache License, Version 2.0 or the MIT license,
// at your option. This file may not be copied, modified, or distributed
// except according to those terms.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::error::Fallible;
use crate::Session;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusReport {
    pub scoop_outdated: bool,
    pub buckets_outdated: bool,
    /// True if one or more git repositories could not be read locally.
    pub git_check_failed: bool,
    pub entries: Vec<StatusEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusEntry {
    pub name: String,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub missing_dependencies: Vec<String>,
    pub flags: Vec<StatusInfoFlag>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StatusInfoFlag {
    Outdated,
    InstallFailed,
    Held,
    Deprecated,
    ManifestRemoved,
    MissingDependencies,
}

impl StatusInfoFlag {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Outdated => "Outdated",
            Self::InstallFailed => "Install failed",
            Self::Held => "Held package",
            Self::Deprecated => "Deprecated",
            Self::ManifestRemoved => "Manifest removed",
            Self::MissingDependencies => "Missing dependencies",
        }
    }
}

pub fn collect_status(session: &Session, local_only: bool) -> Fallible<StatusReport> {
    let mut report = StatusReport {
        scoop_outdated: false,
        buckets_outdated: false,
        git_check_failed: false,
        entries: vec![],
    };

    if !local_only {
        let config = session.config();
        let scoop_path = config.root_path().to_path_buf();
        let buckets = crate::bucket::bucket_added(session).unwrap_or_default();
        let bucket_paths: Vec<PathBuf> = buckets.iter().map(|b| b.path().to_path_buf()).collect();

        let (scoop_result, bucket_results): (_, Vec<Result<bool, _>>) = rayon::join(
            || check_repo_outdated(&scoop_path),
            || {
                bucket_paths
                    .par_iter()
                    .map(|p| check_repo_outdated(p))
                    .collect()
            },
        );

        match scoop_result {
            Ok(outdated) => report.scoop_outdated = outdated,
            Err(_) => report.git_check_failed = true,
        }

        for result in bucket_results {
            match result {
                Ok(outdated) => {
                    if outdated {
                        report.buckets_outdated = true;
                    }
                }
                Err(_) => report.git_check_failed = true,
            }
        }
    }

    report.entries = collect_status_entries(session)?;
    report.entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(report)
}

pub fn collect_status_entries(session: &Session) -> Fallible<Vec<StatusEntry>> {
    let states = crate::inventory::collect_package_states(session)?;

    let entries = states
        .into_iter()
        .filter_map(|state| {
            if state.flags.is_empty() {
                return None;
            }

            let flags = state
                .flags
                .iter()
                .map(|f| match f {
                    crate::inventory::PackageStateFlag::Outdated => StatusInfoFlag::Outdated,
                    crate::inventory::PackageStateFlag::InstallFailed => {
                        StatusInfoFlag::InstallFailed
                    }
                    crate::inventory::PackageStateFlag::Held => StatusInfoFlag::Held,
                    crate::inventory::PackageStateFlag::Deprecated => StatusInfoFlag::Deprecated,
                    crate::inventory::PackageStateFlag::ManifestRemoved => {
                        StatusInfoFlag::ManifestRemoved
                    }
                    crate::inventory::PackageStateFlag::MissingDependencies => {
                        StatusInfoFlag::MissingDependencies
                    }
                })
                .collect();

            Some(StatusEntry {
                name: state.name,
                installed_version: state.installed_version,
                latest_version: state.latest_version,
                missing_dependencies: state.missing_dependencies,
                flags,
            })
        })
        .collect();

    Ok(entries)
}

fn check_repo_outdated(repo_path: &Path) -> Fallible<bool> {
    if !repo_path.join(".git").exists() {
        return Ok(false);
    }

    let repo = git2::Repository::open(repo_path)?;
    let head = repo.head()?;
    let head_id = match head.target() {
        Some(id) => id,
        None => return Ok(false),
    };

    let branch = head.shorthand().unwrap_or("main");
    let remote_ref_name = format!("refs/remotes/origin/{}", branch);
    let remote_id = match repo.find_reference(&remote_ref_name) {
        Ok(remote_ref) => remote_ref.target(),
        Err(_) => None,
    };

    match remote_id {
        Some(id) => Ok(head_id != id),
        None => Ok(false),
    }
}
