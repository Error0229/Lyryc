//! LLM Provider implementations for track cleaning

mod gemini;
mod github;

pub use gemini::{
    clean_artist_and_track_names_gemini, clean_track_name_gemini, clean_track_name_gemini_with_key,
};
pub use github::{
    clean_artist_and_track_names_github, clean_track_name_github, clean_track_name_github_with_key,
};
