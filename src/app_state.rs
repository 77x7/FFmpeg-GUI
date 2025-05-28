use crate::enums::{AudioFormat, FunctionType, FrameRateMode, OutputFormat};
use parking_lot::RwLock;
use std::path::Path;
use std::sync::{Arc, Mutex};
use crate::ffmpeg_utils;

pub struct MyApp {
    // File paths
    pub input_path: String,
    pub output_path: String,
    
    // Operation settings
    pub selected_function: FunctionType,
    pub output_format: OutputFormat,
    pub audio_format: AudioFormat,
    
    // Video settings
    pub crf: u8,
    pub video_bitrate: u32,
    pub framerate_mode: FrameRateMode,
    pub use_crf: bool,                  // Whether to use CRF or bitrate for video quality
    pub encoding_preset: String,         // FFmpeg preset (ultrafast, medium, veryslow, etc.)
    pub frame_rate: f32,                // Frame rate for CFR mode (frames per second)
    pub original_fps: f32,              // Original video's frame rate
    
    // Audio settings
    pub audio_bitrate: u32,
    pub audio_quality: u8,              // Quality level (0-9 for MP3, 0-10 for OPUS, etc.)
    pub use_audio_quality: bool,        // Whether to use quality or bitrate for audio
    
    // App state
    pub last_command: String,
    pub output_log: Arc<RwLock<String>>,
    pub progress: Arc<RwLock<f32>>,
    pub running: Arc<RwLock<bool>>,
    pub child: Arc<Mutex<Option<std::process::Child>>>,
    pub duration: f32,
    pub auto_scroll: bool,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            input_path: String::new(),
            output_path: String::new(),
            selected_function: FunctionType::ExtractAudio,
            output_format: OutputFormat::Mp4,
            audio_format: AudioFormat::MP3,
            crf: 28,
            video_bitrate: 2000, // 2000 kbps
            framerate_mode: FrameRateMode::CFR,
            use_crf: true,      // Default to CRF mode for video
            encoding_preset: "medium".to_string(), // Default encoding preset
            frame_rate: 30.0,    // Default frame rate (fps)
            original_fps: 30.0,  // Will be updated when probing input file
            audio_bitrate: 192, // 192 kbps
            audio_quality: 4,   // Middle quality for codecs that use it (like OPUS)
            use_audio_quality: true, // Default to VBR for audio
            last_command: String::new(),
            output_log: Arc::new(RwLock::new(String::new())),
            progress: Arc::new(RwLock::new(0.0)),
            running: Arc::new(RwLock::new(false)),
            child: Arc::new(Mutex::new(None)),
            duration: 1.0,
            auto_scroll: true,
        }
    }
}

impl MyApp {
    pub fn default_output(&self) -> String {
        let input = Path::new(&self.input_path);
        if input.file_stem().is_none() { 
            return String::new(); 
        }
        
        let stem = input.file_stem().unwrap().to_string_lossy();
        let dir = input.parent().unwrap_or_else(|| Path::new("."));
        
        let suffix = match self.selected_function {
            FunctionType::ExtractAudio => {
                format!("{}-Audio.{}", stem, self.audio_format.ext())
            },
            FunctionType::CompressVideo => {
                format!("{}-Compressed.{}", stem, self.output_format.ext())
            },
            FunctionType::ConvertToMp4 => {
                format!("{}-Converted.{}", stem, self.output_format.ext())
            },
        };
        
        let output_path = dir.join(suffix);
        ffmpeg_utils::unique_path(output_path).display().to_string()
    }
    
    pub fn update_command(&mut self) {
        // Always update the output path extension based on the selected format
        if !self.output_path.is_empty() {
            let path = Path::new(&self.output_path);
            
            // Get the parent directory and stem
            let dir = path.parent().unwrap_or_else(|| Path::new("."));
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            
            // Create a new path with the correct extension
            let ext = match self.selected_function {
                FunctionType::ExtractAudio => self.audio_format.ext(),
                _ => self.output_format.ext(),
            };
            
            let new_path = dir.join(format!("{}.{}", stem, ext));
            
            // Check if the new path exists and make it unique if needed
            let unique_path = if new_path.exists() {
                ffmpeg_utils::unique_path(new_path)
            } else {
                new_path
            };
            
            self.output_path = unique_path.display().to_string();
        } else {
            self.output_path = self.default_output();
        }
        
        // Update the command
        self.last_command = self.build_command().join(" ");
    }
    
    pub fn build_command(&self) -> Vec<String> {
        let input = self.input_path.clone();
        let output = if self.output_path.is_empty() {
            self.default_output()
        } else {
            self.output_path.clone()
        };
        
        let mut cmd = vec!["-i".to_string(), input];
        
        match self.selected_function {
            FunctionType::ExtractAudio => {
                // Simple, direct approach for all audio formats
                // Select audio stream only (no video)
                cmd.extend([
                    "-vn".to_string(),  // No video
                    "-sn".to_string(), // No subtitles
                    "-map".to_string(), "0:a".to_string(), // Map only audio streams
                ]);
                
                // Add specific settings for each audio format
                match self.audio_format {
                    AudioFormat::MP3 => {
                        cmd.extend([
                            "-c:a".to_string(),
                            "libmp3lame".to_string(),
                        ]);
                        
                        if self.use_audio_quality {
                            // Variable bitrate mode (VBR)
                            cmd.extend([
                                "-q:a".to_string(),
                                self.audio_quality.to_string(),
                            ]);
                        } else {
                            // Constant bitrate mode (CBR)
                            cmd.extend([
                                "-b:a".to_string(),
                                format!("{k}k", k = self.audio_bitrate),
                            ]);
                        }
                    },
                    AudioFormat::OPUS => {
                        cmd.extend([
                            "-c:a".to_string(),
                            "libopus".to_string(),
                            "-b:a".to_string(),
                            format!("{k}k", k = self.audio_bitrate),
                            "-strict".to_string(),
                            "experimental".to_string(),
                        ]);
                    },
                    AudioFormat::AAC => {
                        cmd.extend([
                            "-c:a".to_string(),
                            "aac".to_string(),
                            "-b:a".to_string(),
                            format!("{k}k", k = self.audio_bitrate),
                            "-strict".to_string(),
                            "experimental".to_string(),
                        ]);
                    },
                    AudioFormat::FLAC => {
                        cmd.extend([
                            "-c:a".to_string(),
                            "flac".to_string(),
                            "-compression_level".to_string(),
                            self.audio_quality.to_string(),
                        ]);
                    },
                    AudioFormat::WAV => {
                        cmd.extend([
                            "-c:a".to_string(),
                            "pcm_s16le".to_string(),
                            "-ar".to_string(),  // Sample rate
                            "44100".to_string(), // CD quality
                        ]);
                    }
                }
                
                // Add output file
                cmd.push("-y".to_string()); // Overwrite output file if it exists
                cmd.push(output);
            },
            FunctionType::CompressVideo => {
                // Ensure we map all streams to preserve them
                cmd.extend([
                    "-map".to_string(), "0".to_string(), // Map all streams from input
                ]);
                
                // Video codec
                cmd.extend([
                    "-c:v".to_string(),
                    "libx264".to_string(),
                ]);
                
                // Video quality settings - CRF or bitrate
                if self.use_crf && self.framerate_mode == FrameRateMode::CFR {
                    // Constant Rate Factor mode
                    cmd.extend([
                        "-crf".to_string(),
                        self.crf.to_string(),
                    ]);
                } else {
                    // Bitrate mode
                    cmd.extend([
                        "-b:v".to_string(),
                        format!("{k}k", k = self.video_bitrate),
                    ]);
                }
                
                // Encoding preset
                cmd.extend([
                    "-preset".to_string(),
                    self.encoding_preset.clone(),
                ]);
                
                // Frame rate settings
                if self.framerate_mode == FrameRateMode::CFR {
                    // Set specific frame rate for CFR mode
                    cmd.extend([
                        "-r".to_string(),
                        format!("{:.3}", self.frame_rate),
                    ]);
                } else {
                    // For VFR mode
                    cmd.extend(["-vsync".to_string(), "vfr".to_string()]);
                }
                
                // Audio settings - use the same approach as audio extraction for consistency
                cmd.extend([
                    "-c:a".to_string(),
                ]);
                
                // Audio codec and quality settings based on format
                match self.audio_format {
                    AudioFormat::MP3 => {
                        cmd.push("libmp3lame".to_string());
                        
                        if self.use_audio_quality {
                            // Variable bitrate mode (VBR)
                            cmd.extend([
                                "-q:a".to_string(),
                                self.audio_quality.to_string(),
                            ]);
                        } else {
                            // Constant bitrate mode (CBR)
                            cmd.extend([
                                "-b:a".to_string(),
                                format!("{k}k", k = self.audio_bitrate),
                            ]);
                        }
                    },
                    AudioFormat::OPUS => {
                        cmd.extend([
                            "libopus".to_string(),
                            "-b:a".to_string(),
                            format!("{k}k", k = self.audio_bitrate),
                            "-compression_level".to_string(),
                            self.audio_quality.to_string(),
                            "-strict".to_string(),
                            "experimental".to_string(),
                        ]);
                    },
                    AudioFormat::AAC => {
                        cmd.extend([
                            "aac".to_string(),
                            "-b:a".to_string(),
                            format!("{k}k", k = self.audio_bitrate),
                            "-strict".to_string(),
                            "experimental".to_string(),
                        ]);
                    },
                    AudioFormat::FLAC => {
                        cmd.extend([
                            "flac".to_string(),
                            "-compression_level".to_string(),
                            self.audio_quality.to_string(),
                            "-strict".to_string(),
                            "experimental".to_string(),
                        ]);
                    },
                    AudioFormat::WAV => {
                        cmd.extend([
                            "pcm_s16le".to_string(),
                            "-ar".to_string(),  // Sample rate
                            "44100".to_string(), // CD quality
                        ]);
                    }
                }
                
                // Preserve subtitles if present
                cmd.extend([
                    "-c:s".to_string(),
                    "copy".to_string(),
                ]);
                
                // Add output file
                cmd.push("-y".to_string()); // Overwrite output file if it exists
                cmd.push(output);
            },
            FunctionType::ConvertToMp4 => {
                // Map all streams to preserve them
                cmd.extend([
                    "-map".to_string(), "0".to_string(), // Map all streams from input
                    "-c".to_string(),
                    "copy".to_string(),
                    "-y".to_string(), // Overwrite output file if it exists
                    output,
                ]);
            }
        }
        
        cmd
    }
}
