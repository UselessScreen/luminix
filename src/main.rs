// #![cfg_attr(
//     all(
//         target_os = "windows",
//         not(debug_assertions),
//     ),
//     windows_subsystem = "windows"
// )]
mod settings_window;
mod register_file_association;
mod errors;
mod wgpu_renderer;

use image::{AnimationDecoder, Delay, ImageFormat};
use std::env;
use std::io::BufReader;
use std::sync::Arc;
use std::time::{Duration, Instant};
use wgpu_renderer::WgpuRenderer;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::MouseScrollDelta::LineDelta;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::platform::windows::{BackdropType, IconExtWindows, WindowAttributesExtWindows};
use winit::window::{Icon, Window, WindowId};

#[derive(Default)]
struct App {
    window: Option<Arc<Window>>,
    renderer: Option<WgpuRenderer>,
    
    // Image data
    current_image: Option<ImageData>,
    img_width: u32,
    img_height: u32,
    
    gif_frames: Option<Vec<GifData>>, // Store GIF frames
    current_frame_index: u32,
    next_frame_time: Option<Instant>,
    
    panning_data: PanningData,
    
    settings_window: Option<settings_window::SettingsWindow>,
}

#[derive(Debug, Default, Copy, Clone)]
struct PanningData {
    panning: bool,
    pan_offset: PhysicalPosition<i32>,
    zoom_level: i32,
}

#[derive(Clone)]
#[allow(dead_code)]
struct ImageData {
    rgba_data: Vec<u8>,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone)]
struct GifData {
    rgba_data: Vec<u8>,
    width: u32,
    height: u32,
    delay: Delay,
}


impl ApplicationHandler for App {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::ResumeTimeReached { .. } = cause {
            self.gif_next_frame(event_loop, true);
        }
    }
    
    // init function
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // get args
        let args: Vec<String> = env::args().collect();
        let image_path = &args[1];
        dbg!(image_path);
        // loading image -- load image with image crate
        let img_reader = image::ImageReader::open(image_path).unwrap();
        let format = img_reader.with_guessed_format().unwrap().format().unwrap();
        if format == ImageFormat::Gif {
            // Load GIF and extract frames
            let gif_reader = image::codecs::gif::GifDecoder::new(BufReader::new(std::fs::File::open(image_path).unwrap())).unwrap();
            let frames = gif_reader.into_frames();
            let frames = frames.collect_frames().expect("Failed to collect GIF frames");
            let gif_frames: Vec<GifData> = frames.iter().map(|frame| {
                let delay = frame.delay();
                let dyn_img = image::DynamicImage::ImageRgba8(frame.buffer().clone());
                let rgba = dyn_img.to_rgba8().into_raw();
                GifData {
                    rgba_data: rgba,
                    width: frame.buffer().width(),
                    height: frame.buffer().height(),
                    delay
                }
            }).collect();
            println!("this is gif");
            if let Some(first_frame) = gif_frames.first() {
                let (img_width, img_height) = (first_frame.width, first_frame.height);
                dbg!(img_width, img_height);
                let window_attributes = Window::default_attributes()
                    .with_min_inner_size(LogicalSize::new(img_width, img_height))
                    .with_inner_size(LogicalSize::new(img_width, img_height))
                    .with_active(true)
                    .with_transparent(true)
                    .with_title(format!("luminix ({image_path})"))
                    .with_window_icon(Icon::from_resource(1, Some(PhysicalSize::new(128, 128))).ok())
                    .with_system_backdrop(BackdropType::TransientWindow);
                let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
                
                // Initialize wgpu renderer
                let mut renderer = pollster::block_on(WgpuRenderer::new(window.clone()));
                renderer.load_texture(&first_frame.rgba_data, first_frame.width, first_frame.height);
                
                self.window = Some(window);
                self.renderer = Some(renderer);
                self.gif_frames = Some(gif_frames.clone());
                self.current_image = Some(ImageData {
                    rgba_data: first_frame.rgba_data.clone(),
                    width: first_frame.width,
                    height: first_frame.height,
                });
                self.img_width = img_width;
                self.img_height = img_height;
                self.current_frame_index = 0;
                self.next_frame_time = Some(Instant::now() + first_frame.delay.into());
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_time.unwrap()));
                self.settings_window = Some(settings_window::SettingsWindow::new(event_loop));
            }
            return;
        }
        
        // Load regular image
        let img = image::open(image_path).expect("failed to load image");
        let rgba_img = img.to_rgba8();
        let (img_width, img_height) = rgba_img.dimensions();
        let rgba_data = rgba_img.into_raw();
        
        println!("Loading: {}, {}x{}", image_path, img_width, img_height);
        
        dbg!(img_width, img_height);
        
        // creating window
        let window_attributes = Window::default_attributes()
            .with_min_inner_size(LogicalSize::new(img_width, img_height))
            .with_inner_size(LogicalSize::new(img_width, img_height))
            .with_active(true)
            .with_transparent(true)
            .with_title(format!("luminix ({image_path})"))
            .with_window_icon(Icon::from_resource(1, Some(PhysicalSize::new(128, 128))).ok())
            .with_system_backdrop(BackdropType::TransientWindow);
        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
        
        // Initialize wgpu renderer
        let mut renderer = pollster::block_on(WgpuRenderer::new(window.clone()));
        renderer.load_texture(&rgba_data, img_width, img_height);
        
        self.window = Some(window);
        self.renderer = Some(renderer);
        self.current_image = Some(ImageData {
            rgba_data,
            width: img_width,
            height: img_height,
        });
        self.img_width = img_width;
        self.img_height = img_height;
        self.settings_window = Some(settings_window::SettingsWindow::new(event_loop));
    }
    #[allow(clippy::too_many_lines)]
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let window_ref = self.window.as_ref().unwrap();
        let settings_window = self.settings_window.as_mut().unwrap();
        
        if id == settings_window.window.id() {
            let response = settings_window.on_window_event(&event);
            if response.repaint {
                settings_window.window.request_redraw();
            }
            match event {
                WindowEvent::CursorMoved {position, ..} => {
                    settings_window.state.on_mouse_motion((position.x, position.y));
                }
                WindowEvent::RedrawRequested => {
                    settings_window.on_redraw();
                }
                _ => (),
            }
        }
        if id == window_ref.id() {
            match event {
                WindowEvent::KeyboardInput {event, ..} => {
                    if event.state.is_pressed() {
                        if let PhysicalKey::Code(code) = event.physical_key {
                            if Some(code) == settings_window.get_settings().keys.settings.get_keycode() {
                                self.settings_window.as_ref().unwrap().show();
                            } else if Some(code) == settings_window.get_settings().keys.pause.get_keycode() {
                                if self.gif_frames.is_some() {
                                    match event_loop.control_flow() {
                                        ControlFlow::WaitUntil(_) => {event_loop.set_control_flow(ControlFlow::Wait)}
                                        ControlFlow::Wait => {event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_time.unwrap()))}
                                        ControlFlow::Poll => {}
                                    }
                                }
                            } else if Some(code) == settings_window.get_settings().keys.next_frame.get_keycode() {
                                if self.gif_frames.is_some() && event_loop.control_flow() == ControlFlow::Wait {
                                    // Paused
                                    self.gif_next_frame(event_loop, false);
                                }
                            } else if Some(code) == settings_window.get_settings().keys.prev_frame.get_keycode() && self.gif_frames.is_some() && event_loop.control_flow() == ControlFlow::Wait {
                                // Paused
                                self.gif_prev_frame(event_loop, false,);
                            }
                            // actions
                            for (action, key) in self.settings_window.as_ref().unwrap().configurable_settings.actions.iter().zip(self.settings_window.as_ref().unwrap().configurable_settings.keys.actions.iter()) {
                                if Some(code) == key.get_keycode() {
                                    let _ = action.run_action();
                                }
                            }
                        }
                    }
                }
                WindowEvent::CloseRequested => {
                    println!("The close button was pressed; stopping");
                    event_loop.exit();

                },
                WindowEvent::MouseInput {state, button, .. } => {
                    // dbg!(button, state);

                    if button == MouseButton::Middle {
                        match state {
                            ElementState::Pressed => {
                                self.panning_data.panning = true;
                                let (x, y): (u32, u32) = window_ref.inner_size().into();
                                window_ref.set_cursor_position(PhysicalPosition::new(x/2, y/2)).expect("Error setting cursor position");
                                window_ref.set_cursor_visible(false);
                            }
                            ElementState::Released => {
                                self.panning_data.panning = false;
                                window_ref.set_cursor_visible(true);

                            }
                        }

                    }
                }
                WindowEvent::Resized(new_size) => {
                    // self.panning_data.pan_offset = PhysicalPosition::new(0, 0);
                    // self.panning_data.zoom_level = 0;
                    window_ref.request_redraw();
                    if let Some(renderer) = &mut self.renderer {
                        renderer.resize(new_size);
                        // Render immediately during resize for real-time updates
                        // let _ = renderer.render();
                    }
                }
                WindowEvent::MouseWheel {delta, ..} => {
                    dbg!(delta);

                    let max_zoom_level = 100;
                    match delta {
                        LineDelta(_, y) => {
                            if y.is_sign_positive() {
                                if self.panning_data.zoom_level < max_zoom_level {
                                    self.panning_data.zoom_level += 1;
                                }
                            } else if self.panning_data.zoom_level > -max_zoom_level {
                                self.panning_data.zoom_level -= 1;
                            }
                            
                            // Update renderer zoom
                            if let Some(renderer) = &mut self.renderer {
                                let image_aspect = self.img_width as f32 / self.img_height as f32;
                                renderer.set_zoom(self.panning_data.zoom_level, image_aspect);
                            }
                        }
                        MouseScrollDelta::PixelDelta(_) => {
                            // TODO: add this
                            // or dont it only affects trackpad users
                        }
                    }
                    window_ref.request_redraw();
                }
                WindowEvent::CursorMoved {position, .. } => {
                    if self.panning_data.panning {
                        // adjust panning offset
                        let (mouse_pos_x, mouse_pos_y): (i32, i32) = position.into();

                        let (window_size_x, window_size_y): (u32, u32) = window_ref.inner_size().into();

                        // Negate offset so moving mouse right moves image right
                        let offset_x = -( mouse_pos_x - (window_size_x as i32)/2);
                        let offset_y = -(mouse_pos_y - (window_size_y as i32)/2);
                        // if applying offset will make offset greater than image size, don't apply offset
                        if (self.panning_data.pan_offset.x + offset_x).unsigned_abs() < self.img_width {
                            self.panning_data.pan_offset.x += offset_x;
                        }
                        if (self.panning_data.pan_offset.y + offset_y).unsigned_abs() < self.img_height {
                            self.panning_data.pan_offset.y += offset_y;
                        }

                        // Update renderer pan
                        if let Some(renderer) = &mut self.renderer {
                            renderer.set_pan(self.panning_data.pan_offset, self.img_width, self.img_height);
                        }

                        window_ref.request_redraw();

                        window_ref.set_cursor_position(PhysicalPosition::new(window_size_x/2, window_size_y/2)).expect("Error setting cursor position");
                    }
                }
                WindowEvent::RedrawRequested => {
                    if let Some(renderer) = &mut self.renderer {
                        match renderer.render() {
                            Ok(_) => {}
                            Err(wgpu::SurfaceError::Lost) => {
                                let size = window_ref.inner_size();
                                renderer.resize(size);
                            }
                            Err(wgpu::SurfaceError::OutOfMemory) => {
                                eprintln!("Out of memory!");
                                event_loop.exit();
                            }
                            Err(e) => eprintln!("Render error: {:?}", e),
                        }
                    }
                }
                _ => (),
            }
        }
    }
}

impl App {
    fn gif_next_frame(&mut self, event_loop: &ActiveEventLoop, schedule_next_frame: bool) {
        if let Some(gif_frames) = self.gif_frames.clone() {
            println!("------------------------");
            let current_frame = &gif_frames[self.current_frame_index as usize];
            
            // Update current image
            self.current_image = Some(ImageData {
                rgba_data: current_frame.rgba_data.clone(),
                width: current_frame.width,
                height: current_frame.height,
            });
            
            // Load new texture into renderer
            if let Some(renderer) = &mut self.renderer {
                renderer.load_texture(&current_frame.rgba_data, current_frame.width, current_frame.height);
            }

            // schedule the next frame
            self.current_frame_index = (self.current_frame_index + 1) % u32::try_from(gif_frames.len()).unwrap_or_default();
            self.next_frame_time = Some(Instant::now() + Duration::from_millis(u64::from(
                gif_frames[self.current_frame_index as usize].delay.numer_denom_ms().0 / gif_frames[self.current_frame_index as usize].delay.numer_denom_ms().1
            )));
            println!("{:?}", u64::from(gif_frames[self.current_frame_index as usize].delay.numer_denom_ms().0 / gif_frames[self.current_frame_index as usize].delay.numer_denom_ms().1));
            dbg!(self.current_frame_index);
            self.window.as_ref().unwrap().request_redraw();
            if schedule_next_frame {
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_time.expect("REASON")));
            }
        }
    }
    fn gif_prev_frame(&mut self, event_loop: &ActiveEventLoop, schedule_next_frame: bool) {
        if let Some(gif_frames) = self.gif_frames.clone() {
            println!("------------------------");
            let current_frame = &gif_frames[self.current_frame_index as usize];
            
            // Update current image
            self.current_image = Some(ImageData {
                rgba_data: current_frame.rgba_data.clone(),
                width: current_frame.width,
                height: current_frame.height,
            });
            
            // Load new texture into renderer
            if let Some(renderer) = &mut self.renderer {
                renderer.load_texture(&current_frame.rgba_data, current_frame.width, current_frame.height);
            }

            // schedule the next frame
            if self.current_frame_index > 0 {
                self.current_frame_index -= 1;
            } else {
                self.current_frame_index = u32::try_from(gif_frames.len()).unwrap_or_default() - 1;
            }

            self.next_frame_time = Some(Instant::now() + Duration::from_millis(u64::from(gif_frames[self.current_frame_index as usize].delay.numer_denom_ms().0 / gif_frames[self.current_frame_index as usize].delay.numer_denom_ms().1)));
            println!("{:?}", u64::from(gif_frames[self.current_frame_index as usize].delay.numer_denom_ms().0 / gif_frames[self.current_frame_index as usize].delay.numer_denom_ms().1));
            dbg!(self.current_frame_index);
            self.window.as_ref().unwrap().request_redraw();
            if schedule_next_frame {
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_time.expect("REASON")));
            }
        }
    }
}


fn main() {
    // check if valid args before anything else
    if env::args().collect::<Vec<_>>().len() != 2 {
        eprintln!("Usage: luminix <image_path>");
        return;
    };
    
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("error running event loop");
}
