//! Звуковой движок Звукопада.
//!
//! * [`Engine`] — устройства, микширование «себе» и «в микрофон», подмешивание микрофона, запись.
//! * [`sound::decode_file`] — загрузка звуков (wav, mp3, ogg, flac, m4a).
//! * [`devices::list_devices`] — список устройств.

pub mod devices;
pub mod effects;
pub mod engine;
mod mixer;
mod recorder;
mod replay;
pub mod sound;
mod streams;

pub use devices::{DeviceInfo, DeviceList, list_devices};
pub use engine::{
    Engine, EngineConfig, EngineStatus, Levels, RecordingResult, RecordingStatus, ReplayInfo, StreamStatus,
};
pub use sound::{SoundData, decode_file};
