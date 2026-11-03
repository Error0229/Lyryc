//! The MMS forced-alignment wav2vec2 (int8 ONNX) on ONNX Runtime, CPU only.

use super::align::{self, AlignError, AlignedLine, Chunking};
use super::romanize;
use ort::execution_providers::CPUExecutionProvider;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// The study's setting: 8 intra-op threads, capped at the physical cores.
pub fn threads() -> usize {
    num_cpus::get_physical().clamp(1, 8)
}

pub struct Model {
    session: Session,
    /// Some exports declare an `attention_mask` input; it is all ones for one unpadded clip.
    wants_mask: bool,
}

impl Model {
    /// `runtime`: the onnxruntime library to load (once per process).
    pub fn load(runtime: &Path, path: &Path, threads: usize) -> Result<Model, String> {
        ort::init_from(runtime.to_string_lossy()).with_name("lyryc-prealign").commit().map_err(|e| e.to_string())?;
        // No CPU arena: measured on a 213 s track, peak RSS 1074 MB instead of
        // 1688 MB (the arena also holds the prepacked weights) for RTF 0.24
        // instead of 0.17. A background job should be the light one.
        let cpu = CPUExecutionProvider::default().with_arena_allocator(false).build();
        let session = Session::builder()
            .and_then(|b| b.with_execution_providers([cpu]))
            .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
            .and_then(|b| b.with_intra_threads(threads))
            .and_then(|b| b.commit_from_file(path))
            .map_err(|e| e.to_string())?;
        let wants_mask = session.inputs.iter().any(|i| i.name == "attention_mask");
        Ok(Model { session, wants_mask })
    }

    /// `[frames, vocab]` logits for one normalized clip.
    fn logits(&mut self, x: &[f32]) -> Result<(usize, usize, Vec<f32>), String> {
        let n = x.len();
        let input = Tensor::from_array(([1usize, n], x.to_vec())).map_err(|e| e.to_string())?;
        let outputs = if self.wants_mask {
            let mask = Tensor::from_array(([1usize, n], vec![1i64; n])).map_err(|e| e.to_string())?;
            self.session.run(ort::inputs!["input_values" => input, "attention_mask" => mask])
        } else {
            self.session.run(ort::inputs!["input_values" => input])
        }
        .map_err(|e| e.to_string())?;
        let (shape, data) = outputs["logits"].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        let (frames, vocab) = match shape[..] {
            [1, t, v] => (t as usize, v as usize),
            _ => return Err(format!("unexpected logits shape {shape:?}")),
        };
        Ok((frames, vocab, data.to_vec()))
    }

    /// Align `lines` to 16 kHz mono `wav`.
    pub fn align(&mut self, wav: &[f32], lines: &[String], cancel: &AtomicBool) -> Result<Vec<AlignedLine>, AlignError> {
        // Readings first: the Japanese dictionary is gone before activations need the memory.
        let plan = romanize::plan(lines);
        let em = align::emissions(wav, Chunking::default(), cancel, |x| self.logits(x))?;
        align::align_lines(&em, lines, &plan, wav.len())
    }
}
