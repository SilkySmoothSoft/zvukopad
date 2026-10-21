//! Список аудиоустройств и поиск устройства по сохранённой ссылке.

use cpal::traits::{DeviceTrait, HostTrait};
use zv_core::settings::DeviceRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Стабильный идентификатор (сохраняется в настройках).
    pub id: String,
    pub name: String,
}

impl DeviceInfo {
    pub fn to_ref(&self) -> DeviceRef {
        DeviceRef { id: self.id.clone(), name: self.name.clone() }
    }

    /// Похоже ли устройство на виртуальный кабель (VB-Cable, VoiceMeeter и т. п.).
    pub fn looks_virtual(&self) -> bool {
        let n = self.name.to_lowercase();
        ["cable", "voicemeeter", "virtual", "vb-audio"].iter().any(|k| n.contains(k))
    }
}

#[derive(Debug, Clone, Default)]
pub struct DeviceList {
    /// Микрофоны и другие входы.
    pub inputs: Vec<DeviceInfo>,
    /// Колонки, наушники, виртуальные кабели. Их же можно записывать (loopback).
    pub outputs: Vec<DeviceInfo>,
    pub default_input: Option<String>,
    pub default_output: Option<String>,
}

fn info(d: &cpal::Device) -> Option<DeviceInfo> {
    let name = d.description().ok()?.name().to_string();
    let id = d.id().map(|i| i.to_string()).unwrap_or_else(|_| name.clone());
    Some(DeviceInfo { id, name })
}

pub fn list_devices() -> DeviceList {
    let host = cpal::default_host();
    let mut list = DeviceList::default();
    match host.input_devices() {
        Ok(devs) => list.inputs = devs.filter_map(|d| info(&d)).collect(),
        Err(e) => log::error!("Не удалось получить список входов: {e}"),
    }
    match host.output_devices() {
        Ok(devs) => list.outputs = devs.filter_map(|d| info(&d)).collect(),
        Err(e) => log::error!("Не удалось получить список выходов: {e}"),
    }
    list.default_input = host.default_input_device().and_then(|d| info(&d)).map(|i| i.name);
    list.default_output = host.default_output_device().and_then(|d| info(&d)).map(|i| i.name);
    list
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Direction {
    Input,
    Output,
}

/// Находит устройство: сначала по идентификатору, потом по имени.
/// `None` в ссылке — устройство Windows по умолчанию.
pub(crate) fn find_device(r: Option<&DeviceRef>, dir: Direction) -> Option<cpal::Device> {
    let host = cpal::default_host();
    let Some(r) = r else {
        return match dir {
            Direction::Input => host.default_input_device(),
            Direction::Output => host.default_output_device(),
        };
    };
    let devices: Vec<cpal::Device> = match dir {
        Direction::Input => host.input_devices().map(|d| d.collect()).unwrap_or_default(),
        Direction::Output => host.output_devices().map(|d| d.collect()).unwrap_or_default(),
    };
    let by_id = devices.iter().find(|d| d.id().map(|i| i.to_string() == r.id).unwrap_or(false));
    if let Some(d) = by_id {
        return Some(d.clone());
    }
    devices.iter().find(|d| d.description().map(|x| x.name() == r.name).unwrap_or(false)).cloned()
}

pub(crate) fn device_name(d: &cpal::Device) -> String {
    d.description().map(|x| x.name().to_string()).unwrap_or_else(|_| "?".into())
}
