# Media Studio

Media Studio — локальный конвертер медиа для KDE Dolphin. Выберите файлы в Dolphin, откройте `Media Studio` и поставьте conversion job в user-systemd очередь.

Проект рассчитан на Linux + KDE Plasma 6. Пользовательские данные остаются локальными: Media Studio запускает `ffmpeg`, `ffprobe`, ImageMagick и user-systemd от имени текущего пользователя.

## Что решает проект

- отдельные контекстные меню Dolphin для видео, аудио и изображений;
- профили видео, аудио, изображений и remux;
- сжатие видео до заданного размера;
- расширенная форма с выбором профиля, размера, overwrite и fallback;
- VAAPI и NVENC с проверкой реального FFmpeg encoder/device и software fallback;
- watch-folders с отдельным каталогом назначения;
- безопасно зарезервированные временные файлы, atomic commit, `ffprobe` и полная decode-проверка;
- логи и статусы jobs в `~/.local/state/media-studio/jobs/`.

## Установка

Нужны `ffmpeg`, `ffprobe`, `systemd-run` и `systemctl`. `kbuildsycoca6`/`kbuildsycoca5` и один диалоговый backend (`zenity` или `kdialog`) рекомендуются для полной интеграции: без них меню устанавливаются, а cache/dialog возможности явно помечаются недоступными. Для image-профилей нужен `magick`.
`zenity` показывает все расширенные параметры одной формой; `kdialog` использует последовательные окна с теми же параметрами.

```bash
cargo build --release
./target/release/media-studio install
./target/release/media-studio doctor
```

Для x86_64 и ARM64 можно установить готовый бинарник без Rust toolchain:

```bash
curl --fail --location https://raw.githubusercontent.com/Stranmor/media-studio/main/install.sh | bash
```

Скрипт проверяет SHA-256 release asset, устанавливает бинарник в `~/.local/bin` и запускает `media-studio install`.

После установки:

- binary: `~/.local/bin/media-studio`;
- Dolphin menus: `media-studio-video.desktop`, `media-studio-audio.desktop`, `media-studio-image.desktop` в `~/.local/share/kio/servicemenus/`;
- config: `~/.config/media-studio/config.toml`;
- job state: `~/.local/state/media-studio/jobs/`.

Переиндексация KDE выполняется автоматически через доступный `kbuildsycoca6` или `kbuildsycoca5`; если helper отсутствует, установка завершается с состоянием `kde_cache=not-available`.
Каждое меню содержит только совместимые с MIME-группой действия; generic `desktop-file-validate` может ругаться на KDE-расширения `Service`/`Actions`, хотя KDE KIO их принимает.

## Использование в Dolphin

1. Выберите один или несколько медиафайлов.
2. Откройте правый клик → `Media Studio`.
3. Выберите профиль или `Расширенные настройки…`.
4. Результат появится рядом с исходным файлом, если не задан отдельный каталог назначения.

Для `.ogx` и других Ogg-файлов menu учитывает MIME `application/ogg` и `audio/ogg`.

Для Debian/Ubuntu и Fedora/RHEL доступны binary assets в GitHub Release. Для Arch есть `packaging/arch/PKGBUILD`; после установки пакета выполните `media-studio install` для пользовательского Dolphin-меню.

Flatpak packaging is split by trust boundary: `packaging/flatpak/io.github.stranmor.MediaStudio.yml` is the strict sandbox profile, while `io.github.stranmor.MediaStudio.HostIntegration.yml` is an explicitly experimental host-integration profile that delegates media and desktop helpers through `flatpak-spawn --host`. CI builds both profiles and runs a real conversion smoke inside each installed bundle.

## CLI

```bash
# обычный профиль
media-studio enqueue --profile video_mp4 -- file.mkv

# произвольный лимит размера
media-studio enqueue --profile video_mp4 --target-size-mb 400 -- file.mkv

# интерактивная форма
media-studio choose -- file.mkv

# инспекция
media-studio inspect -- file.mkv

# очередь
media-studio queue list
media-studio queue cancel JOB_ID
```

## Watch-folders

Watch-folder создаёт user-systemd unit, который запускается после входа в графическую сессию:

```bash
media-studio watch add \
  --id camera \
  --source ~/Videos/in \
  --destination ~/Videos/out \
  --profile video_mp4

media-studio watch list
media-studio watch stop --id camera
media-studio watch start --id camera
media-studio watch remove --id camera
```

Обрабатываются только медиафайлы; файл должен оставаться неизменным не менее `settle_seconds`.
Идентификатор watch-folder должен содержать только латинские буквы, цифры, `-` и `_`.

## Аппаратные профили

- `video_mp4_vaapi` — VAAPI с автоматическим поиском `/dev/dri/renderD*`;
- `video_mp4_nvenc` — NVIDIA NVENC при наличии `h264_nvenc` в текущем FFmpeg;
- `media-studio doctor` показывает эффективные `vaapi`/`nvenc` capabilities;
- устройство VAAPI можно задать через `MEDIA_STUDIO_VAAPI_DEVICE` или `vaapi_device` в `config.toml`;
- при недоступном или неисправном hardware encoder используется software-профиль, а причина fallback фиксируется в job log;
- fail-closed можно включить в `config.toml`, установив `hardware_fallback = false`.

## Удаление

Обычное удаление оставляет config и job history:

```bash
media-studio uninstall
```

Команда удаляет binary, service menu, watch units и возвращает ранее скрытые legacy menus. Полное удаление пользовательского config/state выполняется только явно:

```bash
media-studio uninstall --purge
```

## Разработка

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

CI дополнительно запускает KDE smoke-test с KDE cache builder, Dolphin service menus, ImageMagick/FFmpeg fixture и реальной проверкой результата через `ffprobe`.

Архитектура: [D2 map](docs/media-studio.svg). Правила contribution: [CONTRIBUTING.md](CONTRIBUTING.md). Security policy: [SECURITY.md](SECURITY.md).
