//! An m4a (AAC in MP4, YouTube format 140) file → 16 kHz mono f32, without ffmpeg.

use super::align::SAMPLE_RATE;
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Decode the first audio track, downmix to mono and resample to 16 kHz.
pub fn decode_16k(path: &Path) -> Result<Vec<f32>, String> {
    let (mono, rate) = decode_mono(path)?;
    Ok(resample(&mono, rate, SAMPLE_RATE as u32))
}

fn decode_mono(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("probe: {e}"))?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("no audio track")?;
    let track_id = track.id;
    let mut rate = track.codec_params.sample_rate.unwrap_or(0);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("codec: {e}"))?;

    let mut mono = Vec::new();
    let mut buf: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(format!("read: {e}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(_)) => continue, // one bad frame is not the whole song
            Err(e) => return Err(format!("decode: {e}")),
        };
        let spec = *decoded.spec();
        rate = spec.rate;
        let channels = spec.channels.count().max(1);
        let b = buf.get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
        if b.capacity() < decoded.capacity() * channels {
            *b = SampleBuffer::new(decoded.capacity() as u64, spec);
        }
        b.copy_interleaved_ref(decoded);
        mono.extend(b.samples().chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32));
    }
    if mono.is_empty() || rate == 0 {
        return Err("no audio decoded".into());
    }
    Ok((mono, rate))
}

/// Windowed-sinc resampling (Blackman, 32 zero crossings each side), cut off
/// a little below the new Nyquist. Polyphase: with `to/from = L/M` in lowest
/// terms there are only L distinct filters (160 for 44.1 → 16 kHz).
pub fn resample(x: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || x.is_empty() {
        return x.to_vec();
    }
    let g = gcd(from, to);
    let (l, m) = ((to / g) as usize, (from / g) as usize);
    let ratio = to as f64 / from as f64;
    let cutoff = 0.95 * ratio.min(1.0); // cycles per input sample, ×2
    let width = (32.0 / cutoff).ceil() as i64; // input samples each side

    // filters[p][j]: weight of input sample (centre - width + 1 + j) at phase p / L.
    let taps = 2 * width as usize;
    let filters: Vec<Vec<f32>> = (0..l.min(8192))
        .map(|p| {
            let frac = p as f64 / l as f64;
            let h: Vec<f64> = (0..taps).map(|j| kernel(frac + width as f64 - 1.0 - j as f64, cutoff, width)).collect();
            let norm: f64 = h.iter().sum();
            h.iter().map(|v| (v / norm) as f32).collect()
        })
        .collect();

    let n_out = x.len() * l / m;
    let mut out = Vec::with_capacity(n_out);
    for n in 0..n_out {
        let num = n * m;
        let (centre, phase) = ((num / l) as i64, num % l);
        let h = &filters[phase * filters.len() / l];
        let first = centre - width + 1;
        let mut acc = 0.0f32;
        if first >= 0 && first + taps as i64 <= x.len() as i64 {
            let xs = &x[first as usize..first as usize + taps];
            acc = xs.iter().zip(h).map(|(a, b)| a * b).sum();
        } else {
            for (j, w) in h.iter().enumerate() {
                let k = first + j as i64;
                if k >= 0 && (k as usize) < x.len() {
                    acc += x[k as usize] * w;
                }
            }
        }
        out.push(acc);
    }
    out
}

/// Blackman-windowed sinc at distance `d` input samples.
fn kernel(d: f64, cutoff: f64, width: i64) -> f64 {
    use std::f64::consts::PI;
    let arg = d * cutoff;
    let sinc = if arg.abs() < 1e-9 { 1.0 } else { (PI * arg).sin() / (PI * arg) };
    let w = (d / width as f64).clamp(-1.0, 1.0);
    let blackman = 0.42 + 0.5 * (PI * w).cos() + 0.08 * (2.0 * PI * w).cos();
    sinc * blackman.max(0.0)
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f64, rate: u32, secs: f64) -> Vec<f32> {
        (0..(rate as f64 * secs) as usize).map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / rate as f64).sin() as f32).collect()
    }

    fn rms(x: &[f32]) -> f64 {
        (x.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
    }

    #[test]
    fn keeps_the_passband_and_kills_aliases() {
        let out = resample(&tone(440.0, 44_100, 1.0), 44_100, 16_000);
        assert_eq!(out.len(), 16_000);
        assert!((rms(&out[1000..15_000]) - std::f64::consts::FRAC_1_SQRT_2).abs() < 0.01, "{}", rms(&out));
        // 12 kHz is above the new 8 kHz Nyquist: it must not fold back in.
        let out = resample(&tone(12_000.0, 48_000, 1.0), 48_000, 16_000);
        assert!(rms(&out[1000..15_000]) < 0.01, "{}", rms(&out));
    }
}
