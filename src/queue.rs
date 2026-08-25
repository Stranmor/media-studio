use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::runtime;
use crate::{paths, ui};

pub fn enqueue(
    executable: &Path,
    profile: &str,
    target_size_mb: Option<u64>,
    hardware_fallback: bool,
    output_dir: Option<std::path::PathBuf>,
    overwrite: bool,
    files: &[String],
) -> Result<String> {
    if files.is_empty() {
        bail!("в Dolphin ничего не выбрано. Выберите хотя бы один медиафайл.");
    }
    let job_id = new_job_id();
    let unit = format!("media-studio-{job_id}.service");
    let mut command = Command::new(runtime::required("systemd-run")?);
    command
        .args([
            "--user",
            "--collect",
            "--unit",
            &unit,
            "--description",
            "Media Studio conversion",
            "--nice",
            "5",
            "--no-block",
            "--quiet",
        ])
        .arg(executable)
        .args([
            "run",
            "--job-id",
            &job_id,
            "--profile",
            profile,
            "--hardware-fallback",
            &hardware_fallback.to_string(),
        ])
        .args(
            target_size_mb
                .map(|value| vec!["--target-size-mb".to_string(), value.to_string()])
                .unwrap_or_default(),
        )
        .args(
            output_dir
                .map(|value| vec!["--output-dir".to_string(), value.display().to_string()])
                .unwrap_or_default(),
        )
        .args(if overwrite { vec!["--overwrite".to_string()] } else { Vec::new() })
        .arg("--")
        .args(files)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let output = command.output().context("не удалось поставить задачу в user-systemd очередь")?;
    if !output.status.success() {
        let details = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("systemd не принял задачу {job_id}: {details}");
    }
    ui::notify(
        "Media Studio",
        &format!("Задача {job_id} добавлена в очередь: {} файл(ов)", files.len()),
    );
    Ok(job_id)
}

pub fn list() -> Result<String> {
    let output = Command::new(runtime::required("systemctl")?)
        .args(["--user", "list-units", "--all", "--no-legend", "media-studio-*.service"])
        .output()
        .context("не удалось прочитать очередь Media Studio")?;
    if !output.status.success() {
        bail!("systemctl завершился с ошибкой: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(if text.is_empty() { "Очередь Media Studio пуста.".to_string() } else { text })
}

pub fn cancel(job_id: &str) -> Result<()> {
    let normalized = job_id.strip_suffix(".service").unwrap_or(job_id);
    let normalized = normalized.strip_prefix("media-studio-").unwrap_or(normalized);
    anyhow::ensure!(paths::valid_job_id(normalized), "некорректный идентификатор задачи");
    let unit = format!("media-studio-{normalized}.service");
    let output = Command::new(runtime::required("systemctl")?)
        .args(["--user", "stop", &unit])
        .output()
        .context("не удалось остановить задачу")?;
    if !output.status.success() {
        bail!("не удалось остановить {unit}: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    ui::notify("Media Studio", &format!("Задача {unit} остановлена."));
    Ok(())
}

fn new_job_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("{}-{}", nanos, std::process::id())
}
