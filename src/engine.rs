use anyhow::{bail, Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::model::{Config, HardwareBackend, Profile};

pub struct Converted {
    pub output: PathBuf,
    pub log: PathBuf,
}

pub fn convert(
    config: &Config,
    profile: &Profile,
    input: &Path,
    output: &Path,
    temp: &Path,
    log: &Path,
) -> Result<Converted> {
    if !input.is_file() {
        bail!("входной объект не является обычным файлом: {}", input.display());
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
    writeln!(log_file, "INPUT={}", input.display())?;
    writeln!(log_file, "OUTPUT={}", output.display())?;
    writeln!(log_file, "ENGINE={}", profile.engine)?;

    let status = match profile.engine.as_str() {
        "ffmpeg" => run_ffmpeg(config, profile, input, temp, &mut log_file),
        "magick" => run_magick(profile, input, temp, &mut log_file),
        other => bail!("неподдерживаемый engine: {other}"),
    }?;
    if !status.success() {
        bail!("конвертация завершилась с кодом {status}; подробности: {}", log.display());
    }
    if !temp.is_file() || fs::metadata(temp)?.len() == 0 {
        bail!("движок завершился успешно, но временный результат пуст: {}", temp.display());
    }
    if config.verify_results {
        verify(profile, temp, &mut log_file)?;
    }
    fs::rename(temp, output)
        .with_context(|| format!("не удалось атомарно сохранить {}", output.display()))?;
    writeln!(log_file, "RESULT=verified")?;
    Ok(Converted { output: output.to_path_buf(), log: log.to_path_buf() })
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
        writeln!(log, "TARGET_ATTEMPT={attempt} TARGET_MB={target_mb} VIDEO_KBPS={video_kbps:.0}")?;
        let status = run_ffmpeg_once(config, profile, input, temp, log, args)?;
        last_status = Some(status);
        if !status.success() {
            return Ok(status);
        }
        let actual = fs::metadata(temp).map(|meta| meta.len()).unwrap_or(u64::MAX);
        if actual <= target_bytes {
            writeln!(log, "TARGET_RESULT=ok ACTUAL_BYTES={actual}")?;
            return Ok(status);
        }
        let ratio = target_bytes as f64 / actual as f64;
        video_kbps = (video_kbps * ratio * 0.93).max(64.0);
        let _ = fs::remove_file(temp);
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
    let _ = fs::remove_file(temp);
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
    if hardware_available(backend) {
        writeln!(log, "HARDWARE={} selected", backend.label())?;
        return Ok(args);
    }
    if config.hardware_fallback && !profile.fallback_args.is_empty() {
        writeln!(log, "HARDWARE={} unavailable; using software fallback", backend.label())?;
        return Ok(if profile.target_size_mb.is_some() {
            preserve_rate_flags(&profile.fallback_args, &args)
        } else {
            profile.fallback_args.clone()
        });
    }
    bail!("аппаратный профиль {} недоступен", backend.label())
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

fn hardware_available(backend: &HardwareBackend) -> bool {
    match backend {
        HardwareBackend::Vaapi => fs::metadata("/dev/dri/renderD128")
            .map(|meta| meta.file_type().is_char_device())
            .unwrap_or(false),
        HardwareBackend::Nvenc => Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains("h264_nvenc"))
            .unwrap_or(false),
    }
}

fn invoke_ffmpeg(
    config: &Config,
    input: &Path,
    temp: &Path,
    log: &mut File,
    args: &[String],
) -> Result<std::process::ExitStatus> {
    let expanded = expanded_args(args, input, temp);
    let mut command = Command::new("ffmpeg");
    command
        .args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-i"])
        .arg(input)
        .args(["-filter_threads", "1", "-filter_complex_threads", "1", "-threads"])
        .arg(config.ffmpeg_threads.to_string())
        .args(expanded.iter().map(String::as_str));
    if !args.iter().any(|arg| arg == "{output}") {
        command.arg(temp);
    }
    writeln!(log, "COMMAND=ffmpeg {:?}", command)?;
    let log_out = log.try_clone()?;
    let log_err = log.try_clone()?;
    Ok(command.stdout(Stdio::from(log_out)).stderr(Stdio::from(log_err)).status()?)
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
    let output = Command::new("ffprobe")
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
    let mut command = Command::new("magick");
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
    Ok(command.stdout(Stdio::from(log_out)).stderr(Stdio::from(log_err)).status()?)
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
            let probe = Command::new("ffprobe")
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
            let decode = Command::new("ffmpeg")
                .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-xerror", "-i"])
                .arg(output)
                .args(["-map", "0:v?", "-map", "0:a?", "-f", "null", "-"])
                .output()
                .context("не удалось запустить полную проверку декодирования")?;
            log.write_all(&decode.stderr)?;
            if !decode.status.success() {
                bail!("полное декодирование результата завершилось с ошибкой");
            }
        },
        "magick" => {
            let check = Command::new("magick")
                .args(["identify", "-quiet"])
                .arg(output)
                .output()
                .context("не удалось проверить результат ImageMagick")?;
            log.write_all(&check.stderr)?;
            if !check.status.success() {
                bail!("ImageMagick не подтвердил результат: {}", output.display());
            }
        },
        _ => bail!("нельзя проверить неизвестный engine"),
    }
    Ok(())
}

pub fn inspect(input: &Path, json: bool) -> Result<String> {
    if !input.is_file() {
        bail!("файл не найден или это не обычный файл: {}", input.display());
    }
    if json {
        let output = Command::new("ffprobe")
            .args(["-v", "error", "-show_format", "-show_streams", "-of", "json"])
            .arg(input)
            .output()
            .context("не удалось запустить ffprobe")?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
    }
    let output = Command::new("ffprobe").args(["-v", "error", "-show_entries", "format=filename,format_name,duration,size:stream=index,codec_type,codec_name,width,height,channels,sample_rate", "-of", "default=noprint_wrappers=1"]).arg(input).output().context("не удалось запустить ffprobe")?;
    if !output.status.success() {
        let image = Command::new("magick")
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
    if which("zenity").is_none() && which("kdialog").is_none() {
        bail!("не найден zenity или kdialog для пользовательских окон");
    }
    for tool in tools {
        if which(tool).is_none() {
            bail!(
                "не найден обязательный инструмент `{tool}`. Установите пакет и повторите запуск."
            );
        }
    }
    Ok(())
}

fn which(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}
