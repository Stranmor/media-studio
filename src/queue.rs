use anyhow::{bail, Context, Result};
use std::env;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::runtime;
use crate::{paths, ui};

pub struct EnqueueRequest<'a> {
    pub executable: &'a Path,
    pub profile: &'a str,
    pub target_size_mb: Option<u64>,
    pub hardware_fallback: bool,
    pub vaapi_device: Option<&'a str>,
    pub output_dir: Option<std::path::PathBuf>,
    pub overwrite: bool,
    pub files: &'a [String],
}

pub fn enqueue(request: EnqueueRequest<'_>) -> Result<String> {
    if request.files.is_empty() {
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
        .args(inherited_environment_args())
        .args(vaapi_environment_arg(request.vaapi_device))
        .arg(request.executable)
        .args([
            "run",
            "--job-id",
            &job_id,
            "--profile",
            request.profile,
            "--hardware-fallback",
            &request.hardware_fallback.to_string(),
        ])
        .args(
            request
                .target_size_mb
                .map(|value| vec!["--target-size-mb".to_string(), value.to_string()])
                .unwrap_or_default(),
        )
        .args(
            request
                .output_dir
                .map(|value| vec!["--output-dir".to_string(), value.display().to_string()])
                .unwrap_or_default(),
        )
        .args(if request.overwrite {
            vec!["--overwrite".to_string()]
        } else {
            Vec::new()
        })
        .arg("--")
        .args(request.files)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let output = command
        .output()
        .context("не удалось поставить задачу в user-systemd очередь")?;
    if !output.status.success() {
        let details = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("systemd не принял задачу {job_id}: {details}");
    }
    ui::notify(
        "Media Studio",
        &format!(
            "Задача {job_id} добавлена в очередь: {} файл(ов)",
            request.files.len()
        ),
    );
    Ok(job_id)
}

pub fn list() -> Result<String> {
    let output = Command::new(runtime::required("systemctl")?)
        .args([
            "--user",
            "list-units",
            "--all",
            "--no-legend",
            "media-studio-*.service",
        ])
        .output()
        .context("не удалось прочитать очередь Media Studio")?;
    if !output.status.success() {
        bail!(
            "systemctl завершился с ошибкой: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| is_conversion_unit(line.split_whitespace().next().unwrap_or_default()))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(if text.is_empty() {
        "Очередь Media Studio пуста.".to_string()
    } else {
        text
    })
}

pub fn cancel(job_id: &str) -> Result<()> {
    let normalized = job_id.strip_suffix(".service").unwrap_or(job_id);
    let normalized = normalized
        .strip_prefix("media-studio-")
        .unwrap_or(normalized);
    anyhow::ensure!(
        !normalized.starts_with("watch-"),
        "watch-folder unit нельзя отменить через queue; используйте `media-studio watch stop --id ID`"
    );
    anyhow::ensure!(
        paths::valid_job_id(normalized),
        "некорректный идентификатор задачи"
    );
    let unit = format!("media-studio-{normalized}.service");
    let output = Command::new(runtime::required("systemctl")?)
        .args(["--user", "stop", &unit])
        .output()
        .context("не удалось остановить задачу")?;
    if !output.status.success() {
        bail!(
            "не удалось остановить {unit}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    ui::notify("Media Studio", &format!("Задача {unit} остановлена."));
    Ok(())
}

fn new_job_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{}", nanos, std::process::id())
}

fn is_conversion_unit(unit: &str) -> bool {
    unit.starts_with("media-studio-") && !unit.starts_with("media-studio-watch-")
}

fn vaapi_environment_arg(device: Option<&str>) -> Option<String> {
    device.map(|device| format!("--setenv=MEDIA_STUDIO_VAAPI_DEVICE={device}"))
}

fn inherited_environment_args() -> Vec<String> {
    ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME"]
        .into_iter()
        .filter_map(|key| env::var(key).ok().map(|value| format!("--setenv={key}={value}")))
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn queue_filter_excludes_watch_units() {
        assert!(super::is_conversion_unit("media-studio-123-456.service"));
        assert!(!super::is_conversion_unit(
            "media-studio-watch-camera.service"
        ));
    }

    #[test]
    fn queue_vaapi_environment_argument_is_exact() {
        assert_eq!(
            super::vaapi_environment_arg(Some("/dev/dri/renderD128")),
            Some("--setenv=MEDIA_STUDIO_VAAPI_DEVICE=/dev/dri/renderD128".to_string())
        );
        assert_eq!(super::vaapi_environment_arg(None), None);
    }

    #[test]
    fn queue_propagates_user_data_environment_keys() {
        let args = super::inherited_environment_args();
        for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME"] {
            if std::env::var_os(key).is_some() {
                assert!(args.iter().any(|arg| arg.starts_with(&format!("--setenv={key}="))));
            }
        }
    }
}
