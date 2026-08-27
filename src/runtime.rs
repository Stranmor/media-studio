use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub enum OptionalStatus {
    Missing,
    Unavailable(PathBuf),
    Available(PathBuf),
}

pub fn required(name: &str) -> Result<PathBuf> {
    which::which(name).with_context(|| {
        format!(
            "не найден обязательный инструмент `{name}` в PATH; установите пакет и повторите запуск"
        )
    })
}

pub fn optional(name: &str) -> Option<PathBuf> {
    which::which(name).ok()
}

/// Resolve an optional integration helper and perform a side-effect-free
/// capability probe. This is important for Flatpak host wrappers: `which`
/// can find the wrapper even when the corresponding host command is absent.
pub fn probe_optional(name: &str) -> OptionalStatus {
    let Some(path) = optional(name) else {
        return OptionalStatus::Missing;
    };
    let probe = Command::new(&path)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match probe {
        Ok(status) if status.success() => OptionalStatus::Available(path),
        _ => OptionalStatus::Unavailable(path),
    }
}

pub fn optional_usable(name: &str) -> Option<PathBuf> {
    match probe_optional(name) {
        OptionalStatus::Available(path) => Some(path),
        OptionalStatus::Missing | OptionalStatus::Unavailable(_) => None,
    }
}

pub fn probe_first(names: &[&str]) -> OptionalStatus {
    let mut unavailable = None;
    for name in names {
        match probe_optional(name) {
            OptionalStatus::Available(path) => return OptionalStatus::Available(path),
            OptionalStatus::Unavailable(path) => unavailable = Some(path),
            OptionalStatus::Missing => {}
        }
    }
    unavailable
        .map(OptionalStatus::Unavailable)
        .unwrap_or(OptionalStatus::Missing)
}
