//! Синтез речи в фоновом потоке, чтобы окно не зависало, пока плагин думает.

use std::collections::HashMap;
use std::path::PathBuf;
use zv_core::i18n::tr;

use crossbeam_channel::{Receiver, Sender};
use zv_plugins::{PluginKind, PluginManifest, SynthRequest, TtsPlugin, TtsVoice};

use crate::piper::{self, PIPER_ID, PiperPaths};

/// Зачем синтезируем — от этого зависит, что сделать с результатом.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynthPurpose {
    /// Прослушать самому.
    Preview,
    /// Сказать (туда, куда выбран вывод).
    Say,
    /// Сохранить в библиотеку.
    Save,
}

enum Job {
    ListVoices { plugin_id: String },
    Synthesize { plugin_id: String, request: SynthRequest, purpose: SynthPurpose },
}

pub enum TtsEvent {
    Voices { plugin_id: String, result: Result<Vec<TtsVoice>, String> },
    Synthesized { purpose: SynthPurpose, result: Result<PathBuf, String> },
}

pub struct TtsService {
    tx: Sender<Job>,
    rx: Receiver<TtsEvent>,
    pub plugins: Vec<PluginManifest>,
    pub discovery_errors: Vec<String>,
    pub busy: bool,
}

impl TtsService {
    pub fn new(plugin_dirs: Vec<PathBuf>, piper_paths: PiperPaths, repaint: impl Fn() + Send + 'static) -> Self {
        let (all, discovery_errors) = zv_plugins::discover(&plugin_dirs);
        for e in &discovery_errors {
            log::warn!("Плагин: {e}");
        }
        let mut plugins: Vec<PluginManifest> = all.into_iter().filter(|p| p.kind == PluginKind::Tts).collect();
        // Встроенный Piper — всегда в списке, даже если ещё не скачан.
        plugins.retain(|p| p.id != PIPER_ID);
        plugins.push(piper::manifest(&piper_paths));
        log::info!("Плагинов синтеза речи: {}", plugins.len());

        let (job_tx, job_rx) = crossbeam_channel::unbounded::<Job>();
        let (ev_tx, ev_rx) = crossbeam_channel::unbounded::<TtsEvent>();
        let manifests = plugins.clone();
        std::thread::Builder::new()
            .name("zv-tts".into())
            .spawn(move || worker(manifests, piper_paths, job_rx, ev_tx, repaint))
            .expect("не удалось запустить поток синтеза речи");
        Self { tx: job_tx, rx: ev_rx, plugins, discovery_errors, busy: false }
    }

    pub fn request_voices(&self, plugin_id: &str) {
        let _ = self.tx.send(Job::ListVoices { plugin_id: plugin_id.to_string() });
    }

    pub fn synthesize(&mut self, plugin_id: &str, request: SynthRequest, purpose: SynthPurpose) {
        self.busy = true;
        let _ = self.tx.send(Job::Synthesize { plugin_id: plugin_id.to_string(), request, purpose });
    }

    pub fn take_events(&mut self) -> Vec<TtsEvent> {
        let events: Vec<TtsEvent> = self.rx.try_iter().collect();
        if events.iter().any(|e| matches!(e, TtsEvent::Synthesized { .. })) {
            self.busy = false;
        }
        events
    }
}

fn worker(
    manifests: Vec<PluginManifest>,
    piper_paths: PiperPaths,
    rx: Receiver<Job>,
    tx: Sender<TtsEvent>,
    repaint: impl Fn(),
) {
    let mut plugins: HashMap<String, TtsPlugin> =
        manifests.into_iter().filter(|m| m.id != PIPER_ID).map(|m| (m.id.clone(), TtsPlugin::new(m))).collect();
    while let Ok(job) = rx.recv() {
        let event = match job {
            Job::ListVoices { plugin_id } => {
                let result = match plugins.get_mut(&plugin_id) {
                    _ if plugin_id == PIPER_ID => piper::list_voices(&piper_paths),
                    Some(p) => p.list_voices(),
                    None => Err(tr("плагин не найден").into()),
                };
                if let Err(e) = &result {
                    log::error!("Голоса плагина {plugin_id}: {e}");
                }
                TtsEvent::Voices { plugin_id, result }
            }
            Job::Synthesize { plugin_id, request, purpose } => {
                let result = match plugins.get_mut(&plugin_id) {
                    _ if plugin_id == PIPER_ID => piper::synthesize(&piper_paths, &request),
                    Some(p) => p.synthesize(&request),
                    None => Err(tr("плагин не найден").into()),
                };
                if let Err(e) = &result {
                    log::error!("Синтез ({plugin_id}): {e}");
                }
                TtsEvent::Synthesized { purpose, result }
            }
        };
        if tx.send(event).is_err() {
            break;
        }
        repaint();
    }
}
