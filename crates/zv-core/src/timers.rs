//! Таймеры: запись или воспроизведение через заданное время или в заданное время суток.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TimerKind {
    /// Записать звук длиной `secs` секунд (источник — из настроек записи).
    Record { secs: u32 },
    /// Сыграть звук из библиотеки.
    PlaySound { id: String },
    /// Запустить очередь.
    PlayQueue,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TimerStart {
    /// Через столько секунд после создания (или после предыдущего срабатывания).
    After { secs: u32 },
    /// В это время суток (по часам компьютера).
    At { hour: u8, minute: u8 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimerTask {
    pub id: u64,
    pub enabled: bool,
    pub kind: TimerKind,
    pub start: TimerStart,
    /// Повторять: для «через» — каждые столько же секунд, для «в» — каждый день.
    pub repeat: bool,
    /// Когда сработает в следующий раз (секунды Unix).
    pub next_due: Option<u64>,
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl TimerTask {
    /// Считает следующее срабатывание. `local_secs_of_day` — сколько секунд прошло
    /// с местной полуночи сейчас.
    pub fn schedule(&mut self, now: u64, local_secs_of_day: u32) {
        self.next_due = Some(match self.start {
            TimerStart::After { secs } => now + secs.max(1) as u64,
            TimerStart::At { hour, minute } => {
                let target = hour as u32 * 3600 + minute as u32 * 60;
                let wait = (target + 86_400 - local_secs_of_day % 86_400) % 86_400;
                now + if wait == 0 { 86_400 } else { wait } as u64
            }
        });
    }

    /// После срабатывания: следующий раз или выключение.
    pub fn after_fire(&mut self, now: u64, local_secs_of_day: u32) {
        if self.repeat {
            self.schedule(now, local_secs_of_day);
        } else {
            self.enabled = false;
            self.next_due = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_time_wraps_to_tomorrow() {
        let mut t = TimerTask {
            id: 1,
            enabled: true,
            kind: TimerKind::PlayQueue,
            start: TimerStart::At { hour: 8, minute: 0 },
            repeat: false,
            next_due: None,
        };
        // Сейчас 09:00 — следующее 08:00 через 23 часа.
        t.schedule(1000, 9 * 3600);
        assert_eq!(t.next_due, Some(1000 + 23 * 3600));
        // Сейчас 07:30 — через 30 минут.
        t.schedule(1000, 7 * 3600 + 1800);
        assert_eq!(t.next_due, Some(1000 + 1800));
        t.after_fire(2000, 0);
        assert!(!t.enabled);
    }
}
