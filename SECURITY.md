# Security policy

Сообщайте о проблемах безопасности приватно через [GitHub Security Advisories](https://github.com/Stranmor/media-studio/security/advisories/new). Если форма недоступна, используйте private contact, указанный в профиле владельца.

Не публикуйте в issue секреты, приватные файлы, токены или полные логи пользовательских задач.

Основная граница безопасности проекта — локальная машина пользователя: Media Studio запускает локальные `ffmpeg`, `ffprobe`, `magick` и user-systemd jobs от имени текущего пользователя.
