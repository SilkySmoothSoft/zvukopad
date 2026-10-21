//! Проверка протокола на примере плагина из docs/examples (нужен python3 в PATH).

use std::path::PathBuf;

use zv_plugins::{PluginManifest, SynthRequest, TtsPlugin};

#[test]
fn python_example_plugin_works() {
    if std::process::Command::new("python3").arg("--version").output().is_err() {
        eprintln!("python3 не найден — тест пропущен");
        return;
    }
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples/python-tts-example");
    let dir = std::env::temp_dir().join(format!("zv-py-плагин-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(src.join("plugin.py"), dir.join("plugin.py")).unwrap();
    let manifest_text = std::fs::read_to_string(src.join("plugin.json")).unwrap().replace("\"python\"", "\"python3\"");
    std::fs::write(dir.join("plugin.json"), manifest_text).unwrap();

    let manifest = PluginManifest::load(&dir).unwrap();
    let mut plugin = TtsPlugin::new(manifest);
    let voices = plugin.list_voices().unwrap();
    assert_eq!(voices.len(), 2);
    assert_eq!(voices[0].name, "Писк высокий");

    let out = dir.join("выход звук.wav");
    let path = plugin
        .synthesize(&SynthRequest {
            text: "Привет, мир".into(),
            voice: voices[1].id.clone(),
            rate: 0,
            output: out.clone(),
        })
        .unwrap();
    assert_eq!(path, out);
    assert!(std::fs::metadata(&out).unwrap().len() > 1000);

    // Ошибка плагина доходит до программы текстом.
    let err = plugin
        .synthesize(&SynthRequest { text: "x".into(), voice: "v".into(), rate: 0, output: dir.clone() })
        .unwrap_err();
    assert!(!err.is_empty());
    drop(plugin);
    std::fs::remove_dir_all(&dir).unwrap();
}
