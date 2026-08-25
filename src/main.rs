mod engine;
mod model;
mod paths;
mod queue;
mod runtime;
mod ui;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use model::{valid_watch_id, Config, WatchFolder};

#[derive(Debug, Parser)]
#[command(name = "media-studio", version, about = "Media Studio — конвертация медиа из Dolphin")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "Установить бинарник, Dolphin-меню и watch-units")]
    Install {
        #[arg(long)]
        force_config: bool,
    },
    #[command(about = "Удалить установленные файлы; --purge удаляет config и историю")]
    Uninstall {
        #[arg(long)]
        purge: bool,
    },
    #[command(about = "Проверить зависимости и текущую конфигурацию")]
    Doctor,
    #[command(about = "Открыть расширенный выбор профиля и поставить задания в очередь")]
    Choose {
        #[arg(trailing_var_arg = true)]
        files: Vec<String>,
    },
    #[command(about = "Поставить файлы в user-systemd очередь по профилю")]
    Enqueue {
        #[arg(short, long)]
        profile: String,
        #[arg(long)]
        target_size_mb: Option<u64>,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        hardware_fallback: bool,
        #[arg(trailing_var_arg = true)]
        files: Vec<String>,
    },
    #[command(hide = true)]
    Run {
        #[arg(long)]
        job_id: String,
        #[arg(short, long)]
        profile: String,
        #[arg(long)]
        overwrite: bool,
        #[arg(long)]
        target_size_mb: Option<u64>,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        hardware_fallback: bool,
        #[arg(long)]
        output_dir: Option<PathBuf>,
        #[arg(trailing_var_arg = true)]
        files: Vec<String>,
    },
    #[command(about = "Показать техническую информацию о медиафайле")]
    Inspect {
        #[arg(long)]
        json: bool,
        #[arg(trailing_var_arg = true)]
        files: Vec<String>,
    },
    #[command(about = "Показать или отменить задания")]
    Queue {
        #[command(subcommand)]
        command: QueueCommand,
    },
    #[command(about = "Настроить автоматическую обработку каталогов")]
    Watch {
        #[command(subcommand)]
        command: WatchCommand,
    },
    #[command(hide = true)]
    WatchRun {
        #[arg(long)]
        id: String,
    },
    #[command(about = "Показать доступные профили с понятными названиями")]
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
}

#[derive(Debug, Subcommand)]
enum QueueCommand {
    List,
    Cancel { job_id: String },
}

#[derive(Debug, Subcommand)]
enum WatchCommand {
    List,
    Add {
        #[arg(long)]
        id: String,
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        destination: PathBuf,
        #[arg(short, long)]
        profile: String,
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        recursive: bool,
        #[arg(long, default_value_t = 3)]
        settle_seconds: u64,
    },
    Remove {
        #[arg(long)]
        id: String,
    },
    Start {
        #[arg(long)]
        id: String,
    },
    Stop {
        #[arg(long)]
        id: String,
    },
}

#[derive(Debug, Subcommand)]
enum ProfileCommand {
    List,
}

fn main() {
    if let Err(error) = run() {
        ui::error("Media Studio: ошибка", &format_error(&error));
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Install { force_config } => install(force_config),
        Commands::Uninstall { purge } => uninstall(purge),
        Commands::Doctor => doctor(),
        Commands::Choose { files } => {
            let config = load_config()?;
            let choice = ui::choose_advanced(&config)?;
            enqueue_with_profile(
                &config,
                &choice.profile,
                choice.target_size_mb,
                choice.hardware_fallback,
                None,
                choice.overwrite,
                files,
            )
        },
        Commands::Enqueue { profile, target_size_mb, hardware_fallback, files } => {
            let config = load_config()?;
            require_profile(&config, &profile)?;
            enqueue_with_profile(
                &config,
                &profile,
                target_size_mb,
                hardware_fallback,
                None,
                false,
                files,
            )
        },
        Commands::Run {
            job_id,
            profile,
            overwrite,
            target_size_mb,
            hardware_fallback,
            output_dir,
            files,
        } => run_job(
            &job_id,
            &profile,
            overwrite,
            target_size_mb,
            hardware_fallback,
            output_dir,
            files,
        ),
        Commands::Inspect { json, files } => inspect_files(json, files),
        Commands::Queue { command } => match command {
            QueueCommand::List => {
                println!("{}", queue::list()?);
                Ok(())
            },
            QueueCommand::Cancel { job_id } => {
                queue::cancel(&job_id)?;
                Ok(())
            },
        },
        Commands::Watch { command } => run_watch_command(command),
        Commands::WatchRun { id } => watch_run(&id),
        Commands::Profile { command: ProfileCommand::List } => {
            let config = load_config()?;
            for (id, profile) in config.profiles {
                println!(
                    "{id}\t{}\t{}\tengine={}\text=.{}",
                    profile.category, profile.label, profile.engine, profile.extension
                );
            }
            Ok(())
        },
    }
}

fn load_config() -> Result<Config> {
    Config::load(&paths::config_path())
}

fn enqueue_with_profile(
    config: &Config,
    profile: &str,
    target_size_mb: Option<u64>,
    hardware_fallback: bool,
    output_dir: Option<PathBuf>,
    overwrite: bool,
    files: Vec<String>,
) -> Result<()> {
    require_profile(config, profile)?;
    let mut selected_profile = config.profiles.get(profile).expect("validated profile").clone();
    if target_size_mb.is_some() {
        selected_profile.target_size_mb = target_size_mb;
    }
    engine::ensure_tools(Some(&selected_profile))?;
    let executable = paths::installed_binary_path();
    let executable = if executable.is_file() { executable } else { std::env::current_exe()? };
    let job_id = queue::enqueue(
        &executable,
        profile,
        target_size_mb,
        hardware_fallback,
        output_dir,
        overwrite,
        &files,
    )?;
    println!("QUEUED job_id={job_id} profile={profile} files={}", files.len());
    Ok(())
}

fn run_job(
    job_id: &str,
    profile_id: &str,
    overwrite: bool,
    target_size_mb: Option<u64>,
    hardware_fallback: bool,
    output_dir: Option<PathBuf>,
    raw_files: Vec<String>,
) -> Result<()> {
    anyhow::ensure!(paths::valid_job_id(job_id), "некорректный идентификатор задачи");
    let config = load_config()?;
    require_profile(&config, profile_id)?;
    if raw_files.is_empty() {
        bail!("задача {job_id} не содержит входных файлов");
    }
    let mut profile = config.profiles.get(profile_id).expect("validated profile").clone();
    if target_size_mb.is_some() {
        profile.target_size_mb = target_size_mb;
    }
    let mut runtime_config = config.clone();
    runtime_config.hardware_fallback = hardware_fallback;
    engine::ensure_tools(Some(&profile))?;
    let log = paths::job_log_path(job_id);
    if let Some(parent) = log.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&log, format!("JOB_ID={job_id}\nPROFILE={profile_id}\nLABEL={}\n", profile.label))?;
    paths::write_job_status(job_id, "running", &[("profile", profile_id.to_string())])?;
    ui::notify("Media Studio", &format!("Запущена задача {job_id}: {}", profile.label));

    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut outputs = Vec::new();
    for raw in raw_files.iter() {
        let input = paths::normalize_path_arg(raw);
        let output = paths::output_path(&input, &profile, output_dir.as_deref(), overwrite);
        let details = [
            ("profile", profile_id.to_string()),
            ("input", input.display().to_string()),
            ("output", output.display().to_string()),
        ];
        paths::write_job_status(job_id, "running", &details)?;
        match engine::convert(&runtime_config, &profile, &input, &output, overwrite, &log) {
            Ok(result) => {
                ok += 1;
                outputs.push(result.output.clone());
                println!(
                    "OK {} -> {} (лог: {})",
                    input.display(),
                    result.output.display(),
                    result.log.display()
                );
            },
            Err(error) => {
                failed += 1;
                eprintln!(
                    "FAIL {}: {} (лог: {})",
                    input.display(),
                    format_error(&error),
                    log.display()
                );
            },
        }
    }
    let state = if failed == 0 {
        "completed"
    } else if ok > 0 {
        "partial"
    } else {
        "failed"
    };
    paths::write_job_status(
        job_id,
        state,
        &[
            ("profile", profile_id.to_string()),
            ("ok", ok.to_string()),
            ("failed", failed.to_string()),
            ("log", log.display().to_string()),
            (
                "hardware_fallback",
                fs::read_to_string(&log)
                    .unwrap_or_default()
                    .contains("HARDWARE_FALLBACK=")
                    .to_string(),
            ),
        ],
    )?;
    if failed == 0 {
        let fallback_note =
            if fs::read_to_string(&log).unwrap_or_default().contains("HARDWARE_FALLBACK=") {
                " Аппаратное ускорение недоступно, использован software fallback."
            } else {
                ""
            };
        let output_note = outputs
            .first()
            .map(|output| format!(" Первый результат: {}.", output.display()))
            .unwrap_or_default();
        ui::notify(
            "Media Studio: готово",
            &format!(
                "{}: обработано {} файл(ов).{}{} Лог: {}",
                profile.label,
                ok,
                output_note,
                fallback_note,
                log.display()
            ),
        );
        Ok(())
    } else {
        ui::error(
            "Media Studio: частичный результат",
            &format!("Успешно: {ok}, ошибок: {failed}. Подробности: {}", log.display()),
        );
        bail!("задача завершена с ошибками: успешно {ok}, ошибок {failed}")
    }
}

fn inspect_files(json: bool, raw_files: Vec<String>) -> Result<()> {
    if raw_files.is_empty() {
        bail!("выберите хотя бы один файл для инспекции");
    }
    let mut dialog = String::new();
    for raw in raw_files {
        let input = paths::normalize_path_arg(&raw);
        let body = engine::inspect(&input, json)?;
        if json {
            println!("{}", body);
        } else {
            println!("{}\n{}", input.display(), body.trim());
            println!();
            dialog.push_str(&format!("{}\n{}\n\n", input.display(), body.trim()));
        }
    }
    if !json
        && (std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some())
    {
        ui::show_info("Media Studio — информация о файле", &dialog);
    }
    Ok(())
}

fn require_profile(config: &Config, profile: &str) -> Result<()> {
    if config.profiles.contains_key(profile) {
        return Ok(());
    }
    let available = config.profiles.keys().cloned().collect::<Vec<_>>().join(", ");
    bail!("неизвестный профиль `{profile}`. Доступны: {available}")
}

fn run_watch_command(command: WatchCommand) -> Result<()> {
    match command {
        WatchCommand::List => {
            let config = load_config()?;
            if config.watch_folders.is_empty() {
                println!("Watch-folders не настроены.");
            } else {
                for folder in config.watch_folders {
                    println!(
                        "{}\t{} -> {}\tprofile={}\tenabled={}\trecursive={}",
                        folder.id,
                        folder.source,
                        folder.destination,
                        folder.profile,
                        folder.enabled,
                        folder.recursive
                    );
                }
            }
            Ok(())
        },
        WatchCommand::Add { id, source, destination, profile, recursive, settle_seconds } => {
            ensure_watch_id(&id)?;
            let mut config = load_config()?;
            let previous = config.clone();
            require_profile(&config, &profile)?;
            let source = fs::canonicalize(&source)
                .with_context(|| format!("каталог источника не найден: {}", source.display()))?;
            if !source.is_dir() {
                bail!("каталог источника не найден: {}", source.display());
            }
            fs::create_dir_all(&destination)?;
            let destination = fs::canonicalize(&destination)?;
            config.watch_folders.retain(|folder| folder.id != id);
            config.watch_folders.push(WatchFolder {
                id: id.clone(),
                source: source.display().to_string(),
                destination: destination.display().to_string(),
                profile,
                recursive,
                enabled: true,
                settle_seconds,
            });
            config.validate()?;
            config.write(&paths::config_path())?;
            if let Err(error) = install_watch_unit(
                config.watch_folders.iter().find(|folder| folder.id == id).expect("watch folder"),
            ) {
                previous.write(&paths::config_path())?;
                let _ = fs::remove_file(paths::watch_service_path(&id));
                return Err(error);
            }
            println!("WATCH_ADDED id={id} state=enabled");
            Ok(())
        },
        WatchCommand::Remove { id } => {
            ensure_watch_id(&id)?;
            let mut config = load_config()?;
            if !config.watch_folders.iter().any(|folder| folder.id == id) {
                bail!("watch-folder не найден: {id}");
            }
            config.watch_folders.retain(|folder| folder.id != id);
            config.write(&paths::config_path())?;
            let unit = format!("media-studio-watch-{id}.service");
            let _ =
                required_command("systemctl")?.args(["--user", "disable", "--now", &unit]).status();
            let _ = fs::remove_file(paths::watch_service_path(&id));
            let _ = required_command("systemctl")?.args(["--user", "daemon-reload"]).status();
            println!("WATCH_REMOVED id={id}");
            Ok(())
        },
        WatchCommand::Start { id } => {
            ensure_watch_id(&id)?;
            let mut config = load_config()?;
            let index = config
                .watch_folders
                .iter()
                .position(|folder| folder.id == id)
                .with_context(|| format!("watch-folder не найден: {id}"))?;
            config.watch_folders[index].enabled = true;
            let folder = config.watch_folders[index].clone();
            install_watch_unit(&folder)?;
            config.write(&paths::config_path())?;
            println!("WATCH_STARTED id={id}");
            Ok(())
        },
        WatchCommand::Stop { id } => {
            ensure_watch_id(&id)?;
            let mut config = load_config()?;
            let unit = format!("media-studio-watch-{id}.service");
            let status = required_command("systemctl")?
                .args(["--user", "disable", "--now", &unit])
                .status()?;
            if !status.success() {
                bail!("не удалось остановить {unit}");
            }
            if let Some(folder) = config.watch_folders.iter_mut().find(|folder| folder.id == id) {
                folder.enabled = false;
                config.write(&paths::config_path())?;
            } else {
                bail!("watch-folder не найден: {id}");
            }
            println!("WATCH_STOPPED id={id}");
            Ok(())
        },
    }
}

fn ensure_watch_id(id: &str) -> Result<()> {
    anyhow::ensure!(
        valid_watch_id(id),
        "id watch-folder должен содержать только A-Z, a-z, 0-9, '-' или '_' и быть не длиннее 64 символов"
    );
    Ok(())
}

fn install_watch_unit(folder: &WatchFolder) -> Result<()> {
    let binary = paths::installed_binary_path();
    let path = paths::watch_service_path(&folder.id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let binary = systemd_quote(&binary);
    let unit = format!("[Unit]\nDescription=Media Studio watch-folder {}\nAfter=graphical-session.target\n\n[Service]\nExecStart={} watch-run --id {}\nRestart=always\nRestartSec=4\n\n[Install]\nWantedBy=default.target\n", folder.id, binary, folder.id);
    paths::atomic_write(&path, unit.as_bytes())?;
    let reload = required_command("systemctl")?.args(["--user", "daemon-reload"]).status()?;
    if !reload.success() {
        bail!("systemd user не принял daemon-reload");
    }
    let unit_name = format!("media-studio-watch-{}.service", folder.id);
    let enabled =
        required_command("systemctl")?.args(["--user", "enable", "--now", &unit_name]).status()?;
    if !enabled.success() {
        bail!("не удалось включить {unit_name}");
    }
    Ok(())
}

fn preflight_install() -> Result<()> {
    let required = ["ffmpeg", "ffprobe", "systemd-run", "systemctl", "kbuildsycoca6"];
    let missing =
        required.iter().filter(|tool| find_on_path(tool).is_none()).copied().collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!("нельзя установить Media Studio: отсутствуют {}", missing.join(", "));
    }
    if find_on_path("zenity").is_none() && find_on_path("kdialog").is_none() {
        bail!("нельзя установить Media Studio: нужен zenity или kdialog для расширенного меню");
    }
    Ok(())
}

fn watch_run(id: &str) -> Result<()> {
    let mut seen = HashMap::<PathBuf, SystemTime>::new();
    loop {
        let config = load_config()?;
        let folder = config
            .watch_folders
            .iter()
            .find(|folder| folder.id == id)
            .with_context(|| format!("watch-folder не найден: {id}"))?
            .clone();
        if !folder.enabled {
            thread::sleep(Duration::from_secs(10));
            continue;
        }
        let source = fs::canonicalize(&folder.source)
            .with_context(|| format!("каталог watch-folder не найден: {}", folder.source))?;
        let destination = fs::canonicalize(&folder.destination)
            .with_context(|| format!("каталог назначения не найден: {}", folder.destination))?;
        if !source.is_dir() {
            bail!("каталог watch-folder не найден: {}", source.display());
        }
        for input in scan_media_files(&source, folder.recursive)? {
            if input.starts_with(&destination) {
                continue;
            }
            let metadata = match fs::metadata(&input) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
            let age = SystemTime::now().duration_since(modified).unwrap_or_default().as_secs();
            if age < folder.settle_seconds {
                continue;
            }
            if seen.get(&input).copied() == Some(modified) {
                continue;
            }
            enqueue_with_profile(
                &config,
                &folder.profile,
                None,
                config.hardware_fallback,
                Some(destination.clone()),
                false,
                vec![input.display().to_string()],
            )?;
            seen.insert(input, modified);
        }
        thread::sleep(Duration::from_secs(5));
    }
}

fn scan_media_files(root: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root)
        .with_context(|| format!("не удалось прочитать каталог {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() && recursive {
            files.extend(scan_media_files(&path, true)?);
        }
        if metadata.is_file() && is_media_path(&path) {
            files.push(path);
        }
    }
    Ok(files)
}

fn is_media_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "3gp"
            | "avi"
            | "avif"
            | "flac"
            | "gif"
            | "jpeg"
            | "jpg"
            | "m4a"
            | "mkv"
            | "mov"
            | "mp3"
            | "mp4"
            | "ogg"
            | "ogv"
            | "ogx"
            | "opus"
            | "png"
            | "wav"
            | "webm"
            | "webp"
            | "wmv"
    )
}

fn doctor() -> Result<()> {
    let config_path = paths::config_path();
    let config = load_config()?;
    println!("Media Studio doctor");
    println!("config: {}", config_path.display());
    println!("profiles: {}", config.profiles.len());
    println!("default_profile: {}", config.default_profile);
    println!("verify_results: {}", config.verify_results);
    println!("ffmpeg_threads: {}", config.ffmpeg_threads);
    let mut missing_core = Vec::new();
    for tool in ["ffmpeg", "ffprobe", "systemd-run", "systemctl", "kbuildsycoca6"] {
        match find_on_path(tool) {
            Some(path) => println!("{tool}: OK ({})", path.display()),
            None => {
                println!("{tool}: MISSING");
                missing_core.push(tool);
            },
        }
    }
    let dialog_backend = find_on_path("zenity").or_else(|| find_on_path("kdialog"));
    match dialog_backend {
        Some(path) => println!("dialog_backend: OK ({})", path.display()),
        None => {
            println!("dialog_backend: MISSING (zenity or kdialog)");
            missing_core.push("zenity|kdialog");
        },
    }
    for tool in ["magick", "notify-send"] {
        match find_on_path(tool) {
            Some(path) => println!("{tool}: OK ({})", path.display()),
            None => println!("{tool}: OPTIONAL-MISSING"),
        }
    }
    println!("binary: {}", paths::installed_binary_path().display());
    println!(
        "service_menus: {}",
        paths::service_menu_paths()
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    if !missing_core.is_empty() {
        bail!("не хватает обязательных инструментов: {}", missing_core.join(", "));
    }
    Ok(())
}

fn install(force_config: bool) -> Result<()> {
    preflight_install()?;
    let source = std::env::current_exe().context("не удалось определить текущий бинарник")?;
    let target = paths::installed_binary_path();
    if source != target {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&source, &target)
            .with_context(|| format!("не удалось установить бинарник в {}", target.display()))?;
    }
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755))?;

    let config_path = paths::config_path();
    let config = if force_config || !config_path.exists() {
        let config = Config::built_in();
        config.write(&config_path)?;
        config
    } else {
        let config = Config::load(&config_path)?;
        config.write(&config_path)?;
        config
    };
    let menu_paths = paths::service_menu_paths();
    fs::create_dir_all(paths::service_menu_dir())?;
    for (menu_path, menu) in menu_paths.iter().zip(service_menus(&target)) {
        fs::write(menu_path, menu)?;
        fs::set_permissions(menu_path, fs::Permissions::from_mode(0o755))?;
    }
    let legacy_menu = paths::legacy_service_menu_path();
    if legacy_menu.is_file() {
        fs::remove_file(&legacy_menu)?;
    }
    hide_legacy_menus()?;
    for folder in &config.watch_folders {
        if folder.enabled {
            install_watch_unit(folder)?;
        }
    }
    let cache = required_command("kbuildsycoca6")?.arg("--noincremental").status();
    println!("installed_binary={}", target.display());
    println!("config={}", config_path.display());
    println!(
        "service_menus={}",
        menu_paths.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(",")
    );
    println!(
        "kde_cache={}",
        match cache {
            Ok(status) if status.success() => "updated",
            Ok(status) => return Err(anyhow::anyhow!("kbuildsycoca6 завершился с кодом {status}")),
            Err(error) => return Err(error.into()),
        }
    );
    Ok(())
}

fn uninstall(purge: bool) -> Result<()> {
    let mut warnings = Vec::new();
    if let Some(systemctl) = runtime::optional("systemctl") {
        if let Ok(status) = Command::new(&systemctl).args(["--user", "daemon-reload"]).status() {
            if !status.success() {
                warnings.push(format!("первичный daemon-reload завершился с кодом {status}"));
            }
        }
    } else {
        warnings.push("systemctl не найден; user units не остановлены".to_string());
    }
    if let Ok(entries) = fs::read_dir(paths::systemd_user_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else { continue };
            if name.starts_with("media-studio-watch-") && name.ends_with(".service") {
                if let Some(systemctl) = runtime::optional("systemctl") {
                    match Command::new(systemctl)
                        .args(["--user", "disable", "--now", name])
                        .status()
                    {
                        Ok(status) if status.success() => {},
                        Ok(status) => {
                            warnings.push(format!("не удалось остановить {name}: код {status}"))
                        },
                        Err(error) => {
                            warnings.push(format!("не удалось остановить {name}: {error}"))
                        },
                    }
                } else {
                    warnings.push(format!("systemctl не найден; unit {name} не остановлен"));
                }
                let _ = fs::remove_file(path);
            }
        }
    }
    if let Some(systemctl) = runtime::optional("systemctl") {
        match Command::new(systemctl).args(["--user", "daemon-reload"]).status() {
            Ok(status) if status.success() => {},
            Ok(status) => {
                warnings.push(format!("финальный daemon-reload завершился с кодом {status}"))
            },
            Err(error) => {
                warnings.push(format!("не удалось выполнить финальный daemon-reload: {error}"))
            },
        }
    } else {
        warnings.push("systemctl не найден; финальный daemon-reload пропущен".to_string());
    }
    let binary = paths::installed_binary_path();
    for menu in paths::service_menu_paths()
        .into_iter()
        .chain(std::iter::once(paths::legacy_service_menu_path()))
    {
        if menu.is_file() {
            fs::remove_file(menu)?;
        }
    }
    if binary.is_file() {
        fs::remove_file(&binary)?;
    }
    restore_legacy_menus()?;
    if purge {
        let config_dir = paths::config_path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| paths::home_dir().join(".config/media-studio"));
        let state_dir = paths::state_dir();
        if config_dir.is_dir() {
            fs::remove_dir_all(config_dir)?;
        }
        if state_dir.is_dir() {
            fs::remove_dir_all(state_dir)?;
        }
    }
    let cache = runtime::optional("kbuildsycoca6")
        .map(|kbuildsycoca6| Command::new(kbuildsycoca6).arg("--noincremental").status());
    println!("uninstalled_binary={}", binary.display());
    println!("uninstalled_service_menus={}", paths::service_menu_dir().display());
    println!("purged={purge}");
    match cache {
        Some(Ok(status)) if status.success() => {},
        Some(Ok(status)) => warnings.push(format!("kbuildsycoca6 завершился с кодом {status}")),
        None => warnings.push("kbuildsycoca6 не найден; KDE cache не обновлён".to_string()),
        Some(Err(error)) => warnings.push(format!("не удалось обновить KDE cache: {error}")),
    }
    if warnings.is_empty() {
        Ok(())
    } else {
        println!("uninstall_state=partial");
        bail!("удаление завершено с предупреждениями: {}", warnings.join("; "))
    }
}

fn hide_legacy_menus() -> Result<()> {
    let legacy = [
        "compress-video-max-400mb.desktop",
        "compress-video-to-400mb.desktop",
        "compress-video-to-custom-size.desktop",
        "video-tools.desktop",
        "convert-audio-to-opus.desktop",
    ];
    let source_dir = paths::service_menu_dir();
    let disabled_dir = source_dir.join("disabled");
    fs::create_dir_all(&disabled_dir)?;
    for name in legacy {
        let source = source_dir.join(name);
        if source.is_file() {
            let target = disabled_dir.join(name);
            if !target.exists() {
                fs::rename(&source, &target)?;
            }
        }
    }
    Ok(())
}

fn restore_legacy_menus() -> Result<()> {
    let source_dir = paths::service_menu_dir();
    let disabled_dir = source_dir.join("disabled");
    if !disabled_dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(&disabled_dir)?.flatten() {
        let source = entry.path();
        let Some(name) = source.file_name() else { continue };
        let target = source_dir.join(name);
        if !target.exists() {
            fs::rename(source, target)?;
        }
    }
    let _ = fs::remove_dir(&disabled_dir);
    Ok(())
}

#[derive(Clone, Copy)]
struct MenuAction {
    id: &'static str,
    name: &'static str,
    name_ru: &'static str,
    icon: &'static str,
    command: &'static str,
}

const VIDEO_ACTIONS: &[MenuAction] = &[
    MenuAction {
        id: "mp4",
        name: "Video → MP4 (H.264)",
        name_ru: "Видео → MP4 (H.264)",
        icon: "video-x-generic",
        command: "enqueue --profile video_mp4",
    },
    MenuAction {
        id: "mp4hq",
        name: "Video → MP4 (H.264 HQ)",
        name_ru: "Видео → MP4 (H.264 HQ)",
        icon: "video-x-generic",
        command: "enqueue --profile video_mp4_hq",
    },
    MenuAction {
        id: "h265",
        name: "Video → MP4 (H.265)",
        name_ru: "Видео → MP4 (H.265)",
        icon: "video-x-generic",
        command: "enqueue --profile video_mp4_h265",
    },
    MenuAction {
        id: "webm",
        name: "Video → WebM (AV1/Opus)",
        name_ru: "Видео → WebM (AV1/Opus)",
        icon: "video-x-generic",
        command: "enqueue --profile video_webm_av1",
    },
    MenuAction {
        id: "size400",
        name: "Video → MP4 (до 400 МБ)",
        name_ru: "Видео → MP4 (до 400 МБ)",
        icon: "video-x-generic",
        command: "enqueue --profile video_mp4_400mb",
    },
    MenuAction {
        id: "max400",
        name: "Video → MP4 (сильное сжатие, до 400 МБ)",
        name_ru: "Видео → MP4 (сильное сжатие, до 400 МБ)",
        icon: "video-x-generic",
        command: "enqueue --profile video_mp4_max_400mb",
    },
    MenuAction {
        id: "vaapi",
        name: "Video → MP4 (VAAPI)",
        name_ru: "Видео → MP4 (VAAPI)",
        icon: "video-x-generic",
        command: "enqueue --profile video_mp4_vaapi",
    },
    MenuAction {
        id: "nvenc",
        name: "Video → MP4 (NVENC)",
        name_ru: "Видео → MP4 (NVENC)",
        icon: "video-x-generic",
        command: "enqueue --profile video_mp4_nvenc",
    },
    MenuAction {
        id: "opus",
        name: "Extract audio → Opus",
        name_ru: "Извлечь аудио → Opus",
        icon: "audio-x-generic",
        command: "enqueue --profile extract_audio_opus",
    },
    MenuAction {
        id: "remux",
        name: "Container → MKV (no re-encode)",
        name_ru: "Контейнер → MKV (без перекодирования)",
        icon: "package-x-generic",
        command: "enqueue --profile remux_mkv",
    },
    MenuAction {
        id: "strip",
        name: "Video without audio",
        name_ru: "Видео без аудио",
        icon: "audio-volume-muted",
        command: "enqueue --profile strip_audio",
    },
    MenuAction {
        id: "advanced",
        name: "Advanced profile…",
        name_ru: "Расширенный профиль…",
        icon: "configure",
        command: "choose",
    },
    MenuAction {
        id: "inspect",
        name: "Inspect media",
        name_ru: "Информация о медиафайле",
        icon: "dialog-information",
        command: "inspect",
    },
];

const AUDIO_ACTIONS: &[MenuAction] = &[
    MenuAction {
        id: "mp3",
        name: "Audio → MP3",
        name_ru: "Аудио → MP3",
        icon: "audio-x-generic",
        command: "enqueue --profile audio_mp3",
    },
    MenuAction {
        id: "flac",
        name: "Audio → FLAC",
        name_ru: "Аудио → FLAC",
        icon: "audio-x-generic",
        command: "enqueue --profile audio_flac",
    },
    MenuAction {
        id: "opus",
        name: "Audio → Opus",
        name_ru: "Аудио → Opus",
        icon: "audio-x-generic",
        command: "enqueue --profile audio_opus",
    },
    MenuAction {
        id: "advanced",
        name: "Advanced profile…",
        name_ru: "Расширенный профиль…",
        icon: "configure",
        command: "choose",
    },
    MenuAction {
        id: "inspect",
        name: "Inspect media",
        name_ru: "Информация о медиафайле",
        icon: "dialog-information",
        command: "inspect",
    },
];

const IMAGE_ACTIONS: &[MenuAction] = &[
    MenuAction {
        id: "webp",
        name: "Image → WebP",
        name_ru: "Изображение → WebP",
        icon: "image-x-generic",
        command: "enqueue --profile image_webp",
    },
    MenuAction {
        id: "jpeg",
        name: "Image → JPEG",
        name_ru: "Изображение → JPEG",
        icon: "image-x-generic",
        command: "enqueue --profile image_jpeg",
    },
    MenuAction {
        id: "avif",
        name: "Image → AVIF",
        name_ru: "Изображение → AVIF",
        icon: "image-x-generic",
        command: "enqueue --profile image_avif",
    },
    MenuAction {
        id: "resize",
        name: "Image → maximum 1920×1080",
        name_ru: "Изображение → максимум 1920×1080",
        icon: "image-x-generic",
        command: "enqueue --profile image_resize_1080",
    },
    MenuAction {
        id: "advanced",
        name: "Advanced profile…",
        name_ru: "Расширенный профиль…",
        icon: "configure",
        command: "choose",
    },
    MenuAction {
        id: "inspect",
        name: "Inspect media",
        name_ru: "Информация о медиафайле",
        icon: "dialog-information",
        command: "inspect",
    },
];

fn service_menus(executable: &Path) -> Vec<String> {
    vec![
        service_menu(
            executable,
            "Media Studio — Video",
            "Видео",
            "video/*;video/ogg;",
            VIDEO_ACTIONS,
        ),
        service_menu(
            executable,
            "Media Studio — Audio",
            "Аудио",
            "audio/*;application/ogg;application/x-ogg;audio/ogg;audio/opus;audio/x-opus+ogg;",
            AUDIO_ACTIONS,
        ),
        service_menu(executable, "Media Studio — Images", "Изображения", "image/*;", IMAGE_ACTIONS),
    ]
}

fn service_menu(
    executable: &Path,
    name: &str,
    name_ru: &str,
    mime_types: &str,
    actions: &[MenuAction],
) -> String {
    let exe = desktop_quote(executable);
    let action_ids = actions.iter().map(|action| action.id).collect::<Vec<_>>().join(";");
    let mut output = format!(
        "[Desktop Entry]\nType=Service\nServiceTypes=KonqPopupMenu/Plugin\nX-KDE-ServiceTypes=KonqPopupMenu/Plugin\nName={name}\nName[ru]={name_ru}\nMimeType={mime_types}\nActions={action_ids};\nX-KDE-Submenu=Media Studio\nX-KDE-Submenu[ru]=Media Studio\nIcon=applications-multimedia\n"
    );
    for action in actions {
        output.push_str(&format!(
            "\n[Desktop Action {}]\nName={}\nName[ru]={}\nIcon={}\nExec={} {} %F\n",
            action.id, action.name, action.name_ru, action.icon, exe, action.command
        ));
    }
    output
}

fn desktop_quote(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{raw}\"")
}

fn systemd_quote(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{raw}\"")
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    runtime::optional(name)
}

fn required_command(name: &str) -> Result<Command> {
    Ok(Command::new(runtime::required(name)?))
}

fn format_error(error: &anyhow::Error) -> String {
    let mut text = format!("{error:#}");
    if text.len() > 1200 {
        text.truncate(1200);
        text.push('…');
    }
    text
}

#[allow(dead_code)]
fn _stable_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("{}-{}", nanos, std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_quotes_executable() {
        let menus = service_menus(Path::new("/tmp/media studio"));
        assert_eq!(menus.len(), 3);
        assert!(menus[0].contains("Exec=\"/tmp/media studio\" enqueue"));
        assert!(menus[0].contains("MimeType=video/*;"));
        assert!(menus[1].contains("MimeType=audio/*;"));
        assert!(menus[2].contains("MimeType=image/*;"));
        assert!(!menus[2].contains("video_mp4"));
        assert!(!menus[1].contains("image_webp"));
    }

    #[test]
    fn watch_unit_quotes_binary_path() {
        assert_eq!(systemd_quote(Path::new("/tmp/media studio/bin")), "\"/tmp/media studio/bin\"");
    }

    #[test]
    fn built_in_config_is_valid() {
        let config = Config::built_in();
        config.validate().expect("built-in config must validate");
        assert!(config.profiles.len() >= 10);
    }
}
