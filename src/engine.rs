use anyhow::{bail, Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::{Builder, NamedTempFile};

use crate::model::{Config, HardwareBackend, Profile};
use crate::runtime;

pub struct Converted {
    pub output: PathBuf,
    pub log: PathBuf,
}

pub fn convert(
    config: &Config,
    profile: &Profile,
    input: &Path,
    output: &Path,
    overwrite: bool,
    log: &Path,
) -> Result<Converted> {
    if !input.is_file() {
        bail!(
            "входной объект не является обычным файлом: {}",
            input.display()
        );
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = log.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .with_context(|| format!("не удалось открыть лог {}", log.display()))?;
    let extension = output
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("tmp");
    let temp = Builder::new()
        .prefix(".media-studio-")
        .suffix(&format!(".part.{extension}"))
        .tempfile_in(output.parent().unwrap_or_else(|| Path::new(".")))?;
    let temp_path = temp.path().to_path_buf();
    writeln!(log_file, "INPUT={}", input.display())?;
    writeln!(log_file, "OUTPUT_REQUESTED={}", output.display())?;
    writeln!(log_file, "ENGINE={}", profile.engine)?;

    let status = match profile.engine.as_str() {
        "ffmpeg" => run_ffmpeg(config, profile, input, &temp_path, &mut log_file),
        "magick" => run_magick(profile, input, &temp_path, &mut log_file),
        other => bail!("неподдерживаемый engine: {other}"),
    }?;
    if !status.success() {
        bail!(
            "конвертация завершилась с кодом {status}; подробности: {}",
            log.display()
        );
    }
    if !temp_path.is_file() || fs::metadata(&temp_path)?.len() == 0 {
        bail!(
            "движок завершился успешно, но временный результат пуст: {}",
            temp_path.display()
        );
    }
    if config.verify_results {
        verify(profile, &temp_path, &mut log_file)?;
    }
    let actual_output = persist_output(temp, output, overwrite)?;
    writeln!(log_file, "OUTPUT_COMMITTED={}", actual_output.display())?;
    writeln!(log_file, "RESULT=verified")?;
    Ok(Converted {
        output: actual_output,
        log: log.to_path_buf(),
    })
}

fn persist_output(temp: NamedTempFile, requested: &Path, overwrite: bool) -> Result<PathBuf> {
    if overwrite {
        temp.persist(requested).map_err(|error| {
            anyhow::anyhow!(
                "не удалось атомарно заменить {}: {}",
                requested.display(),
                error.error
            )
        })?;
        return Ok(requested.to_path_buf());
    }
    let mut temp = temp;
    for index in 0..10_000u32 {
        let candidate = if index == 0 {
            requested.to_path_buf()
        } else {
            let stem = requested
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("output");
            let extension = requested
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("bin");
            requested
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(format!("{stem}-{index}.{extension}"))
        };
        match temp.persist_noclobber(&candidate) {
            Ok(_) => return Ok(candidate),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                temp = error.file;
            }
            Err(error) => {
                return Err(anyhow::anyhow!(
                    "не удалось атомарно сохранить {}: {}",
                    candidate.display(),
                    error.error
                ));
            }
        }
    }
    bail!(
        "не удалось зарезервировать свободное имя результата рядом с {}",
        requested.display()
    )
}

fn run_ffmpeg(
    config: &Config,
    profile: &Profile,
    input: &Path,
    temp: &Path,
    log: &mut File,
) -> Result<std::process::ExitStatus> {
    if profile.target_size_mb.is_some() {
        return run_ffmpeg_target(config, profile, input, temp, log);
    }
    run_ffmpeg_once(config, profile, input, temp, log, profile.args.clone())
}

fn run_ffmpeg_target(
    config: &Config,
    profile: &Profile,
    input: &Path,
    temp: &Path,
    log: &mut File,
) -> Result<std::process::ExitStatus> {
    let target_mb = profile.target_size_mb.expect("target profile");
    let duration = probe_duration(input)?;
    if duration <= 0.0 {
        bail!("не удалось определить длительность для расчёта целевого размера");
    }
    let target_bytes = target_mb.saturating_mul(1_000_000);
    let audio_kbps = if profile.target_audio_bitrate_kbps == 0 {
        128
    } else {
        profile.target_audio_bitrate_kbps
    } as f64;
    let total_kbps = (target_bytes as f64 * 8.0 / duration / 1000.0 * 0.965).max(audio_kbps + 96.0);
    let mut video_kbps = (total_kbps - audio_kbps - 24.0).max(96.0);
    let mut last_status = None;
    for attempt in 1..=5 {
        let args = target_args(&profile.args, video_kbps, audio_kbps);
        writeln!(
            log,
            "TARGET_ATTEMPT={attempt} TARGET_MB={target_mb} VIDEO_KBPS={video_kbps:.0}"
        )?;
        let status = run_ffmpeg_once(config, profile, input, temp, log, args)?;
        last_status = Some(status);
        if !status.success() {
            return Ok(status);
        }
        let actual = fs::metadata(temp)
            .map(|meta| meta.len())
            .unwrap_or(u64::MAX);
        if actual <= target_bytes {
            writeln!(log, "TARGET_RESULT=ok ACTUAL_BYTES={actual}")?;
            return Ok(status);
        }
        let ratio = target_bytes as f64 / actual as f64;
        video_kbps = (video_kbps * ratio * 0.93).max(64.0);
    }
    let actual = fs::metadata(temp).map(|meta| meta.len()).unwrap_or(0);
    if actual > target_bytes {
        bail!(
            "не удалось уложить результат в {target_mb} МБ за 5 проходов (получилось {:.1} МБ)",
            actual as f64 / 1_000_000.0
        );
    }
    Ok(last_status.expect("target attempts"))
}

fn run_ffmpeg_once(
    config: &Config,
    profile: &Profile,
    input: &Path,
    temp: &Path,
    log: &mut File,
    args: Vec<String>,
) -> Result<std::process::ExitStatus> {
    let selected_args = select_hardware_args(config, profile, args, log)?;
    let status = invoke_ffmpeg(config, input, temp, log, &selected_args)?;
    if status.success()
        || profile.hardware.is_none()
        || !config.hardware_fallback
        || profile.fallback_args.is_empty()
    {
        return Ok(status);
    }
    writeln!(log, "HARDWARE_FALLBACK=software AFTER_STATUS={status}")?;
    invoke_ffmpeg(config, input, temp, log, &profile.fallback_args)
}

fn select_hardware_args(
    config: &Config,
    profile: &Profile,
    args: Vec<String>,
    log: &mut File,
) -> Result<Vec<String>> {
    let Some(backend) = profile.hardware.as_ref() else {
        return Ok(args);
    };
    if let Some(device) = hardware_device(config, backend) {
        let selected = if matches!(backend, HardwareBackend::Vaapi) {
            replace_placeholder(&args, "{vaapi_device}", &device)
        } else {
            args
        };
        writeln!(log, "HARDWARE={} selected DEVICE={device}", backend.label())?;
        return Ok(selected);
    }
    if config.hardware_fallback && !profile.fallback_args.is_empty() {
        writeln!(
            log,
            "HARDWARE={} unavailable; using software fallback",
            backend.label()
        )?;
        return Ok(if profile.target_size_mb.is_some() {
            preserve_rate_flags(&profile.fallback_args, &args)
        } else {
            profile.fallback_args.clone()
        });
    }
    bail!(
        "аппаратный профиль {} недоступен: проверьте encoder и устройство; включите hardware_fallback или исправьте конфигурацию",
        backend.label()
    )
}

fn preserve_rate_flags(base: &[String], requested: &[String]) -> Vec<String> {
    let mut output = base.to_vec();
    for flag in ["-b:v", "-maxrate", "-bufsize", "-b:a"] {
        if let Some(index) = requested.iter().position(|arg| arg == flag) {
            if let Some(value) = requested.get(index + 1) {
                output.push(flag.to_string());
                output.push(value.clone());
            }
        }
    }
    output
}

fn hardware_device(config: &Config, backend: &HardwareBackend) -> Option<String> {
    match backend {
        HardwareBackend::Vaapi => {
            let device = std::env::var_os("MEDIA_STUDIO_VAAPI_DEVICE")
                .map(PathBuf::from)
                .or_else(|| config.vaapi_device.clone().map(PathBuf::from))
                .or_else(discover_vaapi_device)?;
            if !is_render_node(&device) || !ffmpeg_encoder_available("h264_vaapi") {
                return None;
            }
            Some(device.display().to_string())
        }
        HardwareBackend::Nvenc => {
            ffmpeg_encoder_available("h264_nvenc").then(|| "nvidia:nvenc".to_string())
        }
    }
}

pub fn hardware_diagnostics(config: &Config) -> Vec<String> {
    let vaapi_device = std::env::var_os("MEDIA_STUDIO_VAAPI_DEVICE")
        .map(PathBuf::from)
        .or_else(|| config.vaapi_device.clone().map(PathBuf::from))
        .or_else(discover_vaapi_device);
    let vaapi = match vaapi_device {
        Some(device) if is_render_node(&device) && ffmpeg_encoder_available("h264_vaapi") => {
            format!("vaapi: OK ({})", device.display())
        }
        Some(device) => format!(
            "vaapi: MISSING (device or h264_vaapi unavailable: {})",
            device.display()
        ),
        None => "vaapi: MISSING (render node or h264_vaapi unavailable)".to_string(),
    };
    let nvenc = if ffmpeg_encoder_available("h264_nvenc") {
        "nvenc: OK (h264_nvenc)".to_string()
    } else {
        "nvenc: MISSING (h264_nvenc unavailable)".to_string()
    };
    vec![vaapi, nvenc]
}

fn discover_vaapi_device() -> Option<PathBuf> {
    let mut devices = fs::read_dir("/dev/dri")
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("renderD"))
                .unwrap_or(false)
        })
        .filter(|path| is_render_node(path))
        .collect::<Vec<_>>();
    devices.sort();
    devices.into_iter().next()
}

fn is_render_node(path: &Path) -> bool {
    fs::metadata(path)
        .map(|meta| meta.file_type().is_char_device())
        .unwrap_or(false)
}

fn ffmpeg_encoder_available(encoder: &str) -> bool {
    let Some(ffmpeg) = runtime::optional("ffmpeg") else {
        return false;
    };
    let Ok(output) = Command::new(ffmpeg)
        .args(["-hide_banner", "-encoders"])
        .output()
    else {
        return false;
    };
    let listing = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_encoder_listing(&listing, encoder)
}

fn parse_encoder_listing(listing: &str, encoder: &str) -> bool {
    listing
        .lines()
        .any(|line| line.split_whitespace().any(|token| token == encoder))
}

fn replace_placeholder(args: &[String], placeholder: &str, value: &str) -> Vec<String> {
    args.iter()
        .map(|arg| {
            if arg == placeholder {
                value.to_string()
            } else {
                arg.clone()
            }
        })
        .collect()
}

fn invoke_ffmpeg(
    config: &Config,
    input: &Path,
    temp: &Path,
    log: &mut File,
    args: &[String],
) -> Result<std::process::ExitStatus> {
    let expanded = expanded_args(args, input, temp);
    let mut command = Command::new(runtime::required("ffmpeg")?);
    command
        .args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-i"])
        .arg(input)
        .args([
            "-filter_threads",
            "1",
            "-filter_complex_threads",
            "1",
            "-threads",
        ])
        .arg(config.ffmpeg_threads.to_string())
        .args(expanded.iter().map(String::as_str));
    if !args.iter().any(|arg| arg == "{output}") {
        command.arg(temp);
    }
    writeln!(log, "COMMAND=ffmpeg {:?}", command)?;
    let log_out = log.try_clone()?;
    let log_err = log.try_clone()?;
    Ok(command
        .stdout(Stdio::from(log_out))
        .stderr(Stdio::from(log_err))
        .status()?)
}

fn target_args(base: &[String], video_kbps: f64, audio_kbps: f64) -> Vec<String> {
    let mut output = Vec::with_capacity(base.len() + 8);
    let mut index = 0;
    while index < base.len() {
        let arg = &base[index];
        if matches!(arg.as_str(), "-crf" | "-cq" | "-qp") {
            index += 2;
            continue;
        }
        if arg == "-b:v" {
            index += 2;
            continue;
        }
        if arg == "-b:a" {
            output.push(arg.clone());
            output.push(format!("{audio_kbps:.0}k"));
            index += 2;
            continue;
        }
        output.push(arg.clone());
        index += 1;
    }
    output.extend([
        "-b:v".to_string(),
        format!("{video_kbps:.0}k"),
        "-maxrate".to_string(),
        format!("{video_kbps:.0}k"),
        "-bufsize".to_string(),
        format!("{:.0}k", video_kbps * 2.0),
    ]);
    output
}

fn probe_duration(input: &Path) -> Result<f64> {
    let output = Command::new(runtime::required("ffprobe")?)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(input)
        .output()
        .context("не удалось определить длительность через ffprobe")?;
    if !output.status.success() {
        bail!("ffprobe не смог определить длительность");
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .context("ffprobe вернул некорректную длительность")
}

fn run_magick(
    profile: &Profile,
    input: &Path,
    temp: &Path,
    log: &mut File,
) -> Result<std::process::ExitStatus> {
    let args = expanded_args(&profile.args, input, temp);
    let mut command = Command::new(runtime::required("magick")?);
    if !profile.args.iter().any(|arg| arg == "{input}") {
        command.arg(input);
    }
    command.args(args.iter().map(String::as_str));
    if !profile.args.iter().any(|arg| arg == "{output}") {
        command.arg(temp);
    }
    writeln!(log, "COMMAND=magick {:?}", command)?;
    let log_out = log.try_clone()?;
    let log_err = log.try_clone()?;
    Ok(command
        .stdout(Stdio::from(log_out))
        .stderr(Stdio::from(log_err))
        .status()?)
}

fn expanded_args(args: &[String], input: &Path, output: &Path) -> Vec<String> {
    args.iter()
        .map(|arg| {
            arg.replace("{input}", &input.to_string_lossy())
                .replace("{output}", &output.to_string_lossy())
        })
        .collect()
}

fn verify(profile: &Profile, output: &Path, log: &mut File) -> Result<()> {
    match profile.engine.as_str() {
        "ffmpeg" => {
            let probe = Command::new(runtime::required("ffprobe")?)
                .args([
                    "-v",
                    "error",
                    "-show_entries",
                    "format=duration,size",
                    "-of",
                    "default=noprint_wrappers=1",
                ])
                .arg(output)
                .output()
                .context("не удалось запустить ffprobe для проверки")?;
            log.write_all(&probe.stderr)?;
            if !probe.status.success() || String::from_utf8_lossy(&probe.stdout).trim().is_empty() {
                bail!("ffprobe не подтвердил результат: {}", output.display());
            }
            let decode = Command::new(runtime::required("ffmpeg")?)
                .args([
                    "-hide_banner",
                    "-nostdin",
                    "-loglevel",
                    "error",
                    "-xerror",
                    "-i",
                ])
                .arg(output)
                .args(["-map", "0:v?", "-map", "0:a?", "-f", "null", "-"])
                .output()
                .context("не удалось запустить полную проверку декодирования")?;
            log.write_all(&decode.stderr)?;
            if !decode.status.success() {
                bail!("полное декодирование результата завершилось с ошибкой");
            }
        }
        "magick" => {
            let check = Command::new(runtime::required("magick")?)
                .args(["identify", "-quiet"])
                .arg(output)
                .output()
                .context("не удалось проверить результат ImageMagick")?;
            log.write_all(&check.stderr)?;
            if !check.status.success() {
                bail!("ImageMagick не подтвердил результат: {}", output.display());
            }
        }
        _ => bail!("нельзя проверить неизвестный engine"),
    }
    Ok(())
}

pub fn inspect(input: &Path, json: bool) -> Result<String> {
    if !input.is_file() {
        bail!(
            "файл не найден или это не обычный файл: {}",
            input.display()
        );
    }
    if json {
        let output = Command::new(runtime::required("ffprobe")?)
            .args([
                "-v",
                "error",
                "-show_format",
                "-show_streams",
                "-of",
                "json",
            ])
            .arg(input)
            .output()
            .context("не удалось запустить ffprobe")?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
    }
    let output = Command::new(runtime::required("ffprobe")?).args(["-v", "error", "-show_entries", "format=filename,format_name,duration,size:stream=index,codec_type,codec_name,width,height,channels,sample_rate", "-of", "default=noprint_wrappers=1"]).arg(input).output().context("не удалось запустить ffprobe")?;
    if !output.status.success() {
        let image = Command::new(runtime::required("magick")?)
            .args(["identify", "-format", "%m %wx%h %[channels]"])
            .arg(input)
            .output()?;
        if image.status.success() {
            return Ok(String::from_utf8_lossy(&image.stdout).into_owned());
        }
        bail!("не удалось определить формат {}", input.display());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn ensure_tools(profile: Option<&Profile>) -> Result<()> {
    let mut tools = vec!["ffmpeg", "ffprobe", "systemd-run"];
    if profile.map(|item| item.engine == "magick").unwrap_or(true) {
        tools.push("magick");
    }
    if runtime::optional("zenity").is_none() && runtime::optional("kdialog").is_none() {
        bail!("не найден zenity или kdialog для пользовательских окон");
    }
    for tool in tools {
        if runtime::optional(tool).is_none() {
            bail!(
                "не найден обязательный инструмент `{tool}`. Установите пакет и повторите запуск."
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoder_listing_requires_exact_encoder_token() {
        let listing = " V..... h264_vaapi           H.264/AVC (VAAPI)\n V..... h264_nvenc           NVIDIA NVENC H.264 encoder";
        assert!(parse_encoder_listing(listing, "h264_vaapi"));
        assert!(parse_encoder_listing(listing, "h264_nvenc"));
        assert!(!parse_encoder_listing(listing, "h264"));
    }

    #[test]
    fn placeholder_replacement_is_exact() {
        let args = vec![
            "-vaapi_device".to_string(),
            "{vaapi_device}".to_string(),
            "{vaapi_device}-suffix".to_string(),
        ];
        let replaced = replace_placeholder(&args, "{vaapi_device}", "/dev/dri/renderD129");
        assert_eq!(replaced[1], "/dev/dri/renderD129");
        assert_eq!(replaced[2], "{vaapi_device}-suffix");
    }
}
