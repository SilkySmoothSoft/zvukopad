//! Запуск плагина как отдельного процесса и обмен строками JSON.
//!
//! Запрос:  `{"id":1,"method":"list_voices","params":{}}`
//! Ответ:   `{"id":1,"result":{...}}`  или  `{"id":1,"error":{"message":"..."}}`
//!
//! Все не-ASCII символы в запросах передаются как `\uXXXX`, поэтому кодировка консоли
//! плагина (особенно PowerShell) не имеет значения.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use zv_core::i18n::tr;
use zv_core::tf;

use serde_json::{Value, json};

use crate::manifest::PluginManifest;

pub struct PluginProcess {
    manifest: PluginManifest,
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<String>,
    next_id: u64,
}

impl PluginProcess {
    pub fn spawn(manifest: &PluginManifest) -> Result<Self, String> {
        let program = manifest.resolved_command();
        let mut cmd = Command::new(&program);
        cmd.args(&manifest.args)
            .current_dir(&manifest.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONUTF8", "1");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| tf!("не удалось запустить плагин «{}» ({}): {}", tr(&manifest.name), program.display(), e))?;
        let stdin = child.stdin.take().ok_or(tr("нет stdin"))?;
        let stdout = child.stdout.take().ok_or(tr("нет stdout"))?;
        let stderr = child.stderr.take().ok_or(tr("нет stderr"))?;

        let (tx, rx) = mpsc::channel();
        let name = manifest.id.clone();
        std::thread::Builder::new()
            .name(format!("plugin-{name}-out"))
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        let name2 = manifest.id.clone();
        std::thread::Builder::new()
            .name(format!("plugin-{name2}-err"))
            .spawn(move || {
                // Всё, что плагин пишет в stderr, попадает в лог программы.
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    log::warn!("[плагин {name2}] {line}");
                }
            })
            .map_err(|e| e.to_string())?;

        log::info!("Плагин «{}» запущен", manifest.name);
        Ok(Self { manifest: manifest.clone(), child, stdin, lines: rx, next_id: 1 })
    }

    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Отправляет запрос и ждёт ответ не дольше `timeout`.
    pub fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({ "id": id, "method": method, "params": params });
        let line = ascii_json(&msg.to_string());
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(|e| tf!("плагин не принимает запросы: {}", e))?;

        let deadline = std::time::Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Err(tf!("плагин не ответил за {} с", timeout.as_secs()));
            }
            let line = match self.lines.recv_timeout(left) {
                Ok(l) => l,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(tr("плагин завершился (подробности в логе)").into());
                }
            };
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                log::warn!("[плагин {}] не JSON: {line}", self.manifest.id);
                continue;
            };
            if v.get("id").and_then(Value::as_u64) != Some(id) {
                continue; // ответ на старый запрос (например, после таймаута)
            }
            if let Some(err) = v.get("error") {
                let msg =
                    err.get("message").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| err.to_string());
                return Err(msg);
            }
            return Ok(v.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

impl Drop for PluginProcess {
    fn drop(&mut self) {
        // Вежливо просим завершиться, потом добиваем.
        let _ = self.stdin.write_all(b"{\"id\":0,\"method\":\"shutdown\",\"params\":{}}\n");
        let _ = self.stdin.flush();
        for _ in 0..10 {
            if !self.is_alive() {
                return;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Заменяет все не-ASCII символы на `\uXXXX`. Безопасно для готовой JSON-строки:
/// не-ASCII символы в JSON встречаются только внутри строк.
pub fn ascii_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut buf = [0u16; 2];
            for unit in c.encode_utf16(&mut buf) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_roundtrip() {
        let original = json!({"text": "Привет, мир! 🎤", "path": "C:\\Пользователи\\я"});
        let encoded = ascii_json(&original.to_string());
        assert!(encoded.is_ascii());
        let back: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(back, original);
    }
}
