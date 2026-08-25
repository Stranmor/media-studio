use anyhow::{Context, Result};
use std::path::PathBuf;

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
