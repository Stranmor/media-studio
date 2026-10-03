use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

const CURRENT_SCHEMA: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub schema: u32,
    pub default_profile: String,
    pub verify_results: bool,
    pub ffmpeg_threads: u32,
    #[serde(default = "default_true")]
    pub hardware_fallback: bool,
    #[serde(default)]
    pub vaapi_device: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
    #[serde(default)]
    pub watch_folders: Vec<WatchFolder>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub label: String,
    pub category: String,
    pub engine: String,
    #[serde(default)]
    pub extension: String,
    pub suffix: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub target_size_mb: Option<u64>,
    #[serde(default)]
    pub target_audio_bitrate_kbps: u32,
    #[serde(default)]
    pub hardware: Option<HardwareBackend>,
    #[serde(default)]
    pub fallback_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HardwareBackend {
    Vaapi,
    Nvenc,
}

impl HardwareBackend {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Vaapi => "VAAPI",
            Self::Nvenc => "NVENC",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchFolder {
    pub id: String,
    pub source: String,
    pub destination: String,
    pub profile: String,
    #[serde(default = "default_true")]
    pub recursive: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_settle_seconds")]
    pub settle_seconds: u64,
}

impl Config {
    pub fn built_in() -> Self {
        let mut profiles = BTreeMap::new();

        add(
            &mut profiles,
            "video_mp4",
            "Видео → MP4 (H.264)",
            "Видео",
            "ffmpeg",
            "mp4",
            "mp4",
            vec![
                "-map",
                "0:v:0",
                "-map",
                "0:a?",
                "-map_metadata",
                "0",
                "-sn",
                "-dn",
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                "23",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "160k",
                "-movflags",
                "+faststart",
            ],
        );
        add(
            &mut profiles,
            "video_mp4_hq",
            "Видео → MP4 (H.264 HQ)",
            "Видео",
            "ffmpeg",
            "mp4",
            "hq",
            vec![
                "-map",
                "0:v:0",
                "-map",
                "0:a?",
                "-map_metadata",
                "0",
                "-sn",
                "-dn",
                "-c:v",
                "libx264",
                "-preset",
                "slow",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "192k",
                "-movflags",
                "+faststart",
            ],
        );
        add(
            &mut profiles,
            "video_mp4_h265",
            "Видео → MP4 (H.265)",
            "Видео",
            "ffmpeg",
            "mp4",
            "h265",
            vec![
                "-map",
                "0:v:0",
                "-map",
                "0:a?",
                "-map_metadata",
                "0",
                "-sn",
                "-dn",
                "-c:v",
                "libx265",
                "-preset",
                "medium",
                "-crf",
                "27",
                "-pix_fmt",
                "yuv420p",
                "-tag:v",
                "hvc1",
                "-c:a",
                "aac",
                "-b:a",
                "160k",
                "-movflags",
                "+faststart",
            ],
        );

        add(
            &mut profiles,
            "video_mp4_400mb",
            "Видео → MP4 (до 400 МБ)",
            "Сжатие",
            "ffmpeg",
            "mp4",
            "400mb",
            vec![
                "-map",
                "0:v:0",
                "-map",
                "0:a?",
                "-map_metadata",
                "0",
                "-sn",
                "-dn",
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "128k",
                "-movflags",
                "+faststart",
            ],
        );
        profiles
            .get_mut("video_mp4_400mb")
            .expect("profile")
            .target_size_mb = Some(400);
        profiles
            .get_mut("video_mp4_400mb")
            .expect("profile")
            .target_audio_bitrate_kbps = 128;

        add(
            &mut profiles,
            "video_mp4_max_400mb",
            "Видео → MP4 (сильное сжатие, до 400 МБ)",
            "Сжатие",
            "ffmpeg",
            "mp4",
            "max-400mb",
            vec![
                "-map",
                "0:v:0",
                "-map",
                "0:a?",
                "-map_metadata",
                "0",
                "-sn",
                "-dn",
                "-c:v",
                "libx264",
                "-preset",
                "slow",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "96k",
                "-movflags",
                "+faststart",
            ],
        );
        profiles
            .get_mut("video_mp4_max_400mb")
            .expect("profile")
            .target_size_mb = Some(400);
        profiles
            .get_mut("video_mp4_max_400mb")
            .expect("profile")
            .target_audio_bitrate_kbps = 96;

        add(
            &mut profiles,
            "video_webm_av1",
            "Видео → WebM (AV1/Opus)",
            "Видео",
            "ffmpeg",
            "webm",
            "av1",
            vec![
                "-map",
                "0:v:0",
                "-map",
                "0:a?",
                "-map_metadata",
                "0",
                "-sn",
                "-dn",
                "-c:v",
                "libsvtav1",
                "-crf",
                "35",
                "-preset",
                "6",
                "-c:a",
                "libopus",
                "-b:a",
                "128k",
            ],
        );

        add(
            &mut profiles,
            "video_mp4_vaapi",
            "Видео → MP4 (VAAPI)",
            "Аппаратное ускорение",
            "ffmpeg",
            "mp4",
            "vaapi",
            vec![
                "-map",
                "0:v:0",
                "-map",
                "0:a?",
                "-map_metadata",
                "0",
                "-sn",
                "-dn",
                "-vaapi_device",
                "{vaapi_device}",
                "-vf",
                "format=nv12,hwupload",
                "-c:v",
                "h264_vaapi",
                "-qp",
                "23",
                "-c:a",
                "aac",
                "-b:a",
                "160k",
                "-movflags",
                "+faststart",
            ],
        );
        profiles
            .get_mut("video_mp4_vaapi")
            .expect("profile")
            .hardware = Some(HardwareBackend::Vaapi);
        profiles
            .get_mut("video_mp4_vaapi")
            .expect("profile")
            .fallback_args = profiles
            .get("video_mp4")
            .expect("software profile")
            .args
            .clone();

        add(
            &mut profiles,
            "video_mp4_nvenc",
            "Видео → MP4 (NVENC)",
            "Аппаратное ускорение",
            "ffmpeg",
            "mp4",
            "nvenc",
            vec![
                "-map",
                "0:v:0",
                "-map",
                "0:a?",
                "-map_metadata",
                "0",
                "-sn",
                "-dn",
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p5",
                "-cq",
                "23",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "160k",
                "-movflags",
                "+faststart",
            ],
        );
        profiles
            .get_mut("video_mp4_nvenc")
            .expect("profile")
            .hardware = Some(HardwareBackend::Nvenc);
        profiles
            .get_mut("video_mp4_nvenc")
            .expect("profile")
            .fallback_args = profiles
            .get("video_mp4")
            .expect("software profile")
            .args
            .clone();

        add(
            &mut profiles,
            "audio_opus",
            "Аудио → Opus",
            "Аудио",
            "ffmpeg",
            "opus",
            "opus",
            vec![
                "-vn",
                "-map",
                "0:a:0",
                "-map_metadata",
                "0",
                "-c:a",
                "libopus",
                "-b:a",
                "128k",
                "-vbr",
                "on",
                "-compression_level",
                "10",
            ],
        );
        add(
            &mut profiles,
            "audio_mp3",
            "Аудио → MP3",
            "Аудио",
            "ffmpeg",
            "mp3",
            "mp3",
            vec![
                "-vn",
                "-map",
                "0:a:0",
                "-map_metadata",
                "0",
                "-c:a",
                "libmp3lame",
                "-q:a",
                "2",
            ],
        );
        add(
            &mut profiles,
            "audio_flac",
            "Аудио → FLAC",
            "Аудио",
            "ffmpeg",
            "flac",
            "flac",
            vec!["-vn", "-map", "0:a:0", "-map_metadata", "0", "-c:a", "flac"],
        );
        add(
            &mut profiles,
            "extract_audio_opus",
            "Извлечь аудио → Opus",
            "Извлечение",
            "ffmpeg",
            "opus",
            "audio",
            vec![
                "-vn", "-map", "0:a:0", "-c:a", "libopus", "-b:a", "128k", "-vbr", "on",
            ],
        );
        add(
            &mut profiles,
            "remux_mkv",
            "Контейнер → MKV (без перекодирования)",
            "Контейнер",
            "ffmpeg",
            "mkv",
            "remux",
            vec!["-map", "0", "-map_metadata", "0", "-c", "copy"],
        );
        add(
            &mut profiles,
            "strip_audio",
            "Видео без аудио",
            "Видео",
            "ffmpeg",
            "",
            "no-audio",
            vec!["-map", "0", "-map", "-0:a", "-c", "copy"],
        );
        add(
            &mut profiles,
            "image_webp",
            "Изображение → WebP",
            "Изображения",
            "magick",
            "webp",
            "webp",
            vec!["-strip", "-quality", "85"],
        );
        add(
            &mut profiles,
            "image_jpeg",
            "Изображение → JPEG",
            "Изображения",
            "magick",
            "jpg",
            "jpg",
            vec!["-strip", "-quality", "92"],
        );
        add(
            &mut profiles,
            "image_avif",
            "Изображение → AVIF",
            "Изображения",
            "magick",
            "avif",
            "avif",
            vec!["-strip", "-quality", "75"],
        );
        add(
            &mut profiles,
            "image_resize_1080",
            "Изображение → максимум 1920×1080",
            "Изображения",
            "magick",
            "",
            "1080p",
            vec!["-strip", "-resize", "1920x1080>"],
        );

        Self {
            schema: CURRENT_SCHEMA,
            default_profile: "video_mp4".to_string(),
            verify_results: true,
            ffmpeg_threads: 4,
            hardware_fallback: true,
            vaapi_device: None,
            profiles,
            watch_folders: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::built_in());
        }
        let text = fs::read_to_string(path)
            .with_context(|| format!("не удалось прочитать конфигурацию {}", path.display()))?;
        let mut config: Self = toml::from_str(&text)
            .with_context(|| format!("ошибка TOML в конфигурации {}", path.display()))?;
        config.upgrade();
        config.validate()?;
        Ok(config)
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        fs::write(path, text).with_context(|| format!("не удалось записать {}", path.display()))?;
        Ok(())
    }

    pub fn upgrade(&mut self) {
        if self.schema >= CURRENT_SCHEMA {
            return;
        }
        let defaults = Self::built_in();
        for (id, profile) in defaults.profiles {
            self.profiles.entry(id).or_insert(profile);
        }
        self.hardware_fallback = true;
        if self.vaapi_device.is_none() {
            self.vaapi_device = defaults.vaapi_device;
        }
        self.schema = CURRENT_SCHEMA;
    }

    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema == CURRENT_SCHEMA,
            "неподдерживаемая версия конфигурации: {}",
            self.schema
        );
        anyhow::ensure!(
            self.ffmpeg_threads > 0,
            "ffmpeg_threads должен быть больше нуля"
        );
        anyhow::ensure!(
            self.profiles.contains_key(&self.default_profile),
            "default_profile не найден: {}",
            self.default_profile
        );
        if let Some(device) = &self.vaapi_device {
            anyhow::ensure!(
                Path::new(device).is_absolute(),
                "vaapi_device должен быть абсолютным путём: {device}"
            );
        }
        for (id, profile) in &self.profiles {
            anyhow::ensure!(!id.trim().is_empty(), "пустой идентификатор профиля");
            anyhow::ensure!(
                !profile.label.trim().is_empty(),
                "пустая метка профиля {id}"
            );
            anyhow::ensure!(
                matches!(profile.engine.as_str(), "ffmpeg" | "magick"),
                "неподдерживаемый engine у профиля {id}: {}",
                profile.engine
            );
            if let Some(size) = profile.target_size_mb {
                anyhow::ensure!(
                    size > 0,
                    "target_size_mb должен быть больше нуля у профиля {id}"
                );
            }
        }
        for folder in &self.watch_folders {
            anyhow::ensure!(
                valid_watch_id(&folder.id),
                "некорректный id watch-folder: {}",
                folder.id
            );
            anyhow::ensure!(
                self.profiles.contains_key(&folder.profile),
                "watch-folder {} ссылается на неизвестный профиль {}",
                folder.id,
                folder.profile
            );
        }
        Ok(())
    }
}

pub fn valid_watch_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[allow(clippy::too_many_arguments)]
fn add(
    profiles: &mut BTreeMap<String, Profile>,
    id: &str,
    label: &str,
    category: &str,
    engine: &str,
    extension: &str,
    suffix: &str,
    args: Vec<&str>,
) {
    profiles.insert(
        id.to_string(),
        Profile {
            label: label.to_string(),
            category: category.to_string(),
            engine: engine.to_string(),
            extension: extension.to_string(),
            suffix: suffix.to_string(),
            args: args.into_iter().map(str::to_string).collect(),
            target_size_mb: None,
            target_audio_bitrate_kbps: 128,
            hardware: None,
            fallback_args: Vec::new(),
        },
    );
}

fn default_true() -> bool {
    true
}
fn default_settle_seconds() -> u64 {
    3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_profiles_cover_requested_features() {
        let config = Config::built_in();
        assert_eq!(config.schema, CURRENT_SCHEMA);
        assert_eq!(config.profiles["video_mp4_400mb"].target_size_mb, Some(400));
        assert!(matches!(
            config.profiles["video_mp4_vaapi"].hardware,
            Some(HardwareBackend::Vaapi)
        ));
        assert!(matches!(
            config.profiles["video_mp4_nvenc"].hardware,
            Some(HardwareBackend::Nvenc)
        ));
        assert!(config.profiles["video_mp4_vaapi"]
            .args
            .windows(2)
            .any(|pair| pair == ["-vaapi_device", "{vaapi_device}"]));
    }

    #[test]
    fn watch_ids_are_safe_for_unit_names() {
        assert!(valid_watch_id("camera_01"));
        assert!(valid_watch_id("archive-2026"));
        assert!(!valid_watch_id("../escape"));
        assert!(!valid_watch_id("with space"));
        assert!(!valid_watch_id(""));
    }
}
