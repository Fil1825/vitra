//! My Wave (stage 10): what each track sounds like, measured once in the background, and
//! the endless queue that picks by mood from those measurements. Pure: FFmpeg's arguments
//! are built here, running it is the app's job.

pub mod features;
pub mod ffmpeg;
pub mod fft;

pub use features::{AnalysisError, Features, SAMPLE_RATE, VERSION, analyze};
