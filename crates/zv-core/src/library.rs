//! Библиотека звуков.
//!
//! Звуки — это обычные файлы в папке звуков (по умолчанию `Документы\Zvukopad\Sounds`).
//! Подпапка = категория. Дополнительные данные (горячая клавиша, громкость) хранятся отдельно
//! в `%APPDATA%\Zvukopad\library.json` по относительному пути файла. Так библиотеку можно
//! пополнять, просто копируя файлы в папку.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Расширения, которые умеет декодировать звуковой движок.
pub const SUPPORTED_EXTENSIONS: &[&str] = &["wav", "mp3", "ogg", "oga", "flac", "m4a", "aac", "mp4"];

pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Звук, найденный в папке.
#[derive(Debug, Clone)]
pub struct SoundEntry {
    /// Относительный путь с `/` в качестве разделителя. Это и есть идентификатор звука.
    pub id: String,
    pub path: PathBuf,
    /// Имя файла без расширения.
    pub name: String,
    /// Подпапка (пусто — корень).
    pub category: String,
}

/// Пользовательские данные о звуке.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SoundMeta {
    pub hotkey: Option<String>,
    /// Громкость звука, 1.0 = 100 %.
    pub volume: f32,
    /// Цвет звука (RGB) — в списке и в оверлее.
    pub color: Option<[u8; 3]>,
    /// Теги. Удаление тега не трогает сами звуки.
    pub tags: Vec<String>,
    /// Постоянный номер для набора на нумпаде (как Numcode в Soundpad). Не сдвигается
    /// при удалении других звуков.
    pub code: Option<u32>,
}

impl Default for SoundMeta {
    fn default() -> Self {
        Self { hotkey: None, volume: 1.0, color: None, tags: Vec::new(), code: None }
    }
}

impl SoundMeta {
    fn is_default(&self) -> bool {
        *self == SoundMeta::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LibraryMeta {
    pub sounds: BTreeMap<String, SoundMeta>,
}

impl LibraryMeta {
    pub fn get(&self, id: &str) -> SoundMeta {
        self.sounds.get(id).cloned().unwrap_or_default()
    }

    pub fn set(&mut self, id: &str, meta: SoundMeta) {
        if meta.is_default() {
            self.sounds.remove(id);
        } else {
            self.sounds.insert(id.to_string(), meta);
        }
    }

    /// Изменить данные звука на месте.
    pub fn update(&mut self, id: &str, f: impl FnOnce(&mut SoundMeta)) {
        let mut m = self.get(id);
        f(&mut m);
        self.set(id, m);
    }

    /// Звук переименован или перемещён — переносим его данные.
    pub fn rename(&mut self, old_id: &str, new_id: &str) {
        if let Some(meta) = self.sounds.remove(old_id) {
            self.sounds.insert(new_id.to_string(), meta);
        }
    }

    /// Какой звук уже занимает эту горячую клавишу (без учёта регистра).
    pub fn hotkey_owner(&self, hotkey: &str) -> Option<&str> {
        self.sounds
            .iter()
            .find(|(_, m)| m.hotkey.as_deref().is_some_and(|h| h.eq_ignore_ascii_case(hotkey)))
            .map(|(id, _)| id.as_str())
    }

    /// Какой звук носит этот номер.
    pub fn code_owner(&self, code: u32) -> Option<&str> {
        self.sounds.iter().find(|(_, m)| m.code == Some(code)).map(|(id, _)| id.as_str())
    }

    /// Выдаёт номера звукам, у которых их нет (следующий свободный, начиная с 1).
    /// Звуки берутся в порядке списка. Возвращает `true`, если что-то поменялось.
    pub fn assign_codes<'a>(&mut self, ids: impl IntoIterator<Item = &'a str>) -> bool {
        let mut used: std::collections::BTreeSet<u32> = self.sounds.values().filter_map(|m| m.code).collect();
        let mut next = 1u32;
        let mut changed = false;
        for id in ids {
            if self.sounds.get(id).is_some_and(|m| m.code.is_some()) {
                continue;
            }
            while used.contains(&next) {
                next += 1;
            }
            used.insert(next);
            self.update(id, |m| m.code = Some(next));
            changed = true;
        }
        changed
    }

    /// Все теги с числом звуков, по алфавиту.
    pub fn all_tags(&self) -> Vec<(String, usize)> {
        let mut map: BTreeMap<String, usize> = BTreeMap::new();
        for m in self.sounds.values() {
            for t in &m.tags {
                *map.entry(t.clone()).or_default() += 1;
            }
        }
        let mut v: Vec<_> = map.into_iter().collect();
        v.sort_by_key(|(t, _)| t.to_lowercase());
        v
    }

    /// Удаляет тег у всех звуков (сами звуки остаются).
    pub fn remove_tag(&mut self, tag: &str) {
        let ids: Vec<String> = self.sounds.keys().cloned().collect();
        for id in ids {
            self.update(&id, |m| m.tags.retain(|t| t != tag));
        }
    }

    /// Переименовывает тег у всех звуков (если новый уже есть — теги сливаются).
    pub fn rename_tag(&mut self, old: &str, new: &str) {
        let new = normalize_tag(new);
        if new.is_empty() {
            return;
        }
        let ids: Vec<String> = self.sounds.keys().cloned().collect();
        for id in ids {
            self.update(&id, |m| {
                if m.tags.iter().any(|t| t == old) {
                    m.tags.retain(|t| t != old && *t != new);
                    m.tags.push(new.clone());
                    m.tags.sort_by_key(|t| t.to_lowercase());
                }
            });
        }
    }

    /// Убирает данные звуков, файлов которых больше нет.
    pub fn prune<'a>(&mut self, existing: impl IntoIterator<Item = &'a str>) {
        let keep: std::collections::HashSet<&str> = existing.into_iter().collect();
        self.sounds.retain(|id, _| keep.contains(id.as_str()));
    }
}

/// Тег: без пробелов по краям, не длиннее 32 символов.
pub fn normalize_tag(s: &str) -> String {
    s.trim().trim_start_matches('#').chars().take(32).collect::<String>().trim().to_string()
}

/// Относительный путь → идентификатор.
pub fn id_for(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    Some(parts.join("/"))
}

/// Сканирует папку звуков (рекурсивно). Папки, начинающиеся с `.` или `_`, пропускаются.
pub fn scan(root: &Path) -> Vec<SoundEntry> {
    let mut out = Vec::new();
    scan_dir(root, root, &mut out, 0);
    out.sort_by(|a, b| {
        a.category
            .to_lowercase()
            .cmp(&b.category.to_lowercase())
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

fn scan_dir(root: &Path, dir: &Path, out: &mut Vec<SoundEntry>, depth: usize) {
    if depth > 8 {
        return;
    }
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            if !file_name.starts_with('.') && !file_name.starts_with('_') {
                scan_dir(root, &path, out, depth + 1);
            }
        } else if ft.is_file() && is_supported(&path) {
            let Some(id) = id_for(root, &path) else { continue };
            let category = path.parent().and_then(|p| id_for(root, p)).unwrap_or_default();
            let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            out.push(SoundEntry { id, path, name, category });
        }
    }
}

/// Убирает из имени файла символы, запрещённые в Windows.
pub fn sanitize_file_name(name: &str) -> String {
    let cleaned: String =
        name.chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '_' } else { c }).collect();
    let trimmed = cleaned.trim().trim_end_matches('.').trim();
    let mut s: String = trimmed.chars().take(80).collect();
    if s.is_empty() {
        s = crate::i18n::tr("звук").into();
    }
    // Зарезервированные имена Windows.
    let upper = s.to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"];
    if reserved.contains(&upper.as_str())
        || ((upper.starts_with("COM") || upper.starts_with("LPT")) && upper.len() == 4)
    {
        s.push('_');
    }
    s
}

/// Свободный путь: `name.ext`, `name (2).ext`, `name (3).ext`…
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let make = |n: usize| {
        if n == 1 { dir.join(format!("{stem}.{ext}")) } else { dir.join(format!("{stem} ({n}).{ext}")) }
    };
    let mut n = 1;
    loop {
        let p = make(n);
        if !p.exists() {
            return p;
        }
        n += 1;
    }
}

/// Копирует файл в папку звуков (в подпапку `category`, если не пустая). Возвращает новый путь.
pub fn import_file(root: &Path, category: &str, src: &Path) -> std::io::Result<PathBuf> {
    if !is_supported(src) {
        return Err(std::io::Error::other(crate::i18n::tr("формат не поддерживается")));
    }
    let dir = if category.is_empty() { root.to_path_buf() } else { root.join(category) };
    std::fs::create_dir_all(&dir)?;
    let stem = sanitize_file_name(&src.file_stem().unwrap_or_default().to_string_lossy());
    let ext = src.extension().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    let dst = unique_path(&dir, &stem, &ext);
    std::fs::copy(src, &dst)?;
    Ok(dst)
}

/// Переименовывает звук на диске (расширение сохраняется). Возвращает новый путь.
pub fn rename_file(path: &Path, new_name: &str) -> std::io::Result<PathBuf> {
    let dir = path.parent().ok_or_else(|| std::io::Error::other(crate::i18n::tr("нет родительской папки")))?;
    let ext = path.extension().unwrap_or_default().to_string_lossy().into_owned();
    let stem = sanitize_file_name(new_name);
    let target = dir.join(format!("{stem}.{ext}"));
    if target == path {
        return Ok(target);
    }
    if target.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            crate::i18n::tr("файл с таким именем уже есть"),
        ));
    }
    std::fs::rename(path, &target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize() {
        assert_eq!(sanitize_file_name("a:b?c"), "a_b_c");
        assert_eq!(sanitize_file_name("  "), "звук");
        assert_eq!(sanitize_file_name("con"), "con_");
        assert_eq!(sanitize_file_name("Привет."), "Привет");
    }

    #[test]
    fn scan_and_ids() {
        let root = std::env::temp_dir().join(format!("zv-lib-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Мемы")).unwrap();
        std::fs::create_dir_all(root.join("_skip")).unwrap();
        std::fs::write(root.join("a.wav"), b"x").unwrap();
        std::fs::write(root.join("Мемы").join("б.mp3"), b"x").unwrap();
        std::fs::write(root.join("_skip").join("c.wav"), b"x").unwrap();
        std::fs::write(root.join("notes.txt"), b"x").unwrap();
        let items = scan(&root);
        let ids: Vec<_> = items.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["a.wav", "Мемы/б.mp3"]);
        assert_eq!(items[1].category, "Мемы");
        let p = unique_path(&root, "a", "wav");
        assert!(p.ends_with("a (2).wav"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn meta_hotkey_owner() {
        let mut m = LibraryMeta::default();
        m.set("x.wav", SoundMeta { hotkey: Some("Ctrl+1".into()), ..Default::default() });
        assert_eq!(m.hotkey_owner("ctrl+1"), Some("x.wav"));
        m.rename("x.wav", "y.wav");
        assert_eq!(m.hotkey_owner("CTRL+1"), Some("y.wav"));
        m.set("y.wav", SoundMeta::default());
        assert!(m.sounds.is_empty());
    }

    #[test]
    fn codes_are_stable() {
        let mut m = LibraryMeta::default();
        assert!(m.assign_codes(["a", "b", "c"]));
        assert_eq!(m.get("b").code, Some(2));
        m.set("b", SoundMeta::default());
        m.assign_codes(["a", "c", "d"]);
        assert_eq!(m.get("c").code, Some(3));
        assert_eq!(m.get("d").code, Some(2)); // занимает освободившийся номер
        assert_eq!(m.code_owner(3), Some("c"));
    }

    #[test]
    fn tags() {
        let mut m = LibraryMeta::default();
        m.update("a", |x| x.tags = vec!["мемы".into(), "смех".into()]);
        m.update("b", |x| x.tags = vec!["мемы".into()]);
        assert_eq!(m.all_tags(), vec![("мемы".into(), 2), ("смех".into(), 1)]);
        m.rename_tag("мемы", "приколы");
        assert_eq!(m.get("b").tags, vec!["приколы".to_string()]);
        m.remove_tag("приколы");
        assert!(m.get("b").tags.is_empty());
        assert_eq!(m.get("a").tags, vec!["смех".to_string()]);
        assert_eq!(normalize_tag("  #смех "), "смех");
    }
}
