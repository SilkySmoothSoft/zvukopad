//! Скрытие личных данных в интерфейсе: пути к папкам содержат имя пользователя Windows.
//!
//! Когда включено «скрывать пути», пути на экране заменяются значком «скрыто», а в журнале
//! и всплывающих сообщениях домашняя папка и имя пользователя заменяются заглушками.

use std::path::Path;

/// Заменяет `needle` на `rep` без учёта регистра (пути Windows регистр не различают).
fn replace_ci(hay: &str, needle: &str, rep: &str) -> String {
    if needle.is_empty() {
        return hay.to_string();
    }
    let lower_hay = hay.to_lowercase();
    let lower_needle = needle.to_lowercase();
    // Если при смене регистра длина в байтах изменилась — безопаснее точное совпадение.
    if lower_hay.len() != hay.len() || lower_needle.len() != needle.len() {
        return hay.replace(needle, rep);
    }
    let mut out = String::with_capacity(hay.len());
    let mut last = 0;
    for (i, _) in lower_hay.match_indices(&lower_needle) {
        if i < last {
            continue;
        }
        out.push_str(&hay[last..i]);
        out.push_str(rep);
        last = i + needle.len();
    }
    out.push_str(&hay[last..]);
    out
}

/// Что и на что заменять. Порядок важен: сначала длинные пути.
pub struct Masker {
    rules: Vec<(String, String)>,
    user: String,
}

impl Masker {
    pub fn new(sounds_dir: &Path, extra: &[(&Path, &str)]) -> Self {
        let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
        let user = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
        let mut rules: Vec<(String, String)> = Vec::new();
        for (p, rep) in extra {
            rules.push((p.display().to_string(), rep.to_string()));
        }
        // Своя папка звуков может лежать где угодно (D:\Вася\Звуки) — прячем её целиком.
        let sd = sounds_dir.display().to_string();
        if home.is_empty() || !sd.to_lowercase().starts_with(&home.to_lowercase()) {
            rules.push((sd, "‹Sounds›".into()));
        }
        if !home.is_empty() {
            rules.push((home, if cfg!(windows) { "%USERPROFILE%".into() } else { "~".into() }));
        }
        rules.retain(|(from, _)| from.len() > 3);
        rules.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
        Self { rules, user }
    }

    /// Текст с замаскированными путями и именем пользователя.
    pub fn mask(&self, text: &str) -> String {
        let mut s = text.to_string();
        for (from, rep) in &self.rules {
            s = replace_ci(&s, from, rep);
        }
        if self.user.chars().count() >= 2 {
            for sep in ['\\', '/'] {
                s = replace_ci(&s, &format!("{sep}{}{sep}", self.user), &format!("{sep}•••{sep}"));
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_paths() {
        let m = Masker {
            rules: vec![
                ("C:\\Users\\Вася".into(), "%USERPROFILE%".into()),
                ("D:\\Мои звуки".into(), "‹Sounds›".into()),
            ],
            user: "Вася".into(),
        };
        assert_eq!(m.mask("C:\\Users\\Вася\\Documents\\a.wav"), "%USERPROFILE%\\Documents\\a.wav");
        assert_eq!(m.mask("файл D:\\Мои звуки\\x.wav"), "файл ‹Sounds›\\x.wav");
        assert_eq!(m.mask("E:\\Вася\\x"), "E:\\•••\\x");
        assert_eq!(replace_ci("C:\\USERS\\bob\\x", "c:\\users\\bob", "~"), "~\\x");
    }
}
