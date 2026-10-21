//! Создание потоков cpal для любого формата сэмплов, который отдаёт устройство.

use cpal::traits::DeviceTrait;
use cpal::{FromSample, SampleFormat, SizedSample, SupportedStreamConfig};
use zv_core::tf;

use crate::mixer::BusRenderer;

/// Размер внутреннего блока микширования, кадров.
const BLOCK: usize = 2048;

pub(crate) type ErrFn = Box<dyn FnMut(cpal::Error) + Send + 'static>;

macro_rules! dispatch_format {
    ($fmt:expr, $func:ident, $($arg:expr),*) => {
        match $fmt {
            SampleFormat::F32 => $func::<f32>($($arg),*),
            SampleFormat::F64 => $func::<f64>($($arg),*),
            SampleFormat::I16 => $func::<i16>($($arg),*),
            SampleFormat::I32 => $func::<i32>($($arg),*),
            SampleFormat::I24 => $func::<cpal::I24>($($arg),*),
            SampleFormat::U16 => $func::<u16>($($arg),*),
            SampleFormat::U8 => $func::<u8>($($arg),*),
            SampleFormat::I8 => $func::<i8>($($arg),*),
            other => Err(tf!("формат сэмплов {} не поддерживается", format!("{:?}", other))),
        }
    };
}

/// Выходной поток, который берёт звук из микшера.
pub(crate) fn build_output(
    device: &cpal::Device,
    config: SupportedStreamConfig,
    renderer: BusRenderer,
    err: ErrFn,
) -> Result<cpal::Stream, String> {
    dispatch_format!(config.sample_format(), build_output_typed, device, config, renderer, err)
}

fn build_output_typed<T>(
    device: &cpal::Device,
    config: SupportedStreamConfig,
    mut renderer: BusRenderer,
    err: ErrFn,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels() as usize;
    let mut scratch = vec![0.0f32; BLOCK * 2];
    device
        .build_output_stream::<T, _, _>(
            config.config(),
            move |data: &mut [T], _| {
                let total = data.len() / channels.max(1);
                let mut done = 0;
                while done < total {
                    let n = (total - done).min(BLOCK);
                    let buf = &mut scratch[..n * 2];
                    renderer.render(buf);
                    for f in 0..n {
                        let (l, r) = (buf[f * 2], buf[f * 2 + 1]);
                        let dst = &mut data[(done + f) * channels..(done + f + 1) * channels];
                        if channels == 1 {
                            dst[0] = T::from_sample((l + r) * 0.5);
                        } else {
                            dst[0] = T::from_sample(l);
                            dst[1] = T::from_sample(r);
                            for c in &mut dst[2..] {
                                *c = T::EQUILIBRIUM;
                            }
                        }
                    }
                    done += n;
                }
            },
            err,
            None,
        )
        .map_err(|e| e.to_string())
}

/// Входной поток: каждый блок переводится в f32 и передаётся в `on_data(сэмплы, каналы)`.
pub(crate) fn build_input<F>(
    device: &cpal::Device,
    config: SupportedStreamConfig,
    on_data: F,
    err: ErrFn,
) -> Result<cpal::Stream, String>
where
    F: FnMut(&[f32], usize) + Send + 'static,
{
    dispatch_format!(config.sample_format(), build_input_typed, device, config, on_data, err)
}

fn build_input_typed<T>(
    device: &cpal::Device,
    config: SupportedStreamConfig,
    mut on_data: impl FnMut(&[f32], usize) + Send + 'static,
    err: ErrFn,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels() as usize;
    let mut scratch: Vec<f32> = Vec::with_capacity(BLOCK * 8);
    device
        .build_input_stream::<T, _, _>(
            config.config(),
            move |data: &[T], _| {
                for chunk in data.chunks(BLOCK * channels.max(1)) {
                    scratch.clear();
                    scratch.extend(chunk.iter().map(|s| s.to_sample::<f32>()));
                    on_data(&scratch, channels);
                }
            },
            err,
            None,
        )
        .map_err(|e| e.to_string())
}
