//! echora-kara-300m (int8 ONNX) on ONNX Runtime, CPU only: the wav2vec2
//! encoder and the two halves of the text head. Loaded and dropped together.

use super::align::{self, AlignError, AlignedLine, Chunking, Lattice};
use super::assets::Paths;
use super::decode::VOCAB;
use super::head::{self, Packed, Text, HIDDEN};
use super::romanize;
use ort::execution_providers::CPUExecutionProvider;
use ort::session::{builder::GraphOptimizationLevel, Session, SessionOutputs};
use ort::tensor::PrimitiveTensorElementType;
use ort::value::TensorRef;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// The study's setting: 8 intra-op threads, capped at the physical cores.
pub fn threads() -> usize {
    num_cpus::get_physical().clamp(1, 8)
}

pub struct Model {
    encoder: Session,
    text: Session,
    frames: Session,
}

/// No CPU arena: for MMS on a 213 s track, peak RSS 1074 MB instead of 1688 MB
/// (the arena also holds the prepacked weights) for RTF 0.24 instead of 0.17.
/// A background job should be the light one.
fn session(path: &Path, threads: usize) -> Result<Session, String> {
    let cpu = CPUExecutionProvider::default().with_arena_allocator(false).build();
    Session::builder()
        .and_then(|b| b.with_execution_providers([cpu]))
        .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
        .and_then(|b| b.with_intra_threads(threads))
        .and_then(|b| b.commit_from_file(path))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// An input tensor over `data`, without copying it.
fn view<T, const N: usize>(shape: [usize; N], data: &[T]) -> Result<TensorRef<'_, T>, String>
where
    T: PrimitiveTensorElementType + std::fmt::Debug + Clone + 'static,
{
    TensorRef::from_array_view((shape, data)).map_err(|e| e.to_string())
}

/// An f32 output of rank `rank`, with its shape.
fn output(out: &SessionOutputs, name: &str, rank: usize) -> Result<(Vec<usize>, Vec<f32>), String> {
    let (shape, data) = out[name].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
    if shape.len() != rank {
        return Err(format!("unexpected {name} shape {shape:?}"));
    }
    Ok((shape.iter().map(|&d| d as usize).collect(), data.to_vec()))
}

impl Model {
    /// Load the runtime library (once per process) and the three sessions.
    pub fn load(paths: &Paths, threads: usize) -> Result<Model, String> {
        ort::init_from(paths.runtime().to_string_lossy()).with_name("lyryc-prealign").commit().map_err(|e| e.to_string())?;
        let [encoder, text, frames] = paths.models();
        Ok(Model { encoder: session(&encoder, threads)?, text: session(&text, threads)?, frames: session(&frames, threads)? })
    }

    /// `(frames, logits [T, 30], hidden [T, 1024])` for one normalized clip.
    fn encode(&mut self, x: &[f32]) -> Result<(usize, Vec<f32>, Vec<f32>), String> {
        let out = self.encoder.run(ort::inputs!["input_values" => view([1, x.len()], x)?]).map_err(|e| e.to_string())?;
        let (shape, logits) = output(&out, "logits", 3)?;
        let (_, hidden) = output(&out, "hidden", 3)?;
        if shape[2] != VOCAB || hidden.len() != shape[1] * HIDDEN {
            return Err(format!("unexpected encoder output {shape:?}"));
        }
        Ok((shape[1], logits, hidden))
    }

    /// The text side of the head, once per song.
    fn text(&mut self, p: &Packed) -> Result<Text, String> {
        let k = p.line.len();
        let out = self
            .text
            .run(ort::inputs![
                "letters" => view([k, 16], &p.letters)?,
                "line" => view([k], &p.line)?,
                "pos" => view([k], &p.pos)?,
            ])
            .map_err(|e| e.to_string())?;
        Ok(Text { text: output(&out, "text", 2)?.1, kappa: output(&out, "kappa", 2)?.1, m: output(&out, "m", 2)?.1 })
    }

    /// The frame side on one window: `[W, K]` log P(token | singing).
    fn share(&mut self, hidden: &[f32], logp: &[f32], t: &Text) -> Result<Vec<f32>, String> {
        let (w, k) = (logp.len() / VOCAB, t.m.len() / VOCAB);
        let out = self
            .frames
            .run(ort::inputs![
                "hidden" => view([w, HIDDEN], hidden)?,
                "logp" => view([w, VOCAB], logp)?,
                "text" => view([k, 512], &t.text)?,
                "kappa" => view([k, 512], &t.kappa)?,
                "m" => view([k, VOCAB], &t.m)?,
            ])
            .map_err(|e| e.to_string())?;
        Ok(output(&out, "share", 2)?.1)
    }

    /// The track's lattice (see `align::lattice`).
    pub fn lattice(&mut self, wav: &[f32], cancel: &AtomicBool) -> Result<Lattice, AlignError> {
        align::lattice(wav, Chunking::default(), cancel, |x| self.encode(x))
    }

    /// Align `lines` to 16 kHz mono `wav`.
    pub fn align(&mut self, wav: &[f32], lines: &[String], cancel: &AtomicBool) -> Result<Vec<AlignedLine>, AlignError> {
        // Readings first: the Japanese dictionary is gone before activations need the memory.
        let plan = romanize::plan(lines);
        let labels = align::labels(&plan);
        if labels.tokens.ids.is_empty() {
            return Err(AlignError::NoText);
        }
        let mut lat = self.lattice(wav, cancel)?;
        let text = self.text(&head::pack(&labels.tokens.ids, &labels.line_of)).map_err(AlignError::Model)?;
        let share = head::text_share(&lat.hidden, &lat.logp, labels.tokens.ids.len(), |h, lp| {
            if cancel.load(Ordering::Relaxed) {
                return Err(AlignError::Cancelled);
            }
            self.share(h, lp, &text).map_err(AlignError::Model)
        })?;
        lat.hidden = Vec::new(); // the decoder needs only the log-probabilities
        align::align_lines(&lat, &share, wav, lines, &plan, &labels)
    }
}
