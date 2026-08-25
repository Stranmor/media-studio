# Contributing

## Быстрый цикл

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Перед изменением service menu или CLI проверьте путь установки, MIME-фильтры и реальные аргументы `ffmpeg` через `media-studio doctor` и representative conversion.

## Pull requests

- одна причина изменения на PR;
- описание пользовательского эффекта и проверки;
- не добавляйте персональные пути, секреты или runtime-артефакты;
- новые пользовательские строки синхронизируйте с README и CLI-подсказками.
