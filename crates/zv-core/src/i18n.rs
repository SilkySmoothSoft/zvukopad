//! Перевод интерфейса.
//!
//! Устроено как gettext: в коде пишется русский текст, обёрнутый в `tr("…")` или
//! `tf!("… {} …", значение)`, а файл перевода сопоставляет русскому тексту перевод:
//!
//! ```json
//! { "_name": "English", "Звуки": "Sounds", "Громкость: {} %": "Volume: {} %" }
//! ```
//!
//! * Русский — язык исходного текста, файл перевода ему не нужен.
//! * Английский встроен в программу (`locales/en.json` в корне исходников).
//! * Свой перевод можно добавить без пересборки: положите `xx.json` (xx — код языка,
//!   например `de`) в папку `lang` рядом с программой или в `%APPDATA%\Zvukopad\lang`.
//!   Нет перевода для какой-то строки — показывается английский вариант, если нет и его — русский.
//!
//! `{}` в тексте — место для подстановки значения; их число и порядок в переводе должны совпадать.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Mutex, OnceLock};

type Table = HashMap<&'static str, &'static str>;

/// Встроенный английский перевод.
const EN_JSON: &str = include_str!("../../../locales/en.json");

/// Язык интерфейса.
#[derive(Debug, Clone)]
pub struct Language {
    /// Код: `ru`, `en`, `de`…
    pub code: String,
    /// Название на самом этом языке: «Русский», «English».
    pub native_name: String,
    table: Option<&'static Table>,
}

/// Активная таблица перевода. `null` — русский (исходный текст).
static CURRENT: AtomicPtr<Table> = AtomicPtr::new(std::ptr::null_mut());
/// Запасная таблица (английская) для языков, где переведено не всё.
static FALLBACK: OnceLock<&'static Table> = OnceLock::new();
static LANGUAGES: OnceLock<Mutex<Vec<Language>>> = OnceLock::new();

fn parse(json: &str) -> Result<(String, &'static Table), String> {
    let map: HashMap<String, String> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let name = map.get("_name").cloned().unwrap_or_default();
    let table: Table = map
        .into_iter()
        .filter(|(k, v)| !k.starts_with('_') && !v.is_empty())
        .map(|(k, v)| (&*Box::leak(k.into_boxed_str()), &*Box::leak(v.into_boxed_str())))
        .collect();
    Ok((name, Box::leak(Box::new(table))))
}

fn english() -> &'static Table {
    FALLBACK.get_or_init(|| match parse(EN_JSON) {
        Ok((_, t)) => t,
        Err(e) => {
            log::error!("Встроенный английский перевод повреждён: {e}");
            Box::leak(Box::new(Table::new()))
        }
    })
}

fn languages() -> &'static Mutex<Vec<Language>> {
    LANGUAGES.get_or_init(|| {
        Mutex::new(vec![
            Language { code: "ru".into(), native_name: "Русский".into(), table: None },
            Language { code: "en".into(), native_name: "English".into(), table: Some(english()) },
        ])
    })
}

/// Ищет дополнительные переводы (`*.json`) в указанных папках. Вызывать один раз при запуске.
pub fn load_extra(dirs: &[&Path]) {
    let mut langs = languages().lock().unwrap_or_else(|e| e.into_inner());
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("json")) {
                continue;
            }
            let code = p.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            if code.is_empty() || code == "ru" {
                continue;
            }
            match std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| parse(&t)) {
                Ok((name, table)) => {
                    let native_name = if name.is_empty() { code.clone() } else { name };
                    log::info!("Перевод «{native_name}» ({code}): {} строк, {}", table.len(), p.display());
                    langs.retain(|l| l.code != code);
                    langs.push(Language { code, native_name, table: Some(table) });
                }
                Err(err) => log::warn!("Перевод {} не прочитан: {err}", p.display()),
            }
        }
    }
}

/// Все доступные языки.
pub fn available() -> Vec<Language> {
    languages().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Включает язык по коду. Неизвестный код — английский.
pub fn set_language(code: &str) {
    let langs = languages().lock().unwrap_or_else(|e| e.into_inner());
    let table = match langs.iter().find(|l| l.code.eq_ignore_ascii_case(code)) {
        Some(l) => l.table,
        None => Some(english()),
    };
    let ptr = table.map(|t| t as *const Table as *mut Table).unwrap_or(std::ptr::null_mut());
    CURRENT.store(ptr, Ordering::Relaxed);
}

/// Код текущего языка.
pub fn current_code() -> String {
    let ptr = CURRENT.load(Ordering::Relaxed);
    if ptr.is_null() {
        return "ru".into();
    }
    let langs = languages().lock().unwrap_or_else(|e| e.into_inner());
    langs
        .iter()
        .find(|l| l.table.is_some_and(|t| std::ptr::eq(t, ptr)))
        .map(|l| l.code.clone())
        .unwrap_or_else(|| "en".into())
}

/// Выбирает язык для настройки «как в системе»: код системы, если такой перевод есть, иначе английский.
pub fn resolve_auto(system_code: &str) -> String {
    let langs = languages().lock().unwrap_or_else(|e| e.into_inner());
    let sys = system_code.to_lowercase();
    langs.iter().find(|l| l.code == sys).map(|l| l.code.clone()).unwrap_or_else(|| "en".into())
}

/// Перевод строки. Если перевода нет — исходный (русский) текст.
pub fn tr(s: &str) -> &str {
    let ptr = CURRENT.load(Ordering::Relaxed);
    if ptr.is_null() {
        return s;
    }
    // SAFETY: таблицы создаются один раз через Box::leak и живут до конца программы.
    let table: &'static Table = unsafe { &*ptr };
    if let Some(t) = table.get(s) {
        return t;
    }
    if !std::ptr::eq(table, english())
        && let Some(t) = english().get(s)
    {
        return t;
    }
    s
}

/// Подставляет значения вместо `{}` по порядку. `{{` и `}}` — фигурные скобки.
pub fn fill(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(template.len() + 16);
    let mut it = template.chars().peekable();
    let mut i = 0;
    while let Some(c) = it.next() {
        match c {
            '{' if it.peek() == Some(&'{') => {
                it.next();
                out.push('{');
            }
            '}' if it.peek() == Some(&'}') => {
                it.next();
                out.push('}');
            }
            '{' if it.peek() == Some(&'}') => {
                it.next();
                if let Some(a) = args.get(i) {
                    let _ = write!(out, "{a}");
                }
                i += 1;
            }
            _ => out.push(c),
        }
    }
    out
}

/// Число мест `{}` в шаблоне (для проверки переводов).
pub fn placeholder_count(template: &str) -> usize {
    template.replace("{{", "").replace("}}", "").matches("{}").count()
}

/// Перевод строки с подстановкой: `tf!("Громкость: {} %", pct)`.
#[macro_export]
macro_rules! tf {
    ($lit:literal $(, $arg:expr)* $(,)?) => {
        $crate::i18n::fill($crate::i18n::tr($lit), &[$(&$arg as &dyn ::std::fmt::Display),*])
    };
}

/// Ключи встроенного английского перевода (для проверок).
pub fn english_keys() -> Vec<(&'static str, &'static str)> {
    english().iter().map(|(k, v)| (*k, *v)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_works() {
        assert_eq!(fill("a {} b {} {{x}}", &[&1, &"два"]), "a 1 b два {x}");
        assert_eq!(fill("нет", &[]), "нет");
        assert_eq!(placeholder_count("{} и {{}} и {}"), 2);
    }

    #[test]
    fn english_table_is_consistent() {
        for (k, v) in english_keys() {
            assert_eq!(placeholder_count(k), placeholder_count(v), "разное число {{}}: «{k}» → «{v}»");
        }
    }
}
