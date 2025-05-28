use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionType { 
    ExtractAudio, 
    CompressVideo, 
    ConvertToMp4 
}

impl Default for FunctionType { 
    fn default() -> Self { FunctionType::ExtractAudio } 
}

impl FunctionType { 
    /// Check if audio options should be shown for this function type
    pub fn show_audio_options(&self) -> bool {
        matches!(self, Self::ExtractAudio | Self::CompressVideo)
    }

    /// Check if video options should be shown for this function type
    pub fn show_video_options(&self) -> bool {
        matches!(self, Self::CompressVideo)
    }

    /// Check if output format selection should be shown
    pub fn show_output_format(&self) -> bool {
        !matches!(self, Self::ExtractAudio)
    }
    
    pub fn description(&self) -> &'static str { 
        match self { 
            Self::ExtractAudio => "Extract audio from video file.", 
            Self::CompressVideo => "Compress video with advanced options.", 
            Self::ConvertToMp4 => "Convert video to MP4/MKV without re-encoding.", 
        } 
    }
}

/// Supported audio formats with their file extensions and FFmpeg codecs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioFormat {
    MP3,
    WAV,
    FLAC,
    AAC,
    OPUS,
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self::MP3
    }
}

impl AudioFormat {
    /// Get the file extension for this audio format
    pub fn ext(&self) -> &'static str {
        match self {
            Self::MP3 => "mp3",
            Self::WAV => "wav",
            Self::FLAC => "flac",
            Self::AAC => "m4a",  // AAC is commonly stored in .m4a containers
            Self::OPUS => "opus",
        }
    }
    
    /// Get the FFmpeg codec name for this audio format
    pub fn codec(&self) -> &'static str {
        match self {
            Self::MP3 => "libmp3lame",
            Self::WAV => "pcm_s16le",  // Uncompressed WAV
            Self::FLAC => "flac",
            Self::AAC => "aac",
            Self::OPUS => "libopus",
        }
    }
    
    /// Get a display name for this audio format
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::MP3 => "MP3",
            Self::WAV => "WAV",
            Self::FLAC => "FLAC",
            Self::AAC => "AAC",
            Self::OPUS => "Opus",
        }
    }
    
    pub fn all() -> [AudioFormat; 5] {
        [
            AudioFormat::MP3,
            AudioFormat::WAV,
            AudioFormat::FLAC,
            AudioFormat::AAC,
            AudioFormat::OPUS,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameRateMode { CFR, VFR }
impl Default for FrameRateMode { fn default() -> Self { FrameRateMode::CFR } }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat { Mp4, Mkv }
impl Default for OutputFormat { fn default() -> Self { OutputFormat::Mp4 } }
impl OutputFormat { 
    pub fn ext(&self) -> &'static str { 
        match self { 
            OutputFormat::Mp4 => "mp4", 
            OutputFormat::Mkv => "mkv" 
        } 
    }
    
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Mp4 => "MP4",
            Self::Mkv => "MKV"
        }
    }
    
    pub fn all() -> [OutputFormat; 2] {
        [OutputFormat::Mp4, OutputFormat::Mkv]
    }
}
