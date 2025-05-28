mod enums;
mod ffmpeg_utils;
mod app_state;

use eframe::egui::{self, ScrollArea, Slider};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use app_state::MyApp;
use enums::{AudioFormat, FunctionType, FrameRateMode, OutputFormat};
use ffmpeg_utils::parse_timecode;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "FFmpeg GUI",
        options,
        Box::new(|_cc| {
            let mut app = MyApp::default();
            app.update_command();
            Box::new(app)
        }),
    )
}

impl MyApp {
    // The build_command method has been moved to app_state.rs

    // Using the probe_duration implementation at the end of the file

    fn run(&mut self, ctx: egui::Context) {
        // Check if a process is already running
        if *self.running.read() {
            self.output_log.write().push_str("A process is already running. Please stop it first.\n");
            return;
        }
        
        // Validate input file
        if self.input_path.is_empty() {
            self.output_log.write().push_str("Error: No input file selected.\n");
            return;
        }
        
        if !Path::new(&self.input_path).exists() {
            self.output_log.write().push_str(&format!("Error: Input file does not exist: {}\n", self.input_path));
            return;
        }
        
        // Validate and ensure unique output path
        if self.output_path.is_empty() {
            self.output_path = self.default_output();
        }
        
        // Check if output path exists and make it unique if needed
        let output_path = Path::new(&self.output_path);
        if output_path.exists() {
            // Generate a unique path
            let unique_path = ffmpeg_utils::unique_path(output_path.to_path_buf());
            self.output_path = unique_path.display().to_string();
        }
        
        // Validate output directory exists and is writable
        if let Some(parent) = Path::new(&self.output_path).parent() {
            if !parent.exists() {
                self.output_log.write().push_str(&format!("Error: Output directory does not exist: {}\n", parent.display()));
                *self.running.write() = false;
                return;
            }
        }
        
        // Mark process as running and reset progress
        *self.running.write() = true;
        *self.progress.write() = 0.0;
        self.output_log.write().clear();

        // Get the validated output path
        let final_output_path = PathBuf::from(self.output_path.clone());

        // Log output destination
        self.output_log.write().push_str(&format!("Outputting to: {}\n", final_output_path.display()));

        // Build the FFmpeg command
        let cmd_args = self.build_command();
        self.update_command();

        // Clone necessary state for the background thread
        let log = self.output_log.clone();
        let progress_arc = self.progress.clone();
        let running_arc = self.running.clone();
        let child_arc = self.child.clone();
        let duration = self.duration;
        let ctx = Arc::new(ctx);
        
        // Make sure child process reference is cleared before starting a new one
        if let Ok(mut child_guard) = self.child.lock() {
            *child_guard = None;
        }

        // Spawn a background thread to run FFmpeg
        std::thread::spawn(move || {
            log.write().push_str(&format!("Executing: ffmpeg {}\n", cmd_args.join(" ")));
            
            // Create and spawn the FFmpeg process
            let mut child = Command::new("ffmpeg")
                .args(&cmd_args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Failed to spawn ffmpeg process");

            // Capture stderr for progress monitoring
            if let Some(stderr) = child.stderr.take() {
                // Store the child process for potential cancellation
                *child_arc.lock().unwrap() = Some(child);
                
                // Create a new thread to process stderr output
                let ctx_clone = ctx.clone();
                let log_clone = log.clone();
                let progress_clone = progress_arc.clone();
                let duration_clone = duration;
                
                std::thread::spawn(move || {
                    let reader = BufReader::new(stderr);
                    for line in reader.lines() {
                        if let Ok(line_content) = line {
                            // Add line to log with newline
                            log_clone.write().push_str(&format!("{line_content}\n"));
                            
                            // Parse progress information
                            if line_content.contains("time=") {
                                if let Some(start) = line_content.find("time=") {
                                    let time_str = line_content[start + 5..]
                                        .split_whitespace()
                                        .next()
                                        .unwrap_or("00:00:00.00");
                                    let current_time = parse_timecode(time_str);
                                    let progress = (current_time / duration_clone).clamp(0.0, 1.0);
                                    
                                    // Update progress and log it for debugging
                                    *progress_clone.write() = progress;
                                    
                                    // Force UI update
                                    ctx_clone.request_repaint();
                                }
                            }
                        }
                    }
                });
                
                // Wait for the process to complete
                if let Ok(mut guard) = child_arc.lock() {
                    if let Some(ref mut child_process) = *guard {
                        if let Ok(status) = child_process.wait() {
                            log.write().push_str(&format!("FFmpeg finished with status: {}\n", status));
                            if status.success() {
                                log.write().push_str(&format!("Output successfully saved to {}\n", final_output_path.display()));
                            } else {
                                log.write().push_str("FFmpeg command failed.\n");
                            }
                        }
                    }
                    *guard = None; // Clear child process reference
                }
            } else {
                log.write().push_str("Failed to capture FFmpeg output.\n");
            }
            
            // Mark process as complete
            *running_arc.write() = false;
            *progress_arc.write() = 1.0; // Set progress to 100%
            ctx.request_repaint(); // Update the UI
        });
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.duration == 1.0 && Path::new(&self.input_path).exists() {
            self.probe_duration();
        }

        let _running = *self.running.read();
        let _progress = *self.progress.read();

        // Main layout with a top panel for controls and a central panel for content
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("FFmpeg GUI");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let running = *self.running.read();
                    if ui.add_enabled(!running, egui::Button::new("Start").min_size(egui::vec2(80.0, 0.0))).clicked() {
                        self.run(ctx.clone());
                    }
                    if ui.add_enabled(running, egui::Button::new("Stop").min_size(egui::vec2(80.0, 0.0))).clicked() {
                        self.stop_ffmpeg();
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {

            // Input file selection
            ui.horizontal(|ui| {
                ui.label("Input file:");
                ui.text_edit_singleline(&mut self.input_path);
                if ui.button("Browse").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Media files", &["mp4", "mkv", "mov", "avi", "mp3", "wav"])
                        .pick_file()
                    {
                        self.input_path = path.display().to_string();
                        self.output_path = self.default_output();
                        self.update_command();
                    }
                }
            });

            // Output file selection
            ui.horizontal(|ui| {
                ui.label("Output file:");
                ui.text_edit_singleline(&mut self.output_path);
                if ui.button("Browse").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Output file", &[self.output_format.ext()])
                        .set_file_name(&self.output_path)
                        .save_file()
                    {
                        self.output_path = path.display().to_string();
                        self.update_command();
                    }
                }
            });

            // Function selection
            ui.horizontal(|ui| {
                ui.label("Function:");
                for &func in &[FunctionType::ExtractAudio, FunctionType::CompressVideo, FunctionType::ConvertToMp4] {
                    if ui.radio_value(&mut self.selected_function, func, format!("{:?}", func)).clicked() {
                        self.output_path = self.default_output();
                        self.update_command();
                    }
                }
            });

            // Show function description
            ui.label(self.selected_function.description());

            // Show options based on selected function
            if self.selected_function.show_audio_options() {
                ui.collapsing("Audio Options", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Audio Format:");
                        egui::ComboBox::from_id_source("audio_format")
                            .selected_text(self.audio_format.display_name())
                            .show_ui(ui, |ui| {
                                for format in AudioFormat::all() {
                                    ui.selectable_value(
                                        &mut self.audio_format, 
                                        format, 
                                        format.display_name()
                                    );
                                }
                            });
                    });

                    // Show different options based on audio format
                    match self.audio_format {
                        AudioFormat::MP3 => {
                            // MP3 can use either bitrate or quality level (VBR), but not both
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut self.use_audio_quality, true, "Variable Bitrate (VBR)");
                                ui.radio_value(&mut self.use_audio_quality, false, "Constant Bitrate (CBR)");
                            });

                            if self.use_audio_quality {
                                ui.horizontal(|ui| {
                                    ui.label("MP3 Quality:");
                                    // Invert the quality for display (0=best to 9=worst becomes 9=best to 0=worst)
                                    let mut inverted_quality = 9 - self.audio_quality;
                                    if ui.add(egui::DragValue::new(&mut inverted_quality)
                                        .clamp_range(0..=9))
                                        .on_hover_text("0=worst, 9=best quality")
                                        .changed() 
                                    {
                                        self.audio_quality = 9 - inverted_quality; // Convert back
                                        self.update_command();
                                    }
                                });
                            } else {
                                ui.horizontal(|ui| {
                                    ui.label("Bitrate:");
                                    if ui.add(egui::DragValue::new(&mut self.audio_bitrate)
                                        .clamp_range(8..=320)
                                        .suffix(" kbps"))
                                        .changed() {
                                        self.update_command();
                                    }
                                });
                            }
                        },
                        AudioFormat::OPUS => {
                            ui.horizontal(|ui| {
                                ui.label("Opus Bitrate:");
                                
                                // Common Opus bitrate standards for reference
                                let bitrate_standards = [16, 24, 32, 40, 48, 64, 96, 128, 160, 192, 256];
                                
                                // Allow direct input with custom values
                                if ui.add(egui::DragValue::new(&mut self.audio_bitrate)
                                    .clamp_range(8..=512)
                                    .suffix(" kbps"))
                                    .on_hover_text(format!("Common values: 16, 24, 32, 48, 64, 96, 128, 192, 256 kbps"))
                                    .changed() 
                                {
                                    self.update_command();
                                }
                                
                                // Add buttons for common bitrates
                                ui.horizontal(|ui| {
                                    for &standard in &[32, 64, 128, 192, 256] {
                                        if ui.small_button(format!("{}", standard)).clicked() {
                                            self.audio_bitrate = standard;
                                            self.update_command();
                                        }
                                    }
                                });
                            });
                            ui.horizontal(|ui| {
                                ui.label("Compression:");
                                if ui.add(egui::DragValue::new(&mut self.audio_quality)
                                    .clamp_range(0..=10))
                                    .on_hover_text("0=fastest, 10=smallest file")
                                    .changed() 
                                {
                                    self.update_command();
                                }
                            });
                        },
                        AudioFormat::AAC => {
                            ui.horizontal(|ui| {
                                ui.label("AAC Bitrate:");
                                
                                // Common audio bitrate standards for reference
                                let bitrate_standards = [32, 64, 96, 128, 160, 192, 224, 256, 320];
                                
                                // Allow direct input with custom values
                                if ui.add(egui::DragValue::new(&mut self.audio_bitrate)
                                    .clamp_range(8..=512)
                                    .suffix(" kbps"))
                                    .on_hover_text(format!("Common values: 32, 64, 96, 128, 160, 192, 224, 256, 320 kbps"))
                                    .changed() 
                                {
                                    self.update_command();
                                }
                                
                                // Add buttons for common bitrates
                                ui.horizontal(|ui| {
                                    for &standard in &[64, 128, 192, 256, 320] {
                                        if ui.small_button(format!("{}", standard)).clicked() {
                                            self.audio_bitrate = standard;
                                            self.update_command();
                                        }
                                    }
                                });
                            });
                        },
                        AudioFormat::FLAC => {
                            ui.horizontal(|ui| {
                                ui.label("Compression:");
                                if ui.add(egui::DragValue::new(&mut self.audio_quality)
                                    .clamp_range(0..=12))
                                    .on_hover_text("0=fastest, 12=smallest file")
                                    .changed() 
                                {
                                    self.update_command();
                                }
                            });
                        },
                        AudioFormat::WAV => {
                            ui.label("WAV uses uncompressed PCM audio (no quality settings)");
                        }
                    }
                });
            }

            if self.selected_function.show_video_options() {
                ui.collapsing("Video Options", |ui| {
                    // Frame rate mode selection
                    ui.horizontal(|ui| {
                        ui.label("Frame Rate Mode:");
                        if ui.radio_value(&mut self.framerate_mode, FrameRateMode::CFR, "Constant Frame Rate (CFR)").clicked() {
                            self.update_command();
                        }
                        if ui.radio_value(&mut self.framerate_mode, FrameRateMode::VFR, "Variable Frame Rate (VFR)").clicked() {
                            // When switching to VFR, ensure we're using bitrate mode
                            self.use_crf = false;
                            self.update_command();
                        }
                    });
                    
                    // Quality control method
                    ui.horizontal(|ui| {
                        ui.label("Quality Control Method:");
                        let crf_enabled = self.framerate_mode == FrameRateMode::CFR;
                        ui.add_enabled(crf_enabled, egui::RadioButton::new(self.use_crf, "Constant Rate Factor (CRF)"))
                            .on_hover_text(if !crf_enabled { "CRF is only available with Constant Frame Rate" } else { "Quality-based encoding" })
                            .clicked().then(|| {
                                self.use_crf = true;
                                self.update_command();
                            });
                        ui.radio_value(&mut self.use_crf, false, "Bitrate").clicked().then(|| {
                            self.update_command();
                        });
                    });

                    // Show appropriate quality control based on selection
                    if self.use_crf && self.framerate_mode == FrameRateMode::CFR {
                        ui.horizontal(|ui| {
                            ui.label("Quality:");
                            // Common CRF values
                            let crf_standards = [17, 18, 20, 23, 28, 30, 35, 40];
                            
                            // Allow direct input of CRF value
                            if ui.add(egui::DragValue::new(&mut self.crf)
                                .speed(1.0)
                                .clamp_range(0..=51)
                                .prefix("CRF "))
                                .on_hover_text("Lower value = better quality (17-18=visually lossless, 23=default, 28=good compression)")
                                .changed() 
                            {
                                self.update_command();
                            }
                            
                            // Add buttons for common CRF values
                            ui.horizontal(|ui| {
                                for &standard in &[18, 23, 28, 35] {
                                    if ui.small_button(format!("{}", standard)).clicked() {
                                        self.crf = standard;
                                        self.update_command();
                                    }
                                }
                            });
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("Bitrate:");
                            
                            // Common video bitrate standards
                            let bitrate_standards = [500, 750, 1000, 1500, 2000, 2500, 3000, 4000, 5000, 6000, 8000, 10000, 15000, 20000];
                            
                            // Allow direct input with high upper limit
                            if ui.add(egui::DragValue::new(&mut self.video_bitrate)
                                .speed(100.0)
                                .clamp_range(100..=50000)
                                .suffix(" kbps"))
                                .on_hover_text("Enter any value between 100-50000 kbps")
                                .changed() 
                            {
                                self.update_command();
                            }
                            
                            // Add buttons for common video bitrates
                            ui.horizontal(|ui| {
                                for &standard in &[1000, 2500, 5000, 8000, 15000] {
                                    if ui.small_button(format!("{}", standard)).clicked() {
                                        self.video_bitrate = standard;
                                        self.update_command();
                                    }
                                }
                            });
                        });
                    }
                    
                    // Add frame rate slider for CFR mode
                    if self.framerate_mode == FrameRateMode::CFR {
                        ui.horizontal(|ui| {
                            ui.label("Frame Rate:");
                            let fps_max = self.original_fps.max(60.0); // Use original FPS or 60 as max
                            
                            // Common FPS values to snap to (filtered to not exceed max)
                            let common_fps: Vec<f32> = [23.976, 24.0, 25.0, 29.97, 30.0, 50.0, 60.0]
                                .iter()
                                .filter(|&&fps| fps <= fps_max)
                                .cloned()
                                .collect();
                            
                            // Add original FPS if not already in the list
                            let mut all_fps = common_fps.clone();
                            if !all_fps.contains(&self.original_fps) {
                                all_fps.push(self.original_fps);
                                all_fps.sort_by(|a, b| a.partial_cmp(b).unwrap());
                            }
                            
                            // Allow direct input of frame rate with upper limit based on original fps
                            if ui.add(egui::DragValue::new(&mut self.frame_rate)
                                .speed(0.1)
                                .clamp_range(1.0..=fps_max)
                                .suffix(" fps")
                                .fixed_decimals(3))
                                .on_hover_text(format!("Original: {:.3} fps. Enter any value up to {:.3}", self.original_fps, fps_max))
                                .changed() 
                            {
                                self.update_command();
                            }
                            
                            // Add buttons for common frame rates and original
                            ui.horizontal(|ui| {
                                // Only show fps values that don't exceed the max
                                let common_fps = [23.976, 24.0, 25.0, 29.97, 30.0, 50.0, 60.0];
                                for &fps in common_fps.iter().filter(|&&fps| fps <= fps_max) {
                                    if ui.small_button(format!("{:.3}", fps)).clicked() {
                                        self.frame_rate = fps;
                                        self.update_command();
                                    }
                                }
                                
                                // Add original fps button if not already in common values
                                if !common_fps.contains(&self.original_fps) && ui.small_button("Original").clicked() {
                                    self.frame_rate = self.original_fps;
                                    self.update_command();
                                }
                            });
                        });
                    }
                    
                    // Preset selection
                    ui.horizontal(|ui| {
                        ui.label("Encoding Preset:");
                        egui::ComboBox::from_id_source("encoding_preset")
                            .selected_text(&self.encoding_preset)
                            .show_ui(ui, |ui| {
                                for preset in &["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"] {
                                    ui.selectable_value(&mut self.encoding_preset, preset.to_string(), *preset);
                                }
                            });
                    });
                });
            }

            if self.selected_function.show_output_format() {
                ui.horizontal(|ui| {
                    ui.label("Output Format:");
                    for format in OutputFormat::all() {
                        if ui.radio_value(&mut self.output_format, format, format.display_name()).clicked() {
                            self.output_path = self.default_output();
                            self.update_command();
                        }
                    }
                });
            }

            // Progress bar with proper scaling
            let progress = *self.progress.read();
            let running = *self.running.read();
            
            // Only animate the progress bar when a process is running
            ui.add(
                egui::ProgressBar::new(progress)
                    .show_percentage()
                    .animate(running) // Only animate when actually running
            );
            
            // Only show percentage text when running
            if running || progress > 0.0 {
                ui.label(format!("Progress: {:.1}%", progress * 100.0));
            }

            // Command preview
            ui.collapsing("FFmpeg Command", |ui| {
                ui.monospace(&self.last_command);
            });

            // Log output with better spacing
            ui.vertical_centered_justified(|ui| {
                ui.add_space(10.0);
                ui.heading("Output Log");
                
                // Get the log output as a string
                let output = self.output_log.read().clone();
                
                ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .stick_to_bottom(self.auto_scroll)
                    .show(ui, |ui| {
                        // Create the text edit widget inside the scroll area
                        ui.add_sized(
                            ui.available_size(),
                            egui::TextEdit::multiline(&mut output.as_str())
                                .font(egui::TextStyle::Monospace)
                                .desired_width(ui.available_width())
                                .frame(true),  // Enable the frame
                        );
                    });
            });

            // Bottom panel for log controls
            ui.vertical_centered_justified(|ui| {
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(5.0);
                
                ui.horizontal(|ui| {
                    if ui.button("📋 Copy Command").clicked() {
                        if let Err(e) = arboard::Clipboard::new().and_then(|mut c| c.set_text(self.last_command.clone())) {
                            eprintln!("Failed to copy to clipboard: {}", e);
                        }
                    }
                    
                    if ui.button("🗑️ Clear Log").clicked() {
                        *self.output_log.write() = String::new();
                    }
                    
                    ui.checkbox(&mut self.auto_scroll, "Auto-scroll");
                });
            });
        });

        // Request repaint to keep the UI responsive
        ctx.request_repaint();
    }
}

impl MyApp {
    fn stop_ffmpeg(&mut self) {
        // Log that we're stopping the process
        self.output_log.write().push_str("\nStopping FFmpeg process...\n");
        
        // First set running to false to prevent UI updates
        *self.running.write() = false;
        
        // Create a separate thread to kill the process to avoid UI hanging
        let child_arc = self.child.clone();
        let log_arc = self.output_log.clone();
        let progress_arc = self.progress.clone();
        
        std::thread::spawn(move || {
            // Try to kill the process
            let killed = if let Ok(mut child_guard) = child_arc.lock() {
                if let Some(child) = child_guard.as_mut() {
                    // Try to kill the process
                    match child.kill() {
                        Ok(_) => {
                            log_arc.write().push_str("Process terminated.\n");
                            // Wait for the process to fully exit
                            let _ = child.wait();
                            true
                        },
                        Err(e) => {
                            log_arc.write().push_str(&format!("Error killing process: {}\n", e));
                            false
                        }
                    }
                } else {
                    false
                }
            } else {
                false
            };
            
            // Clear the child process reference regardless of kill result
            if let Ok(mut child_guard) = child_arc.lock() {
                *child_guard = None;
            }
            
            // Set progress to 0 to reset the UI
            *progress_arc.write() = 0.0;
            
            if !killed {
                log_arc.write().push_str("No active process to stop.\n");
            }
        });
    }

    // update_command is now in app_state.rs
    
    fn probe_duration(&mut self) {
        if !Path::new(&self.input_path).exists() {
            self.duration = 1.0;
            self.original_fps = 30.0; // Default FPS
            return;
        }
        
        // Log that we're probing the file
        self.output_log.write().push_str("Probing file information...\n");
        
        // First, get the duration
        let duration_output = Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1", &self.input_path])
            .output();
            
        if let Ok(output) = duration_output {
            if let Ok(duration_str) = String::from_utf8(output.stdout) {
                if let Ok(duration) = duration_str.trim().parse::<f32>() {
                    self.duration = duration.max(1.0); // Ensure duration is at least 1.0
                    self.output_log.write().push_str(&format!("File duration: {:.2} seconds\n", self.duration));
                }
            }
        } else {
            // Default duration if probing fails
            self.duration = 1.0;
            self.output_log.write().push_str("Could not determine file duration, using default.\n");
        }
        
        // Now, get the frame rate
        let fps_output = Command::new("ffprobe")
            .args([
                "-v", "error", 
                "-select_streams", "v:0", 
                "-show_entries", "stream=r_frame_rate", 
                "-of", "default=noprint_wrappers=1:nokey=1", 
                &self.input_path
            ])
            .output();
            
        if let Ok(output) = fps_output {
            if let Ok(fps_str) = String::from_utf8(output.stdout) {
                let fps_str = fps_str.trim();
                if fps_str.contains('/') {
                    // Handle fractional frame rates like "30000/1001"
                    let parts: Vec<&str> = fps_str.split('/').collect();
                    if parts.len() == 2 {
                        if let (Ok(num), Ok(den)) = (parts[0].parse::<f32>(), parts[1].parse::<f32>()) {
                            if den > 0.0 {
                                let fps = num / den;
                                self.original_fps = fps;
                                self.frame_rate = fps.min(60.0); // Cap initial frame rate at 60 fps
                                self.output_log.write().push_str(&format!("Original frame rate: {:.3} fps\n", fps));
                                return;
                            }
                        }
                    }
                } else if let Ok(fps) = fps_str.parse::<f32>() {
                    // Handle direct frame rate values
                    self.original_fps = fps;
                    self.frame_rate = fps.min(60.0); // Cap initial frame rate at 60 fps
                    self.output_log.write().push_str(&format!("Original frame rate: {:.3} fps\n", fps));
                    return;
                }
            }
        }
        
        // Default frame rate if probing fails
        self.original_fps = 30.0;
        self.frame_rate = 30.0;
        self.output_log.write().push_str("Could not determine original frame rate, using 30 fps.\n");
    }
}
