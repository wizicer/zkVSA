use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug)]
#[allow(dead_code)]
pub struct WavFmt {
    pub audio_format: u16,
    pub num_channels: u16,
    pub sample_rate: u32,
    pub byte_rate: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
}

#[derive(Debug)]
pub struct WavData {
    pub fmt: WavFmt,
    pub samples: Vec<i16>, // mono 16-bit PCM only
}

fn read_le_u16(buf: &[u8]) -> u16 {
    u16::from_le_bytes([buf[0], buf[1]])
}

fn read_le_u32(buf: &[u8]) -> u32 {
    u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
}

pub fn parse_wav_mono_16le(path: &Path) -> Result<WavData, String> {
    let mut f = File::open(path).map_err(|e| format!("Failed to open file: {e}"))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    if buf.len() < 44 {
        return Err("File too short to be a valid WAV".into());
    }

    if &buf[0..4] != b"RIFF" {
        return Err("Missing RIFF header".into());
    }
    let _chunk_size = read_le_u32(&buf[4..8]);
    if &buf[8..12] != b"WAVE" {
        return Err("Missing WAVE header".into());
    }

    let mut idx = 12usize;
    let mut fmt: Option<WavFmt> = None;
    let mut data_start: Option<usize> = None;
    let mut data_size: Option<usize> = None;

    while idx + 8 <= buf.len() {
        let chunk_id = &buf[idx..idx + 4];
        let size = read_le_u32(&buf[idx + 4..idx + 8]) as usize;
        let data_idx = idx + 8;
        if data_idx + size > buf.len() {
            return Err("Invalid chunk size exceeding file length".into());
        }

        match chunk_id {
            b"fmt " => {
                if size < 16 {
                    return Err("fmt chunk too small".into());
                }
                let audio_format = read_le_u16(&buf[data_idx..data_idx + 2]);
                let num_channels = read_le_u16(&buf[data_idx + 2..data_idx + 4]);
                let sample_rate = read_le_u32(&buf[data_idx + 4..data_idx + 8]);
                let byte_rate = read_le_u32(&buf[data_idx + 8..data_idx + 12]);
                let block_align = read_le_u16(&buf[data_idx + 12..data_idx + 14]);
                let bits_per_sample = read_le_u16(&buf[data_idx + 14..data_idx + 16]);

                fmt = Some(WavFmt {
                    audio_format,
                    num_channels,
                    sample_rate,
                    byte_rate,
                    block_align,
                    bits_per_sample,
                });
            }
            b"data" => {
                data_start = Some(data_idx);
                data_size = Some(size);
            }
            _ => {}
        }

        let advance = 8 + size + (size % 2);
        idx = idx.saturating_add(advance);
    }

    let fmt = fmt.ok_or_else(|| "Missing fmt chunk".to_string())?;
    if fmt.audio_format != 1 {
        return Err(format!(
            "Unsupported audio format ({}). Only PCM (1) is supported.",
            fmt.audio_format
        ));
    }
    if fmt.num_channels != 1 {
        return Err(format!(
            "Unsupported channels ({}). Only mono (1) is supported.",
            fmt.num_channels
        ));
    }
    if fmt.bits_per_sample != 16 {
        return Err(format!(
            "Unsupported bits per sample ({}). Only 16-bit is supported.",
            fmt.bits_per_sample
        ));
    }

    let data_start = data_start.ok_or_else(|| "Missing data chunk".to_string())?;
    let data_size = data_size.ok_or_else(|| "Missing data chunk size".to_string())?;

    if data_size % 2 != 0 {
        return Err("Data chunk size not aligned to 16-bit samples".into());
    }

    let mut samples = Vec::with_capacity(data_size / 2);
    let mut i = data_start;
    while i + 2 <= data_start + data_size {
        let s = i16::from_le_bytes([buf[i], buf[i + 1]]);
        samples.push(s);
        i += 2;
    }

    Ok(WavData { fmt, samples })
}

pub fn write_wav_mono_16le(path: &Path, sample_rate: u32, samples: &[i16]) -> Result<(), String> {
    let mut f = File::create(path).map_err(|e| format!("Failed to create file: {e}"))?;

    let subchunk1_size: u32 = 16; // PCM
    let audio_format: u16 = 1; // PCM
    let num_channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let byte_rate: u32 = sample_rate * num_channels as u32 * (bits_per_sample as u32 / 8);
    let block_align: u16 = num_channels * (bits_per_sample / 8);

    let data_size: u32 = (samples.len() as u32) * 2;
    let riff_chunk_size: u32 = 4 // "WAVE"
        + 8 + subchunk1_size // fmt chunk
        + 8 + data_size; // data chunk

    f.write_all(b"RIFF").map_err(|e| e.to_string())?;
    f.write_all(&riff_chunk_size.to_le_bytes())
        .map_err(|e| e.to_string())?;
    f.write_all(b"WAVE").map_err(|e| e.to_string())?;

    f.write_all(b"fmt ").map_err(|e| e.to_string())?;
    f.write_all(&subchunk1_size.to_le_bytes())
        .map_err(|e| e.to_string())?;
    f.write_all(&audio_format.to_le_bytes())
        .map_err(|e| e.to_string())?;
    f.write_all(&num_channels.to_le_bytes())
        .map_err(|e| e.to_string())?;
    f.write_all(&sample_rate.to_le_bytes())
        .map_err(|e| e.to_string())?;
    f.write_all(&byte_rate.to_le_bytes())
        .map_err(|e| e.to_string())?;
    f.write_all(&block_align.to_le_bytes())
        .map_err(|e| e.to_string())?;
    f.write_all(&bits_per_sample.to_le_bytes())
        .map_err(|e| e.to_string())?;

    f.write_all(b"data").map_err(|e| e.to_string())?;
    f.write_all(&data_size.to_le_bytes())
        .map_err(|e| e.to_string())?;

    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    f.write_all(&bytes).map_err(|e| e.to_string())?;

    Ok(())
}
