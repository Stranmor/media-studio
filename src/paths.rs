use anyhow::{Context, Result};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::model::Profile;

pub fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub fn config_path() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("media-studio/config.toml")
}

pub fn state_dir() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local/state"))
        .join("media-studio")
}

pub fn installed_binary_path() -> PathBuf {
    home_dir().join(".local/bin/media-studio")
}

pub fn service_menu_dir() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local/share"))
        .join("kio/servicemenus")
}

pub fn service_menu_paths() -> Vec<PathBuf> {
    ["media-studio-video.desktop", "media-studio-audio.desktop", "media-studio-image.desktop"]
        .into_iter()
        .map(|name| service_menu_dir().join(name))
        .collect()
}

pub fn legacy_service_menu_path() -> PathBuf {
    service_menu_dir().join("media-studio.desktop")
}

pub fn systemd_user_dir() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("systemd/user")
}

pub fn watch_service_path(id: &str) -> PathBuf {
    systemd_user_dir().join(format!("media-studio-watch-{id}.service"))
}

pub fn valid_job_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub fn normalize_path_arg(raw: &str) -> PathBuf {
    let decoded =
        raw.strip_prefix("file://").map(|rest| rest.strip_prefix("localhost").unwrap_or(rest));
    match decoded {
        Some(value) => PathBuf::from(percent_decode(value)),
        None => PathBuf::from(raw),
    }
}

pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                output.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn output_path(
    input: &Path,
    profile: &Profile,
    output_dir: Option<&Path>,
    overwrite: bool,
) -> PathBuf {
    let parent = output_dir.unwrap_or_else(|| input.parent().unwrap_or_else(|| Path::new(".")));
    let stem = input.file_stem().and_then(OsStr::to_str).unwrap_or("media");
    let source_ext =
        input.extension().and_then(OsStr::to_str).unwrap_or("bin").to_ascii_lowercase();
    let extension =
        if profile.extension.is_empty() { source_ext.clone() } else { profile.extension.clone() };
    let primary = parent.join(format!("{stem}.{extension}"));
    if !overwrite && primary != input && !primary.exists() {
        return primary;
    }
    if overwrite && primary != input {
        return primary;
    }
    let suffix = if profile.suffix.is_empty()
        || profile.suffix.eq_ignore_ascii_case(&extension)
        || profile.suffix.eq_ignore_ascii_case(&source_ext)
    {
        "converted"
    } else {
        &profile.suffix
    };
    for index in 1..10_000u32 {
        let candidate = parent.join(format!("{stem}.{suffix}-{index}.{extension}"));
        if overwrite || !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem}.{suffix}-overflow.{extension}"))
}

pub fn job_log_path(job_id: &str) -> PathBuf {
    state_dir().join("jobs").join(format!("{job_id}.log"))
}

pub fn job_status_path(job_id: &str) -> PathBuf {
    state_dir().join("jobs").join(format!("{job_id}.status"))
}

pub fn write_job_status(job_id: &str, state: &str, details: &[(&str, String)]) -> Result<()> {
    let path = job_status_path(job_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut body = format!("state={state}\n");
    for (key, value) in details {
        body.push_str(key);
        body.push('=');
        body.push_str(value);
        body.push('\n');
    }
    atomic_write(&path, body.as_bytes())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temp, bytes)
        .with_context(|| format!("не удалось записать временный файл {}", temp.display()))?;
    fs::rename(&temp, path).with_context(|| format!("не удалось заменить {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Config;

    #[test]
    fn file_url_is_decoded() {
        assert_eq!(
            normalize_path_arg("file:///tmp/with%20space.mkv"),
            PathBuf::from("/tmp/with space.mkv")
        );
    }

    #[test]
    fn output_path_does_not_equal_input() {
        let profile = Config::built_in().profiles.get("video_mp4").cloned().expect("profile");
        let input = PathBuf::from("/tmp/example.mp4");
        assert_ne!(output_path(&input, &profile, None, false), input);
    }
}
