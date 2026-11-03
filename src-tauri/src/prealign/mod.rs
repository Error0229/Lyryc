//! Pre-alignment: opt-in, experimental word-level timing for YouTube Music.
//! See docs/research/prealign/REPORT.md for the study.
#![allow(dead_code)]

pub mod align;
mod audio;
mod ctc;
mod model;
mod reject;
mod romanize;
