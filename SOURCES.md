# Источники

Здесь перечислено всё, что скачивается при сборке, и все сторонние библиотеки.
Готовая программа сама в интернет не ходит. Исключения — только по вашему клику:
ссылки на сайты (VB-Cable, RHVoice, примеры голосов Piper) открываются в браузере, а кнопки
«Скачать» на вкладке «Синтез речи» скачивают движок и голоса Piper с адресов из раздела 2.

## 1. Что скачивает build.bat

Скачивание идёт только при сборке и только если нужного ещё нет на диске.
Сервер выбирается по скорости из списка ниже. Чтобы пользоваться только официальными серверами:
`set ZV_MIRROR=official`, затем `build.bat`.

### Компилятор Rust (только если Rust не установлен или нет нужного варианта)

| Сервер | Кто держит | Адрес |
|---|---|---|
| official | Rust Foundation (официальный) | https://static.rust-lang.org |
| rsproxy | ByteDance (зеркало) | https://rsproxy.cn |
| ustc | Университет науки и технологий Китая (зеркало) | https://mirrors.ustc.edu.cn/rust-static |
| tuna | Университет Цинхуа (зеркало) | https://mirrors.tuna.tsinghua.edu.cn/rustup |

Как проверяется:
- установщик `rustup-init.exe` скачивается через `curl.exe` и сверяется по SHA-256 (`certutil`) с контрольной суммой с **официального** сервера
  (если официальный недоступен — с выбранного зеркала, об этом пишется в окне);
- сам компилятор `rustup` проверяет по контрольным суммам из манифеста выпуска.

Официальная страница установки Rust: https://rustup.rs

### Библиотеки (crates)

| Сервер | Кто держит | Индекс |
|---|---|---|
| official | Rust Foundation (официальный) | https://index.crates.io/ (файлы — https://static.crates.io) |
| rsproxy | ByteDance (зеркало) | https://rsproxy.cn/index/ |
| ustc | Университет науки и технологий Китая (зеркало) | https://mirrors.ustc.edu.cn/crates.io-index/ |
| tuna | Университет Цинхуа (зеркало) | https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/ |

Как проверяется: в `Cargo.lock` записаны точные версии и **SHA-256 каждой библиотеки**.
Сборка запускается с `--locked`: cargo не поменяет ни одной версии и сверит каждый скачанный файл
с контрольной суммой. Зеркало не может подсунуть изменённую библиотеку — сборка просто остановится.

Зеркало подключается только на время одной команды (`--config ...`), файлы настроек cargo
на компьютере не меняются.

## 2. Что используется на компьютере пользователя

| Что | Откуда | Зачем |
|---|---|---|
| Windows PowerShell 5.1 | встроен в Windows 10/11 | запуск встроенного плагина голосов |
| System.Speech (.NET Framework) | встроен в Windows | голоса Windows (SAPI) |
| Шрифты Segoe UI | встроены в Windows | интерфейс |
| VB-Cable | https://vb-audio.com/Cable/ — **ставится вручную, в программу не входит** | виртуальный микрофон |
| Piper (движок) 2023.11.14-2 | https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip — **только по кнопке**, 22 477 236 байт, SHA-256 `f3c58906402b24f3a96d92145f58acba6d86c9b5db896d207f78dc80811efcea` (программа сверяет и при несовпадении удаляет файл). Лицензия MIT, автор — Michael Hansen (rhasspy) | синтез речи без интернета |
| Голоса Piper (ru_RU: irina, denis, dmitri, ruslan — medium) | https://huggingface.co/rhasspy/piper-voices (ветка `v1.0.0`, запасной адрес — `main`) — **только по кнопке**. Лицензии голосов — в файле MODEL_CARD рядом с каждым голосом | голоса для Piper |
| RHVoice | https://rhvoice.su — **ставится вручную, в программу не входит**, только упоминается в подсказке | лёгкие голоса Windows (SAPI) |

Движок и голоса кладутся в `%LOCALAPPDATA%\Zvukopad\tts\piper`, а не в папку программы.
Скачивание идёт через Windows (schannel) — сертификаты проверяет сама Windows.

## 3. Как проверить самому

```
cargo tree -e normal --target x86_64-pc-windows-gnu      дерево зависимостей
cargo tree -d                                            библиотеки в нескольких версиях
```
Дополнительно (нужно поставить отдельно): `cargo install cargo-audit` → `cargo audit` —
проверка по базе известных уязвимостей RustSec.

## 4. Прямые зависимости

| Библиотека | Зачем | Лицензия | Код |
|---|---|---|---|
| eframe / egui 0.37 | окно и интерфейс | MIT / Apache-2.0 | https://github.com/emilk/egui |
| cpal 0.18 | доступ к звуковым устройствам (WASAPI) | Apache-2.0 | https://github.com/RustAudio/cpal |
| symphonia 0.6 | декодирование mp3, ogg, flac, aac, wav | MPL-2.0 | https://github.com/pdeljanov/Symphonia |
| hound 3.5 | запись WAV | Apache-2.0 | https://github.com/ruuda/hound |
| global-hotkey 0.9 | глобальные горячие клавиши (RegisterHotKey) | MIT / Apache-2.0 | https://github.com/tauri-apps/global-hotkey |
| rtrb 0.4 | очередь без блокировок между звуковыми потоками | MIT / Apache-2.0 | https://github.com/mgeier/rtrb |
| crossbeam-channel 0.5 | очереди команд | MIT / Apache-2.0 | https://github.com/crossbeam-rs/crossbeam |
| parking_lot 0.12 | быстрые блокировки | MIT / Apache-2.0 | https://github.com/Amanieu/parking_lot |
| serde, serde_json 1 | чтение/запись настроек | MIT / Apache-2.0 | https://github.com/serde-rs |
| dirs 7 | папки Документы / AppData | MIT / Apache-2.0 | https://codeberg.org/dirs/dirs-rs |
| log 0.4 | журнал | MIT / Apache-2.0 | https://github.com/rust-lang/log |
| windows-sys 0.61 | Корзина, местное время, клавиши, нажатие клавиши рации, язык системы (Windows API) | MIT / Apache-2.0 | https://github.com/microsoft/windows-rs |
| ureq 3.4 (native-tls) | скачивание движка и голосов Piper по кнопке; TLS — штатный Windows (schannel) | MIT / Apache-2.0 | https://github.com/algesten/ureq |
| zip 9 (deflate) | распаковка архива Piper | MIT | https://github.com/zip-rs/zip2 |
| sha2 0.11 | проверка контрольной суммы архива Piper | MIT / Apache-2.0 | https://github.com/RustCrypto/hashes |
| arboard 3.6 | буфер обмена (Ctrl+C/Ctrl+V и меню «Вставить» в текстовых полях) | MIT / Apache-2.0 | https://github.com/1Password/arboard |

Все лицензии разрешают свободное использование, в том числе в закрытых программах.
MPL-2.0 (symphonia) требует только открывать изменения в файлах самой библиотеки, если вы их правите.

## 5. Проверка сборки на GitHub (необязательно)

Файл `.github/workflows/build.yml` запускает build.bat на чистой Windows в GitHub Actions.
Используются только официальные действия GitHub: [actions/checkout](https://github.com/actions/checkout)
и [actions/upload-artifact](https://github.com/actions/upload-artifact). Если GitHub не используете — файл ни на что не влияет.

## 6. Полный список библиотек в сборке для Windows

Сгенерирован из `Cargo.lock` (`cargo metadata --filter-platform x86_64-pc-windows-gnu`).
Точные контрольные суммы — в `Cargo.lock`.

| Библиотека | Версия | Лицензия | Репозиторий |
|---|---|---|---|
| [accesskit](https://crates.io/crates/accesskit/0.25.1) | 0.25.1 | MIT OR Apache-2.0 | https://github.com/AccessKit/accesskit |
| [adler2](https://crates.io/crates/adler2/2.0.1) | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | https://github.com/oyvindln/adler2 |
| [ahash](https://crates.io/crates/ahash/0.8.12) | 0.8.12 | MIT OR Apache-2.0 | https://github.com/tkaitchuck/ahash |
| [arboard](https://crates.io/crates/arboard/3.7.0) | 3.7.0 | MIT OR Apache-2.0 | https://github.com/1Password/arboard |
| [arrayvec](https://crates.io/crates/arrayvec/0.7.8) | 0.7.8 | MIT OR Apache-2.0 | https://github.com/bluss/arrayvec |
| [autocfg](https://crates.io/crates/autocfg/1.5.1) | 1.5.1 | Apache-2.0 OR MIT | https://github.com/cuviper/autocfg |
| [base64](https://crates.io/crates/base64/0.23.1) | 0.23.1 | MIT OR Apache-2.0 | https://github.com/marshallpierce/rust-base64 |
| [base64ct](https://crates.io/crates/base64ct/1.8.3) | 1.8.3 | Apache-2.0 OR MIT | https://github.com/RustCrypto/formats |
| [bit-set](https://crates.io/crates/bit-set/0.10.0) | 0.10.0 | Apache-2.0 OR MIT | https://github.com/contain-rs/bit-set |
| [bit-vec](https://crates.io/crates/bit-vec/0.9.1) | 0.9.1 | Apache-2.0 OR MIT | https://github.com/contain-rs/bit-vec |
| [bitflags](https://crates.io/crates/bitflags/2.13.2) | 2.13.2 | MIT OR Apache-2.0 | https://github.com/bitflags/bitflags |
| [block-buffer](https://crates.io/crates/block-buffer/0.12.1) | 0.12.1 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| [bumpalo](https://crates.io/crates/bumpalo/3.20.3) | 3.20.3 | MIT OR Apache-2.0 | https://github.com/fitzgen/bumpalo |
| [bytemuck](https://crates.io/crates/bytemuck/1.25.2) | 1.25.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/Lokathor/bytemuck |
| [bytemuck_derive](https://crates.io/crates/bytemuck_derive/1.12.1) | 1.12.1 | Zlib OR Apache-2.0 OR MIT | https://github.com/Lokathor/bytemuck |
| [byteorder-lite](https://crates.io/crates/byteorder-lite/0.1.0) | 0.1.0 | Unlicense OR MIT | https://github.com/image-rs/byteorder-lite |
| [bytes](https://crates.io/crates/bytes/1.12.1) | 1.12.1 | MIT | https://github.com/tokio-rs/bytes |
| [cfg-if](https://crates.io/crates/cfg-if/1.0.5) | 1.0.5 | MIT OR Apache-2.0 | https://github.com/rust-lang/cfg-if |
| [cfg_aliases](https://crates.io/crates/cfg_aliases/0.2.2) | 0.2.2 | MIT | https://github.com/katharostech/cfg_aliases |
| [clipboard-win](https://crates.io/crates/clipboard-win/5.4.1) | 5.4.1 | BSL-1.0 | https://github.com/DoumanAsh/clipboard-win |
| [codespan-reporting](https://crates.io/crates/codespan-reporting/0.13.1) | 0.13.1 | Apache-2.0 | https://github.com/brendanzab/codespan |
| [color](https://crates.io/crates/color/0.3.3) | 0.3.3 | Apache-2.0 OR MIT | https://github.com/linebender/color |
| [const-oid](https://crates.io/crates/const-oid/0.10.2) | 0.10.2 | Apache-2.0 OR MIT | https://github.com/RustCrypto/formats |
| [cpal](https://crates.io/crates/cpal/0.18.2) | 0.18.2 | Apache-2.0 | https://github.com/RustAudio/cpal |
| [cpufeatures](https://crates.io/crates/cpufeatures/0.3.1) | 0.3.1 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| [crc32fast](https://crates.io/crates/crc32fast/1.5.2) | 1.5.2 | MIT OR Apache-2.0 | https://github.com/srijs/rust-crc32fast |
| [crossbeam-channel](https://crates.io/crates/crossbeam-channel/0.5.17) | 0.5.17 | MIT OR Apache-2.0 | https://github.com/crossbeam-rs/crossbeam |
| [crossbeam-utils](https://crates.io/crates/crossbeam-utils/0.8.23) | 0.8.23 | MIT OR Apache-2.0 | https://github.com/crossbeam-rs/crossbeam |
| [crypto-common](https://crates.io/crates/crypto-common/0.2.2) | 0.2.2 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| [cursor-icon](https://crates.io/crates/cursor-icon/1.2.0) | 1.2.0 | MIT OR Apache-2.0 OR Zlib | https://github.com/rust-windowing/cursor-icon |
| [dasp_sample](https://crates.io/crates/dasp_sample/0.11.0) | 0.11.0 | MIT OR Apache-2.0 | https://github.com/rustaudio/sample.git |
| [der](https://crates.io/crates/der/0.8.2) | 0.8.2 | Apache-2.0 OR MIT | https://github.com/RustCrypto/formats |
| [digest](https://crates.io/crates/digest/0.11.3) | 0.11.3 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| [dirs](https://crates.io/crates/dirs/7.0.0) | 7.0.0 | MIT OR Apache-2.0 | https://codeberg.org/dirs/dirs-rs |
| [dirs-sys](https://crates.io/crates/dirs-sys/0.5.0) | 0.5.0 | MIT OR Apache-2.0 | https://github.com/dirs-dev/dirs-sys-rs |
| [document-features](https://crates.io/crates/document-features/0.2.12) | 0.2.12 | MIT OR Apache-2.0 | https://github.com/slint-ui/document-features |
| [dpi](https://crates.io/crates/dpi/0.1.2) | 0.1.2 | Apache-2.0 AND MIT | https://github.com/rust-windowing/winit |
| [ecolor](https://crates.io/crates/ecolor/0.37.0) | 0.37.0 | MIT OR Apache-2.0 | https://github.com/emilk/egui |
| [eframe](https://crates.io/crates/eframe/0.37.0) | 0.37.0 | MIT OR Apache-2.0 | https://github.com/emilk/egui/tree/main/crates/eframe |
| [egui](https://crates.io/crates/egui/0.37.0) | 0.37.0 | MIT OR Apache-2.0 | https://github.com/emilk/egui |
| [egui-wgpu](https://crates.io/crates/egui-wgpu/0.37.0) | 0.37.0 | MIT OR Apache-2.0 | https://github.com/emilk/egui/tree/main/crates/egui-wgpu |
| [egui-winit](https://crates.io/crates/egui-winit/0.37.0) | 0.37.0 | MIT OR Apache-2.0 | https://github.com/emilk/egui/tree/main/crates/egui-winit |
| [egui_glow](https://crates.io/crates/egui_glow/0.37.0) | 0.37.0 | MIT OR Apache-2.0 | https://github.com/emilk/egui/tree/main/crates/egui_glow |
| [either](https://crates.io/crates/either/1.19.0) | 1.19.0 | MIT OR Apache-2.0 | https://github.com/rayon-rs/either |
| [emath](https://crates.io/crates/emath/0.37.0) | 0.37.0 | MIT OR Apache-2.0 | https://github.com/emilk/egui/tree/main/crates/emath |
| [epaint](https://crates.io/crates/epaint/0.37.0) | 0.37.0 | MIT OR Apache-2.0 | https://github.com/emilk/egui/tree/main/crates/epaint |
| [epaint_default_fonts](https://crates.io/crates/epaint_default_fonts/0.37.0) | 0.37.0 | (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 | https://github.com/emilk/egui/tree/main/crates/epaint_default_fonts |
| [equivalent](https://crates.io/crates/equivalent/1.0.3) | 1.0.3 | Apache-2.0 OR MIT | https://github.com/indexmap-rs/equivalent |
| [error-code](https://crates.io/crates/error-code/3.4.0) | 3.4.0 | BSL-1.0 | https://github.com/DoumanAsh/error-code |
| [euclid](https://crates.io/crates/euclid/0.22.14) | 0.22.14 | MIT OR Apache-2.0 | https://github.com/servo/euclid |
| [extended](https://crates.io/crates/extended/0.1.0) | 0.1.0 | MIT | https://github.com/depp/extended-rs |
| [fax](https://crates.io/crates/fax/0.2.7) | 0.2.7 | MIT | https://github.com/pdf-rs/fax |
| [fdeflate](https://crates.io/crates/fdeflate/0.3.7) | 0.3.7 | MIT OR Apache-2.0 | https://github.com/image-rs/fdeflate |
| [fearless_simd](https://crates.io/crates/fearless_simd/0.4.1) | 0.4.1 | Apache-2.0 OR MIT | https://github.com/linebender/fearless_simd |
| [flate2](https://crates.io/crates/flate2/1.1.10) | 1.1.10 | MIT OR Apache-2.0 | https://github.com/rust-lang/flate2-rs |
| [foldhash](https://crates.io/crates/foldhash/0.2.0) | 0.2.0 | Zlib | https://github.com/orlp/foldhash |
| [font-types](https://crates.io/crates/font-types/0.12.7) | 0.12.7 | MIT OR Apache-2.0 | https://github.com/googlefonts/fontations |
| [gl_generator](https://crates.io/crates/gl_generator/0.14.0) | 0.14.0 | Apache-2.0 | https://github.com/brendanzab/gl-rs/ |
| [glifo](https://crates.io/crates/glifo/0.3.0) | 0.3.0 | Apache-2.0 OR MIT | https://github.com/linebender/vello |
| [global-hotkey](https://crates.io/crates/global-hotkey/0.9.0) | 0.9.0 | Apache-2.0 OR MIT | https://github.com/tauri-apps/global-hotkey |
| [glow](https://crates.io/crates/glow/0.17.0) | 0.17.0 | MIT OR Apache-2.0 OR Zlib | https://github.com/grovesNL/glow |
| [glutin](https://crates.io/crates/glutin/0.32.3) | 0.32.3 | Apache-2.0 | https://github.com/rust-windowing/glutin |
| [glutin-winit](https://crates.io/crates/glutin-winit/0.5.0) | 0.5.0 | MIT | https://github.com/rust-windowing/glutin |
| [glutin_egl_sys](https://crates.io/crates/glutin_egl_sys/0.7.1) | 0.7.1 | Apache-2.0 | https://github.com/rust-windowing/glutin |
| [glutin_wgl_sys](https://crates.io/crates/glutin_wgl_sys/0.6.1) | 0.6.1 | Apache-2.0 | https://github.com/rust-windowing/glutin |
| [guillotiere](https://crates.io/crates/guillotiere/0.7.0) | 0.7.0 | MIT/Apache-2.0 | https://github.com/nical/guillotiere |
| [half](https://crates.io/crates/half/2.7.1) | 2.7.1 | MIT OR Apache-2.0 | https://github.com/VoidStarKat/half-rs |
| [harfrust](https://crates.io/crates/harfrust/0.12.0) | 0.12.0 | MIT | https://github.com/harfbuzz/harfrust |
| [hashbrown](https://crates.io/crates/hashbrown/0.17.1) | 0.17.1 | MIT OR Apache-2.0 | https://github.com/rust-lang/hashbrown |
| [hound](https://crates.io/crates/hound/3.5.1) | 3.5.1 | Apache-2.0 | https://github.com/ruuda/hound |
| [http](https://crates.io/crates/http/1.5.0) | 1.5.0 | MIT OR Apache-2.0 | https://github.com/hyperium/http |
| [httparse](https://crates.io/crates/httparse/1.10.1) | 1.10.1 | MIT OR Apache-2.0 | https://github.com/seanmonstar/httparse |
| [hybrid-array](https://crates.io/crates/hybrid-array/0.4.15) | 0.4.15 | MIT OR Apache-2.0 | https://github.com/RustCrypto/hybrid-array |
| [image](https://crates.io/crates/image/0.25.10) | 0.25.10 | MIT OR Apache-2.0 | https://github.com/image-rs/image |
| [indexmap](https://crates.io/crates/indexmap/2.14.2) | 2.14.2 | Apache-2.0 OR MIT | https://github.com/indexmap-rs/indexmap |
| [itertools](https://crates.io/crates/itertools/0.15.0) | 0.15.0 | MIT OR Apache-2.0 | https://github.com/rust-itertools/itertools |
| [itoa](https://crates.io/crates/itoa/1.0.18) | 1.0.18 | MIT OR Apache-2.0 | https://github.com/dtolnay/itoa |
| [keyboard-types](https://crates.io/crates/keyboard-types/0.8.3) | 0.8.3 | MIT OR Apache-2.0 | https://github.com/rust-windowing/keyboard-types |
| [khronos_api](https://crates.io/crates/khronos_api/3.1.0) | 3.1.0 | Apache-2.0 | https://github.com/brendanzab/gl-rs/ |
| [kurbo](https://crates.io/crates/kurbo/0.13.1) | 0.13.1 | Apache-2.0 OR MIT | https://github.com/linebender/kurbo |
| [lazy_static](https://crates.io/crates/lazy_static/1.5.1) | 1.5.1 | MIT OR Apache-2.0 | https://github.com/rust-lang-nursery/lazy-static.rs |
| [libloading](https://crates.io/crates/libloading/0.8.9) | 0.8.9 | ISC | https://github.com/nagisa/rust_libloading/ |
| [libm](https://crates.io/crates/libm/0.2.16) | 0.2.16 | MIT | https://github.com/rust-lang/compiler-builtins |
| [linebender_resource_handle](https://crates.io/crates/linebender_resource_handle/0.1.1) | 0.1.1 | Apache-2.0 OR MIT | https://github.com/linebender/raw_resource_handle |
| [litrs](https://crates.io/crates/litrs/1.0.1) | 1.0.1 | MIT OR Apache-2.0 | https://github.com/LukasKalbertodt/litrs |
| [lock_api](https://crates.io/crates/lock_api/0.4.14) | 0.4.14 | MIT OR Apache-2.0 | https://github.com/Amanieu/parking_lot |
| [log](https://crates.io/crates/log/0.4.34) | 0.4.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/log |
| [memchr](https://crates.io/crates/memchr/2.8.3) | 2.8.3 | Unlicense OR MIT | https://github.com/BurntSushi/memchr |
| [miniz_oxide](https://crates.io/crates/miniz_oxide/0.8.9) | 0.8.9 | MIT OR Zlib OR Apache-2.0 | https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide |
| [miniz_oxide](https://crates.io/crates/miniz_oxide/0.9.1) | 0.9.1 | MIT OR Zlib OR Apache-2.0 | https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide |
| [moxcms](https://crates.io/crates/moxcms/0.8.1) | 0.8.1 | BSD-3-Clause OR Apache-2.0 | https://github.com/awxkee/moxcms.git |
| [naga](https://crates.io/crates/naga/30.0.1) | 30.0.1 | MIT OR Apache-2.0 | https://github.com/gfx-rs/wgpu |
| [naga-types](https://crates.io/crates/naga-types/30.0.1) | 30.0.1 | MIT OR Apache-2.0 | https://github.com/gfx-rs/wgpu |
| [native-tls](https://crates.io/crates/native-tls/0.2.18) | 0.2.18 | MIT OR Apache-2.0 | https://github.com/rust-native-tls/rust-native-tls |
| [nohash-hasher](https://crates.io/crates/nohash-hasher/0.2.0) | 0.2.0 | Apache-2.0 OR MIT | https://github.com/paritytech/nohash-hasher |
| [num-complex](https://crates.io/crates/num-complex/0.4.6) | 0.4.6 | MIT OR Apache-2.0 | https://github.com/rust-num/num-complex |
| [num-traits](https://crates.io/crates/num-traits/0.2.19) | 0.2.19 | MIT OR Apache-2.0 | https://github.com/rust-num/num-traits |
| [once_cell](https://crates.io/crates/once_cell/1.21.4) | 1.21.4 | MIT OR Apache-2.0 | https://github.com/matklad/once_cell |
| [option-ext](https://crates.io/crates/option-ext/0.2.0) | 0.2.0 | MPL-2.0 | https://github.com/soc/option-ext.git |
| [parking_lot](https://crates.io/crates/parking_lot/0.12.5) | 0.12.5 | MIT OR Apache-2.0 | https://github.com/Amanieu/parking_lot |
| [parking_lot_core](https://crates.io/crates/parking_lot_core/0.9.12) | 0.9.12 | MIT OR Apache-2.0 | https://github.com/Amanieu/parking_lot |
| [pem-rfc7468](https://crates.io/crates/pem-rfc7468/1.0.0) | 1.0.0 | Apache-2.0 OR MIT | https://github.com/RustCrypto/formats |
| [peniko](https://crates.io/crates/peniko/0.6.1) | 0.6.1 | Apache-2.0 OR MIT | https://github.com/linebender/peniko |
| [percent-encoding](https://crates.io/crates/percent-encoding/2.3.2) | 2.3.2 | MIT OR Apache-2.0 | https://github.com/servo/rust-url/ |
| [pin-project-lite](https://crates.io/crates/pin-project-lite/0.2.17) | 0.2.17 | Apache-2.0 OR MIT | https://github.com/taiki-e/pin-project-lite |
| [png](https://crates.io/crates/png/0.18.1) | 0.18.1 | MIT OR Apache-2.0 | https://github.com/image-rs/image-png |
| [polycool](https://crates.io/crates/polycool/0.4.0) | 0.4.0 | MIT OR Apache-2.0 | https://github.com/linebender/kurbo |
| [proc-macro2](https://crates.io/crates/proc-macro2/1.0.107) | 1.0.107 | MIT OR Apache-2.0 | https://github.com/dtolnay/proc-macro2 |
| [profiling](https://crates.io/crates/profiling/1.0.18) | 1.0.18 | MIT OR Apache-2.0 | https://github.com/aclysma/profiling |
| [pxfm](https://crates.io/crates/pxfm/0.1.30) | 0.1.30 | BSD-3-Clause OR Apache-2.0 | https://github.com/awxkee/pxfm |
| [quick-error](https://crates.io/crates/quick-error/2.0.1) | 2.0.1 | MIT/Apache-2.0 | http://github.com/tailhook/quick-error |
| [quote](https://crates.io/crates/quote/1.0.47) | 1.0.47 | MIT OR Apache-2.0 | https://github.com/dtolnay/quote |
| [raw-window-handle](https://crates.io/crates/raw-window-handle/0.6.2) | 0.6.2 | MIT OR Apache-2.0 OR Zlib | https://github.com/rust-windowing/raw-window-handle |
| [read-fonts](https://crates.io/crates/read-fonts/0.41.0) | 0.41.0 | MIT OR Apache-2.0 | https://github.com/googlefonts/fontations |
| [regex-lite](https://crates.io/crates/regex-lite/0.1.9) | 0.1.9 | MIT OR Apache-2.0 | https://github.com/rust-lang/regex |
| [renderdoc-sys](https://crates.io/crates/renderdoc-sys/1.1.0) | 1.1.0 | MIT OR Apache-2.0 | https://github.com/ebkalderon/renderdoc-rs |
| [rtrb](https://crates.io/crates/rtrb/0.4.0) | 0.4.0 | MIT OR Apache-2.0 | https://github.com/mgeier/rtrb |
| [rustc-hash](https://crates.io/crates/rustc-hash/1.1.0) | 1.1.0 | Apache-2.0/MIT | https://github.com/rust-lang-nursery/rustc-hash |
| [rustc-hash](https://crates.io/crates/rustc-hash/2.1.3) | 2.1.3 | Apache-2.0 OR MIT | https://github.com/rust-lang/rustc-hash |
| [rustls-pki-types](https://crates.io/crates/rustls-pki-types/1.15.1) | 1.15.1 | MIT OR Apache-2.0 | https://github.com/rustls/pki-types |
| [schannel](https://crates.io/crates/schannel/0.1.30) | 0.1.30 | MIT | https://github.com/steffengy/schannel-rs |
| [scopeguard](https://crates.io/crates/scopeguard/1.2.0) | 1.2.0 | MIT OR Apache-2.0 | https://github.com/bluss/scopeguard |
| [self_cell](https://crates.io/crates/self_cell/1.3.0) | 1.3.0 | Apache-2.0 OR GPL-2.0-only | https://github.com/Voultapher/self_cell |
| [serde](https://crates.io/crates/serde/1.0.229) | 1.0.229 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| [serde_core](https://crates.io/crates/serde_core/1.0.229) | 1.0.229 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| [serde_derive](https://crates.io/crates/serde_derive/1.0.229) | 1.0.229 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| [serde_json](https://crates.io/crates/serde_json/1.0.154) | 1.0.154 | MIT OR Apache-2.0 | https://github.com/serde-rs/json |
| [sha2](https://crates.io/crates/sha2/0.11.1) | 0.11.1 | MIT OR Apache-2.0 | https://github.com/RustCrypto/hashes |
| [simd-adler32](https://crates.io/crates/simd-adler32/0.3.10) | 0.3.10 | MIT | https://github.com/mcountryman/simd-adler32 |
| [skrifa](https://crates.io/crates/skrifa/0.44.0) | 0.44.0 | MIT OR Apache-2.0 | https://github.com/googlefonts/fontations |
| [smallvec](https://crates.io/crates/smallvec/1.16.3) | 1.16.3 | MIT OR Apache-2.0 | https://github.com/servo/rust-smallvec |
| [smol_str](https://crates.io/crates/smol_str/0.2.2) | 0.2.2 | MIT OR Apache-2.0 | https://github.com/rust-analyzer/smol_str |
| [static_assertions](https://crates.io/crates/static_assertions/1.1.0) | 1.1.0 | MIT OR Apache-2.0 | https://github.com/nvzqz/static-assertions-rs |
| [symphonia](https://crates.io/crates/symphonia/0.6.1) | 0.6.1 | MPL-2.0 | https://github.com/pdeljanov/Symphonia |
| [symphonia-bundle-flac](https://crates.io/crates/symphonia-bundle-flac/0.6.1) | 0.6.1 | MPL-2.0 | https://github.com/pdeljanov/Symphonia |
| [symphonia-bundle-mp3](https://crates.io/crates/symphonia-bundle-mp3/0.6.1) | 0.6.1 | MPL-2.0 | https://github.com/pdeljanov/Symphonia |
| [symphonia-codec-aac](https://crates.io/crates/symphonia-codec-aac/0.6.1) | 0.6.1 | MPL-2.0 | https://github.com/pdeljanov/Symphonia |
| [symphonia-codec-pcm](https://crates.io/crates/symphonia-codec-pcm/0.6.1) | 0.6.1 | MPL-2.0 | https://github.com/pdeljanov/Symphonia |
| [symphonia-codec-vorbis](https://crates.io/crates/symphonia-codec-vorbis/0.6.1) | 0.6.1 | MPL-2.0 | https://github.com/pdeljanov/Symphonia |
| [symphonia-common](https://crates.io/crates/symphonia-common/0.6.1) | 0.6.1 | MPL-2.0 | https://github.com/pdeljanov/Symphonia |
| [symphonia-core](https://crates.io/crates/symphonia-core/0.6.1) | 0.6.1 | MPL-2.0 | https://github.com/pdeljanov/Symphonia |
| [symphonia-format-isomp4](https://crates.io/crates/symphonia-format-isomp4/0.6.1) | 0.6.1 | MPL-2.0 | https://github.com/pdeljanov/Symphonia |
| [symphonia-format-ogg](https://crates.io/crates/symphonia-format-ogg/0.6.1) | 0.6.1 | MPL-2.0 | https://github.com/pdeljanov/Symphonia |
| [symphonia-format-riff](https://crates.io/crates/symphonia-format-riff/0.6.1) | 0.6.1 | MPL-2.0 | https://github.com/pdeljanov/Symphonia |
| [symphonia-metadata](https://crates.io/crates/symphonia-metadata/0.6.1) | 0.6.1 | MPL-2.0 | https://github.com/pdeljanov/Symphonia |
| [syn](https://crates.io/crates/syn/2.0.119) | 2.0.119 | MIT OR Apache-2.0 | https://github.com/dtolnay/syn |
| [syn](https://crates.io/crates/syn/3.0.9) | 3.0.9 | MIT OR Apache-2.0 | https://github.com/dtolnay/syn |
| [thiserror](https://crates.io/crates/thiserror/2.0.21) | 2.0.21 | MIT OR Apache-2.0 | https://github.com/dtolnay/thiserror |
| [thiserror-impl](https://crates.io/crates/thiserror-impl/2.0.21) | 2.0.21 | MIT OR Apache-2.0 | https://github.com/dtolnay/thiserror |
| [tiff](https://crates.io/crates/tiff/0.11.3) | 0.11.3 | MIT | https://github.com/image-rs/image-tiff |
| [tracing](https://crates.io/crates/tracing/0.1.44) | 0.1.44 | MIT | https://github.com/tokio-rs/tracing |
| [tracing-core](https://crates.io/crates/tracing-core/0.1.36) | 0.1.36 | MIT | https://github.com/tokio-rs/tracing |
| [type-map](https://crates.io/crates/type-map/0.5.1) | 0.5.1 | MIT/Apache-2.0 | https://github.com/kardeiz/type-map |
| [typed-path](https://crates.io/crates/typed-path/0.12.3) | 0.12.3 | MIT OR Apache-2.0 | https://github.com/chipsenkbeil/typed-path |
| [typenum](https://crates.io/crates/typenum/1.20.1) | 1.20.1 | MIT OR Apache-2.0 | https://github.com/paholg/typenum |
| [unicode-general-category](https://crates.io/crates/unicode-general-category/1.1.0) | 1.1.0 | Apache-2.0 | https://github.com/yeslogic/unicode-general-category |
| [unicode-ident](https://crates.io/crates/unicode-ident/1.0.27) | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | https://github.com/dtolnay/unicode-ident |
| [unicode-properties](https://crates.io/crates/unicode-properties/0.1.4) | 0.1.4 | MIT/Apache-2.0 | https://github.com/unicode-rs/unicode-properties |
| [unicode-segmentation](https://crates.io/crates/unicode-segmentation/1.13.3) | 1.13.3 | MIT OR Apache-2.0 | https://github.com/unicode-rs/unicode-segmentation |
| [unicode-width](https://crates.io/crates/unicode-width/0.2.2) | 0.2.2 | MIT OR Apache-2.0 | https://github.com/unicode-rs/unicode-width |
| [ureq](https://crates.io/crates/ureq/3.4.2) | 3.4.2 | MIT OR Apache-2.0 | https://github.com/algesten/ureq |
| [ureq-proto](https://crates.io/crates/ureq-proto/0.6.5) | 0.6.5 | MIT OR Apache-2.0 | https://github.com/algesten/ureq-proto |
| [utf8-zero](https://crates.io/crates/utf8-zero/0.8.1) | 0.8.1 | MIT OR Apache-2.0 | https://github.com/algesten/utf8-zero |
| [uuid](https://crates.io/crates/uuid/1.28.0) | 1.28.0 | Apache-2.0 OR MIT | https://github.com/uuid-rs/uuid |
| [vello_common](https://crates.io/crates/vello_common/0.2.0) | 0.2.0 | Apache-2.0 OR MIT | https://github.com/linebender/vello |
| [vello_cpu](https://crates.io/crates/vello_cpu/0.2.0) | 0.2.0 | Apache-2.0 OR MIT | https://github.com/linebender/vello |
| [version_check](https://crates.io/crates/version_check/0.9.5) | 0.9.5 | MIT/Apache-2.0 | https://github.com/SergioBenitez/version_check |
| [web-time](https://crates.io/crates/web-time/1.1.0) | 1.1.0 | MIT OR Apache-2.0 | https://github.com/daxpedda/web-time |
| [webpki-root-certs](https://crates.io/crates/webpki-root-certs/1.0.9) | 1.0.9 | CDLA-Permissive-2.0 | https://github.com/rustls/webpki-roots |
| [weezl](https://crates.io/crates/weezl/0.1.12) | 0.1.12 | MIT OR Apache-2.0 | https://github.com/image-rs/weezl |
| [wgpu](https://crates.io/crates/wgpu/30.0.1) | 30.0.1 | MIT OR Apache-2.0 | https://github.com/gfx-rs/wgpu |
| [wgpu-core](https://crates.io/crates/wgpu-core/30.0.1) | 30.0.1 | MIT OR Apache-2.0 | https://github.com/gfx-rs/wgpu |
| [wgpu-core-deps-windows-linux-android](https://crates.io/crates/wgpu-core-deps-windows-linux-android/30.0.1) | 30.0.1 | MIT OR Apache-2.0 | https://github.com/gfx-rs/wgpu |
| [wgpu-hal](https://crates.io/crates/wgpu-hal/30.0.1) | 30.0.1 | MIT OR Apache-2.0 | https://github.com/gfx-rs/wgpu |
| [wgpu-naga-bridge](https://crates.io/crates/wgpu-naga-bridge/30.0.1) | 30.0.1 | MIT OR Apache-2.0 | https://github.com/gfx-rs/wgpu |
| [wgpu-types](https://crates.io/crates/wgpu-types/30.0.1) | 30.0.1 | MIT OR Apache-2.0 | https://github.com/gfx-rs/wgpu |
| [windows](https://crates.io/crates/windows/0.62.2) | 0.62.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [windows-collections](https://crates.io/crates/windows-collections/0.3.2) | 0.3.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [windows-core](https://crates.io/crates/windows-core/0.62.2) | 0.62.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [windows-future](https://crates.io/crates/windows-future/0.3.2) | 0.3.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [windows-implement](https://crates.io/crates/windows-implement/0.60.2) | 0.60.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [windows-interface](https://crates.io/crates/windows-interface/0.59.3) | 0.59.3 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [windows-link](https://crates.io/crates/windows-link/0.2.1) | 0.2.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [windows-numerics](https://crates.io/crates/windows-numerics/0.3.1) | 0.3.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [windows-result](https://crates.io/crates/windows-result/0.4.1) | 0.4.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [windows-strings](https://crates.io/crates/windows-strings/0.5.1) | 0.5.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [windows-sys](https://crates.io/crates/windows-sys/0.52.0) | 0.52.0 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [windows-sys](https://crates.io/crates/windows-sys/0.59.0) | 0.59.0 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [windows-sys](https://crates.io/crates/windows-sys/0.61.2) | 0.61.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [windows-targets](https://crates.io/crates/windows-targets/0.52.6) | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [windows-threading](https://crates.io/crates/windows-threading/0.2.1) | 0.2.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [windows_x86_64_gnu](https://crates.io/crates/windows_x86_64_gnu/0.52.6) | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| [winit](https://crates.io/crates/winit/0.30.13) | 0.30.13 | Apache-2.0 | https://github.com/rust-windowing/winit |
| [xml-rs](https://crates.io/crates/xml-rs/0.8.29) | 0.8.29 | MIT | https://github.com/kornelski/xml-rs |
| [zerocopy](https://crates.io/crates/zerocopy/0.8.63) | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | https://github.com/google/zerocopy |
| [zerocopy-derive](https://crates.io/crates/zerocopy-derive/0.8.63) | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | https://github.com/google/zerocopy |
| [zeroize](https://crates.io/crates/zeroize/1.9.1) | 1.9.1 | Apache-2.0 OR MIT | https://github.com/RustCrypto/utils |
| [zip](https://crates.io/crates/zip/9.0.5) | 9.0.5 | MIT | https://github.com/zip-rs/zip2 |
| [zlib-rs](https://crates.io/crates/zlib-rs/0.6.8) | 0.6.8 | Zlib | https://github.com/trifectatechfoundation/zlib-rs |
| [zmij](https://crates.io/crates/zmij/1.0.23) | 1.0.23 | MIT | https://github.com/dtolnay/zmij |
| [zopfli](https://crates.io/crates/zopfli/0.8.4) | 0.8.4 | Apache-2.0 | https://github.com/zopfli-rs/zopfli |
| [zune-core](https://crates.io/crates/zune-core/0.5.3) | 0.5.3 | MIT OR Apache-2.0 OR Zlib | https://github.com/etemesi254/zune-image |
| [zune-jpeg](https://crates.io/crates/zune-jpeg/0.5.15) | 0.5.15 | MIT OR Apache-2.0 OR Zlib | https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg |

Всего: 203
