use anyhow::{bail, Context, Result};
use std::process::{Command, Stdio};

use crate::model::Config;
use crate::runtime;

#[derive(Debug, Clone)]
pub struct AdvancedSelection {
    pub profile: String,
    pub target_size_mb: Option<u64>,
    pub overwrite: bool,
    pub hardware_fallback: bool,
}

fn notifications_enabled_for(headless: bool, display: bool, wayland: bool) -> bool {
    !headless && (display || wayland)
}

fn notifications_enabled() -> bool {
    let headless = std::env::var("MEDIA_STUDIO_HEADLESS")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);
    notifications_enabled_for(
        headless,
        std::env::var_os("DISPLAY").is_some(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
    )
}

pub fn notify(title: &str, body: &str) {
    if !notifications_enabled() {
        return;
    }
    if let Some(notify_send) = runtime::optional("notify-send") {
        let notify_status = Command::new(notify_send)
            .arg("--app-name=Media Studio")
            .arg(title)
            .arg(body)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if notify_status
            .map(|status| status.success())
            .unwrap_or(false)
        {
            return;
        }
    }
    if let Some(kdialog) = runtime::optional_usable("kdialog") {
        let _ = Command::new(kdialog)
            .args(["--title", title, "--passivepopup", body, "5"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

pub fn error(title: &str, body: &str) {
    eprintln!("{title}: {body}");
    notify(title, body);
}

pub fn choose_advanced(config: &Config) -> Result<AdvancedSelection> {
    if runtime::optional_usable("zenity").is_some() {
        return choose_with_zenity(config);
    }
    if runtime::optional_usable("kdialog").is_none() {
        bail!("не найден рабочий dialog backend: установите zenity или kdialog");
    }
    choose_with_kdialog(config)
}

fn choose_with_zenity(config: &Config) -> Result<AdvancedSelection> {
    let values = config
        .profiles
        .iter()
        .map(|(id, profile)| format!("{id} — {} / {}", profile.category, profile.label))
        .collect::<Vec<_>>();
    let first = values
        .first()
        .cloned()
        .unwrap_or_else(|| config.default_profile.clone());
    let profile_values = values.join("|");
    let output = Command::new(runtime::required("zenity")?)
        .args([
            "--forms",
            "--title",
            "Media Studio — расширенные настройки",
            "--text",
            "Выберите профиль и параметры задания",
            "--separator",
            "|",
            "--width",
            "720",
            "--height",
            "420",
            "--add-combo",
            "Профиль",
            "--combo-values",
            &profile_values,
            "--add-entry",
            "Целевой размер, МБ (пусто — без ограничения)",
            "--add-checkbox",
            "Перезаписывать существующий файл",
            "--add-checkbox",
            "Software fallback для VAAPI/NVENC",
        ])
        .output()
        .context("не удалось открыть расширенное окно Zenity")?;
    if !output.status.success() {
        bail!("Операция отменена: профиль не выбран.");
    }
    let fields = String::from_utf8_lossy(&output.stdout)
        .trim()
        .split('|')
        .map(str::to_string)
        .collect::<Vec<_>>();
    let selected = fields.first().cloned().unwrap_or(first);
    let profile = selected
        .split(" — ")
        .next()
        .unwrap_or(&selected)
        .to_string();
    if !config.profiles.contains_key(&profile) {
        bail!("Диалог вернул неизвестный профиль: {profile}");
    }
    let target_size_mb = fields
        .get(1)
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0);
    let overwrite = fields
        .get(2)
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let hardware_fallback = fields
        .get(3)
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(config.hardware_fallback);
    Ok(AdvancedSelection {
        profile,
        target_size_mb,
        overwrite,
        hardware_fallback,
    })
}

fn choose_with_kdialog(config: &Config) -> Result<AdvancedSelection> {
    let mut args = vec![
        "--title",
        "Media Studio — расширенные настройки",
        "--menu",
        "Выберите профиль конвертации",
    ];
    let mut owned = Vec::new();
    for (id, profile) in &config.profiles {
        owned.push(id.clone());
        owned.push(format!("{} — {}", profile.category, profile.label));
    }
    for value in &owned {
        args.push(value);
    }
    let output = Command::new(runtime::required("kdialog")?)
        .args(args)
        .output()
        .context("не удалось открыть диалог выбора профиля")?;
    if !output.status.success() {
        bail!("Операция отменена: профиль не выбран.");
    }
    let profile = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if profile.is_empty() || !config.profiles.contains_key(&profile) {
        bail!("Диалог вернул неизвестный профиль: {profile}");
    }
    let target_size_mb = kdialog_inputbox("Целевой размер, МБ (пусто — без ограничения)", "")?
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0);
    let overwrite = kdialog_yesno("Перезаписать существующие результаты?")?;
    let hardware_fallback = kdialog_yesno("Разрешить software fallback для VAAPI/NVENC?")?;
    Ok(AdvancedSelection {
        profile,
        target_size_mb,
        overwrite,
        hardware_fallback,
    })
}

fn kdialog_inputbox(prompt: &str, initial: &str) -> Result<String> {
    let output = Command::new(runtime::required("kdialog")?)
        .args([
            "--title",
            "Media Studio — расширенные настройки",
            "--inputbox",
            prompt,
            initial,
        ])
        .output()
        .context("не удалось открыть поле ввода KDialog")?;
    if !output.status.success() {
        bail!("Операция отменена пользователем.");
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn kdialog_yesno(prompt: &str) -> Result<bool> {
    let status = Command::new(runtime::required("kdialog")?)
        .args([
            "--title",
            "Media Studio — расширенные настройки",
            "--yesno",
            prompt,
        ])
        .status()
        .context("не удалось открыть подтверждение KDialog")?;
    if status.code() == Some(1) {
        return Ok(false);
    }
    if status.success() {
        return Ok(true);
    }
    bail!("Операция отменена пользователем.");
}

pub fn show_info(title: &str, body: &str) {
    if !notifications_enabled() {
        return;
    }
    if let Some(zenity) = runtime::optional_usable("zenity") {
        let _ = Command::new(zenity)
            .args(["--info", "--title", title, "--text", body, "--width", "760"])
            .status();
    } else if let Some(kdialog) = runtime::optional_usable("kdialog") {
        let _ = Command::new(kdialog)
            .args(["--title", title, "--msgbox", body])
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::notifications_enabled_for;

    #[test]
    fn headless_mode_always_disables_notifications() {
        assert!(!notifications_enabled_for(true, true, true));
        assert!(!notifications_enabled_for(true, true, false));
        assert!(!notifications_enabled_for(true, false, true));
    }

    #[test]
    fn graphical_session_enables_notifications() {
        assert!(notifications_enabled_for(false, true, false));
        assert!(notifications_enabled_for(false, false, true));
    }

    #[test]
    fn missing_graphical_session_disables_notifications() {
        assert!(!notifications_enabled_for(false, false, false));
    }
}
