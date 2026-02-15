use anyhow::{Context, Result};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

use crate::audio_analysis::{self, AudioAnalyzer, AudioFeatures};

pub struct Player {
    _stream: OutputStream,
    _stream_handle: OutputStreamHandle,
    sink: Arc<Sink>,
    pub duration: Option<Duration>,
    analyzer: Option<AudioAnalyzer>,
}

impl Player {
    pub fn new() -> Result<Self> {
        let (stream, stream_handle) =
            OutputStream::try_default().context("Failed to open audio output")?;
        let sink = Sink::try_new(&stream_handle).context("Failed to create audio sink")?;
        let sink = Arc::new(sink);
        info!("audio output initialized");

        Ok(Self {
            _stream: stream,
            _stream_handle: stream_handle,
            sink,
            duration: None,
            analyzer: None,
        })
    }

    fn new_sink(&mut self) -> Result<()> {
        self.stop();
        let sink =
            Sink::try_new(&self._stream_handle).context("Failed to create audio sink")?;
        self.sink = Arc::new(sink);
        Ok(())
    }

    pub fn play_file(&mut self, path: &Path, duration_secs: Option<f64>) -> Result<()> {
        info!(path = %path.display(), "playing file");
        self.new_sink()?;

        let file = BufReader::new(File::open(path).context("Failed to open audio file")?);
        let source = Decoder::new(file).context("Failed to decode audio file")?;

        let channels = source.channels();
        let sample_rate = source.sample_rate();

        // Create shared buffer and wrap source with AnalyzingSource
        let buffer = audio_analysis::new_shared_buffer();
        let analyzing_source =
            audio_analysis::AnalyzingSource::new(source.convert_samples::<f32>(), buffer.clone(), channels, sample_rate);

        self.analyzer = Some(AudioAnalyzer::new(buffer, sample_rate));
        self.sink.append(analyzing_source);
        self.duration = duration_secs.map(|s| Duration::from_secs_f64(s));

        Ok(())
    }

    pub fn get_audio_features(&mut self) -> AudioFeatures {
        match self.analyzer {
            Some(ref mut a) => a.analyze(),
            None => AudioFeatures::default(),
        }
    }

    pub fn pause(&self) {
        self.sink.pause();
    }

    pub fn resume(&self) {
        self.sink.play();
    }

    pub fn is_paused(&self) -> bool {
        self.sink.is_paused()
    }

    pub fn set_volume(&self, volume: u8) {
        self.sink.set_volume(volume as f32 / 100.0);
    }

    pub fn is_empty(&self) -> bool {
        self.sink.empty()
    }

    pub fn stop(&mut self) {
        self.sink.stop();
    }

    pub fn get_position(&self) -> Duration {
        self.sink.get_pos()
    }

    pub fn seek(&self, position: Duration) {
        if let Err(e) = self.sink.try_seek(position) {
            warn!(?e, ?position, "seek failed");
        }
    }
}
