mod settings_window;
mod register_file_association;

use bytemuck::cast_slice;
use fast_image_resize::images::Image;
use fast_image_resize::{IntoImageView, PixelType, ResizeAlg, ResizeOptions, Resizer, SrcCropping};
use fraction::{Integer, Zero};
use image::{AnimationDecoder, Delay, ImageFormat};
use photon_rs::PhotonImage;
use softbuffer::{Context, Surface};
use std::io::BufReader;
use std::num::NonZeroU32;
use std::ops::Div;
use std::time::{Duration, Instant};
use std::{env, time};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::MouseScrollDelta::LineDelta;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::windows::{BackdropType, IconExtWindows, WindowAttributesExtWindows};
use winit::raw_window_handle::HasDisplayHandle;
use winit::window::{Icon, Window, WindowId};

#[derive(Default)]
struct App {
    window: Option<Box<Window>>,
    img: Option<PhotonImage>,
    
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
#[derive(Copy, Clone, Default, Debug)]
struct Size {
    width: u32,
    height: u32,
}

#[derive(Debug, Clone)]
struct GifData {
    frame: PhotonImage,
    delay: Delay,
}

fn fast_resize_photon_img(img: &PhotonImage, new_img_size: Size) -> PhotonImage {
    let dyn_img = photon_rs::helpers::dyn_image_from_raw(img);
    let src_img = Image::from_vec_u8(img.get_width(), img.get_height(), dyn_img.into_bytes(), PixelType::U8x4).expect("aasdsad");
    let mut dst_image = Image::new(
        new_img_size.width,
        new_img_size.height,
        src_img.pixel_type(),
    );


    let mut resizer = Resizer::new();

    resizer.resize(&src_img, &mut dst_image, &ResizeOptions { algorithm: ResizeAlg::Nearest, cropping: SrcCropping::None, mul_div_alpha: true }).unwrap();

    let width = dst_image.width();
    let height = dst_image.height();

    
    PhotonImage::new(dst_image.into_vec(), width, height)
}


/// Calculates the maximum buffer size for an image based on the window dimensions and image size.
/// More specifically, the buffer size created is the aspect ratio of the window (to make sure the buffer always completely fills the window frame) but it is the physical pixel size of the image to maximize image space.
///
/// # Arguments
/// * `width` - The width of the window.
/// * `height` - The height of the window.
/// * `img_size` - The size of the image.
///
/// # Returns
/// A `Size` struct representing the new dimensions of the buffer.
fn calculate_max_buffer_size(window_width: u32, window_height: u32, img_size: Size) -> Size {
    let window_width_f= window_width as f64;
    let window_height_f = window_height as f64;
    let img_width_f = img_size.width as f64;
    let img_height_f = img_size.height as f64;
    
    
    if img_size.width == 0 || img_size.height == 0 {
        // Avoid division by zero
        return Size { width: window_width, height: window_height };
    }
    // if window_width > window_height {
    if (img_width_f / img_height_f) < (window_width_f / window_height_f) {
        /*
        +--------+
        |        |
        +--------+
         */
        let aspect_ratio = img_width_f / img_height_f;
        // dbg!(aspect_ratio);
        let new_width = aspect_ratio * window_height_f;
        let new_width = new_width as u32;
        if new_width == 0 {
            return Size { width: window_width, height: window_height };
        }
        Size {
            width: new_width,
            height: window_height,
        }
        // } else if window_width < window_height {
    } else if (img_width_f / img_height_f) > (window_width_f / window_height_f) {
        /*
        +--+
        |  |
        |  |
        +--+
         */
        let aspect_ratio = img_height_f / img_width_f;
        let new_height = aspect_ratio * window_width_f;
        let new_height = new_height as u32;
        if new_height == 0 {
            return Size { width: window_width, height: window_height };
        }
        Size {
            width: window_width,
            height: new_height,
        }
    } else {
        /*
        +----+
        |    |
        +----+
         */
        Size {
            width: window_width,
            height: window_height,
        }
    }
}

/// Pads an image to fit within new dimensions.
///
/// # Arguments
/// * `img` - The original image to be padded.
/// * `old_size` - The original size of the image.
/// * `new_size` - The new size to pad the image to.
///
/// # Returns
/// A new `PhotonImage` instance with the applied padding.
fn pad_img(img: PhotonImage, new_size: Size) -> PhotonImage {
    

    let total_pad_vertical = new_size.height - img.get_height();
    let total_pad_horizontal = new_size.width - img.get_width();
    
    let pad_left;
    let pad_right;
    if total_pad_horizontal != 0 {
        pad_left = total_pad_horizontal.div_ceil(2);
        pad_right = total_pad_horizontal.div(2);
    } else { 
        pad_left = 0;
        pad_right = 0;
    }
    let pad_top;
    let pad_bottom;
    if total_pad_vertical != 0 {
        pad_top = total_pad_vertical.div_ceil(2);
        pad_bottom = total_pad_vertical.div(2);
    } else {
        pad_top = 0;
        pad_bottom = 0;
    }
    
    

    pad_img_sides(img, pad_left, pad_right, pad_top, pad_bottom)
}

/// Applies panning to an image based on the provided panning data.
///
/// # Arguments
/// * `img` - The original image to be panned.
/// * `panning_data` - The data containing panning state and offset.
///
/// # Returns
/// A new `PhotonImage` instance with the applied panning.
fn pan_img(img: PhotonImage, panning_data: PanningData) -> PhotonImage {
    let pan_time = time::Instant::now();
    let pan_math_time = time::Instant::now();
    let pan_offset_x = panning_data.pan_offset.x;
    let pan_offset_y = panning_data.pan_offset.y;

    let pan_offset_x_abs = pan_offset_x.unsigned_abs();
    let pan_offset_y_abs = pan_offset_y.unsigned_abs();
    
    if pan_offset_x.is_zero() && pan_offset_y.is_zero() {
        return img.clone();
    }
    
    // get size of original image
    let img_size = Size{width: img.get_width(), height: img.get_height()};
    
    // dbg!((pan_offset_x,pan_offset_x_abs),(pan_offset_y,pan_offset_y_abs));
    
    //define corner vars
    let (pos_x, pos_y, neg_x, neg_y);
    let (pad_left, pad_right, pad_top, pad_bottom);
    
    // width cropping
    if pan_offset_x.is_positive() {
        // pan to the right (remove right)
        pos_x = img_size.width - pan_offset_x_abs;
        neg_x = 0;
        
        pad_left = pan_offset_x_abs;
        pad_right = 0;
    } else {
        // pan to the left (remove left)
        pos_x = img_size.width;
        neg_x = pan_offset_x_abs;

        pad_left = 0;
        pad_right = pan_offset_x_abs;
    }
    
    // height cropping
    if pan_offset_y.is_positive() {
        // pan up (remove up)
        pos_y = img_size.height - pan_offset_y_abs;
        neg_y = 0;
        
        pad_top = pan_offset_y_abs;
        pad_bottom = 0;
    } else {
        // pan down (remove down)
        pos_y = img_size.height;
        neg_y = pan_offset_y_abs;
        
        pad_top = 0;
        pad_bottom = pan_offset_y_abs;
    }

    // dbg!(pos_x, neg_x, pos_y, neg_y);
    println!("|   |   Pan math time: {:?}", pan_math_time.elapsed());
    let pan_math_time = time::Instant::now();


    let cropped_img = fast_crop(img, pos_x, pos_y, neg_x, neg_y);
    println!("|   |   Pan crop time: {:?}", pan_math_time.elapsed());
    let pan_math_time = time::Instant::now();

    // let cropped_img = photon_rs::transform::crop(img, neg_x, neg_y, pos_x, pos_y);

    let padded_cropped_img = pad_img_sides(cropped_img, pad_left, pad_right, pad_top, pad_bottom);
    println!("|   |   Pan pad time: {:?}", pan_math_time.elapsed());

    println!("|  Pan time: {:?}", pan_time.elapsed());
    padded_cropped_img
}

fn fast_crop(img: PhotonImage, pos_x: u32, pos_y: u32, neg_x: u32, neg_y: u32) -> PhotonImage {
    let new_img_size = Size {
        width: pos_x - neg_x,
        height: pos_y - neg_y,
    };

    let dyn_img = photon_rs::helpers::dyn_image_from_raw(&img);
    let src_img = Image::from_vec_u8(img.get_width(), img.get_height(), dyn_img.into_bytes(), PixelType::U8x4).expect("aasdsad");
    let mut dst_image = Image::new(
        new_img_size.width,
        new_img_size.height,
        src_img.pixel_type(),
    );

    let mut resizer = Resizer::new();

    resizer.resize(&src_img, &mut dst_image, &ResizeOptions::new().resize_alg(ResizeAlg::Nearest).crop(neg_x as _, neg_y as _, new_img_size.width as _, new_img_size.height as _)).unwrap();

    let width = dst_image.width();
    let height = dst_image.height();

    
    PhotonImage::new(dst_image.into_vec(), width, height)
}

fn pad_img_sides (img: PhotonImage, pad_left: u32, pad_right: u32, pad_top: u32, pad_bottom: u32) -> PhotonImage {
    // old_code
    // let padded_cropped_img =
    //     padding_left(
    //         &padding_right(
    //             &padding_bottom(
    //                 &padding_top(
    //                     &img,
    //                     pad_top, rgba_transparent_generator()),
    //                 pad_bottom, rgba_transparent_generator()),
    //             pad_right, rgba_transparent_generator()),
    //         pad_left, rgba_transparent_generator());
    // end old code
    // new code
    use rayon::prelude::*;
    use std::sync::{Arc, Mutex};

    let new_width = img.get_width() + pad_left + pad_right;
    let new_height = img.get_height() + pad_top + pad_bottom;

    // Create a new buffer filled with transparent pixels
    let new_buffer = vec![0u8; (new_width * new_height * 4) as usize];
    
    // Wrap the buffer in an Arc<Mutex<>> to allow safe parallel modification
    let buffer = Arc::new(Mutex::new(new_buffer));
    
    // Copy the source image pixels into the correct position in the new buffer
    let src_pixels = img.get_raw_pixels();
    let src_width = img.get_width();
    let src_height = img.get_height();

    // Process rows in parallel
    (0..src_height).into_par_iter().for_each(|y| {
        let src_row_start = (y * src_width * 4) as usize;
        let src_row_end = src_row_start + (src_width * 4) as usize;
        let dst_row_start = ((y + pad_top) * new_width * 4 + pad_left * 4) as usize;
        let dst_row_end = dst_row_start + (src_width * 4) as usize;
        
        // Create a row buffer with the pixels we want to copy
        let row_data = src_pixels[src_row_start..src_row_end].to_vec();
        
        // Lock the buffer only for the time needed to update it
        let mut buffer = buffer.lock().unwrap();
        buffer[dst_row_start..dst_row_end].copy_from_slice(&row_data);
    });

    // Unwrap the Arc<Mutex<>> to get back our buffer
    let final_buffer = Arc::try_unwrap(buffer)
        .expect("Failed to unwrap Arc")
        .into_inner()
        .expect("Failed to unwrap Mutex");

    PhotonImage::new(final_buffer, new_width, new_height)
}

/// Applies zooming to an image based on the provided panning data.
///
/// # Arguments
/// * `img` - The original image to be zoomed.
/// * `panning_data` - The data containing the zoom level.
///
/// # Returns
/// A new `PhotonImage` instance with the applied zoom.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, clippy::cast_sign_loss)]
fn zoom_img(img: PhotonImage, panning_data: PanningData) -> PhotonImage {
    let zoom_level = panning_data.zoom_level;
    
    // zoom level should be so that 10 zooms gets you to around one pixel
    let zoom_level_multiplier = 
        if img.get_height() > img.get_width() {
            // use width
            img.get_width() as f32 / 10_f32
        } else {
            // use height
            img.get_height() as f32 / 10_f32
        };

    let zoom_constant = zoom_level.unsigned_abs() as f32 * zoom_level_multiplier / 2_f32;
    // dbg!(zoom_constant, zoom_level_multiplier);
    // dbg!(zoom_constant);
    
    
    
    
    // return this
    
    if zoom_level.is_positive()
    {
        // let pos_x = img.get_width() - zoom_constant;
        // let pos_y = img.get_height() - zoom_constant;
        // let neg_x = zoom_constant;
        // let neg_y= zoom_constant;
        // 
        // dbg!(pos_x, neg_x, pos_y, neg_y);
        // zoomed_img = fast_crop(img, pos_x, pos_y, neg_x, neg_y);

        let zoom_ratio = zoom_level as f32 / 10f32; // 0.0 - 1.0
        // total percent to crop, e.g. 0.3 means remove 30% total (15% per side)
        let crop_ratio = 0.8 * zoom_ratio; // up to 80% total crop

        let aspect = img.get_width() as f32 / img.get_height() as f32;
        
        let base_crop_width = img.get_width() as f32 * crop_ratio;
        let base_crop_height = img.get_height() as f32 * crop_ratio;

        // adjust one dimension to maintain aspect ratio
        let (crop_width, crop_height) = if aspect >= 1.0 {
            // wide image
            let h = img.get_height() as f32 - base_crop_height;
            let w = h * aspect;
            (w, h)
        } else {
            // tall image
            let w = img.get_width() as f32 - base_crop_width;
            let h = w / aspect;
            (w, h)
        };

        let center_x = img.get_width() as f32 / 2.0;
        let center_y = img.get_height() as f32 / 2.0;

        let x0 = (center_x - crop_width / 2.0).max(0.0);
        let y0 = (center_y - crop_height / 2.0).max(0.0);

        let x1 = (x0 + crop_width).min(img.get_width() as f32);
        let y1 = (y0 + crop_height).min(img.get_height() as f32);
        
        fast_crop(img, x1 as u32, y1 as u32, x0 as u32, y0 as u32)
        
    } else {
        let pad_top = zoom_constant as u32;
        let pad_bottom = zoom_constant as u32;
        let pad_left = zoom_constant as u32;
        let pad_right = zoom_constant as u32;
        // dbg!(pad_top);
        pad_img_sides(img, pad_left, pad_right, pad_top, pad_bottom)
    }
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
            let photon_frames: Vec<GifData> = frames.iter().map(|frame| {
                let delay = frame.delay();
                let dyn_img = image::DynamicImage::ImageRgba8(frame.buffer().clone());
                let rgba = dyn_img.to_rgba8().into_raw();
                let mut photon_img = PhotonImage::new(rgba, frame.buffer().width(), frame.buffer().height());
                photon_rs::channels::swap_channels(&mut photon_img, 0, 2);
                GifData {frame: photon_img, delay}
            }).collect();
            println!("this is gif");
            if let Some(first_frame) = photon_frames.first() {
               
                
                let (img_width, img_height) = (first_frame.frame.get_width(), first_frame.frame.get_height());
                dbg!(img_width, img_height);
                let window_attributes = Window::default_attributes()
                    .with_min_inner_size(LogicalSize::new(img_width, img_height))
                    .with_inner_size(LogicalSize::new(img_width, img_height))
                    .with_active(true)
                    .with_transparent(true)
                    .with_title(format!("luminix ({image_path})"))
                    .with_window_icon(Icon::from_resource(1, Some(PhysicalSize::new(128, 128))).ok())
                    .with_system_backdrop(BackdropType::TransientWindow);
                let window = Box::new(event_loop.create_window(window_attributes).unwrap());
                self.window = Some(window);
                self.gif_frames = Some(photon_frames.clone()); // Use clone to avoid move error
                self.img = Some(first_frame.frame.clone()); // Display first frame by default
                self.current_frame_index = 0;
                self.next_frame_time = Some(Instant::now() + first_frame.delay.into());
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_time.unwrap()));
                self.settings_window = Some(settings_window::SettingsWindow::new(event_loop));
            }
            return;
        }
        let mut img = photon_rs::native::open_image(image_path).expect("failed to load image");
        photon_rs::channels::swap_channels(&mut img, 0, 2);
        
        let (img_width, img_height) = (img.get_width(), img.get_height());
        dbg!(img_width, img_height);
        
        
        // creating window
        let window_attributes = Window::default_attributes()
            .with_min_inner_size(LogicalSize::new(img_width, img_height))
            .with_inner_size(LogicalSize::new(img_width, img_height))
            .with_active(true)
            // .with_enabled_buttons(WindowButtons::CLOSE)
            .with_transparent(true)
            .with_title(format!("luminix ({image_path})"))
            .with_window_icon(Icon::from_resource(1, Some(PhysicalSize::new(128, 128))).ok())
            .with_system_backdrop(BackdropType::TransientWindow);
        let window = Box::new(event_loop.create_window(window_attributes).unwrap());
        
        
        //continue loading image
        // let surface_texture = SurfaceTexture::new(width, height, window_ptr);
        // let pixels = Pixels::new(width, height, surface_texture).expect("Failed to create pixel buffer");
        self.window = Some(window);
        // self.pixels = Some(pixels);
        self.img = Some(img);
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
                            let settings_keycode = settings_window.get_settings().keys.settings.get_keycode();
                            if code == settings_keycode {
                                
                            }
                            match code {
                                KeyCode::KeyP => {
                                    if self.gif_frames.is_some() {
                                        match event_loop.control_flow() {
                                            ControlFlow::WaitUntil(_) => {event_loop.set_control_flow(ControlFlow::Wait)}
                                            ControlFlow::Wait => {event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_time.unwrap()))}
                                            ControlFlow::Poll => {}
                                        }
                                    }
                                }
                                KeyCode::Period => {
                                    if self.gif_frames.is_some() && event_loop.control_flow() == ControlFlow::Wait {
                                        // Paused
                                        self.gif_next_frame(event_loop, false);
                                    }
                                }
                                KeyCode::Comma => {
                                    if self.gif_frames.is_some() && event_loop.control_flow() == ControlFlow::Wait {
                                        // Paused
                                        self.gif_prev_frame(event_loop, false,);
                                    }
                                }
                                code if code == settings_keycode => {
                                    self.settings_window.as_mut().unwrap().show();
                                }
                                _ => {}
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
                WindowEvent::Resized(..) => {
                    self.panning_data.pan_offset = PhysicalPosition::new(0, 0);
                    self.panning_data.zoom_level = 0;
                }
                WindowEvent::MouseWheel {delta, ..} => {
                    dbg!(delta);

                    let max_zoom_level = 10;
                    match delta {
                        LineDelta(_, y) => {
                            if y.is_sign_positive() {
                                if self.panning_data.zoom_level < max_zoom_level {
                                    self.panning_data.zoom_level += 1;
                                }
                            } else if self.panning_data.zoom_level > -max_zoom_level {
                                self.panning_data.zoom_level -= 1;
                            }
                            // dbg!(self.panning_data.zoom_level);
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
                        // dbg!(position);
                        // adjust panning offset
                        let (mouse_pos_x, mouse_pos_y): (i32, i32) = position.into();

                        let (window_size_x, window_size_y): (u32, u32) = window_ref.inner_size().into();


                        let offset_x = mouse_pos_x - (window_size_x as i32)/2;
                        let offset_y = mouse_pos_y - (window_size_y as i32)/2;
                        // if applying offset will make offset greater than image size, don't apply offset
                        if (self.panning_data.pan_offset.x + offset_x).unsigned_abs() < self.img.as_ref().unwrap().get_width() {
                            self.panning_data.pan_offset.x += offset_x;
                        }
                        if (self.panning_data.pan_offset.y + offset_y).unsigned_abs() < self.img.as_ref().unwrap().get_height() {
                            self.panning_data.pan_offset.y += offset_y;
                        }

                        // dbg!(self.panning_data);
                        window_ref.request_redraw();
                        // dbg!((offset_x,offset_y));

                        window_ref.set_cursor_position(PhysicalPosition::new(window_size_x/2, window_size_y/2)).expect("Error setting cursor position");
                    }
                }
                WindowEvent::RedrawRequested => {
                    let total_time = std::time::Instant::now();
                    let start_time = std::time::Instant::now();
                    // setup
                    let display = window_ref.display_handle().unwrap();
                    let context = Context::new(display).unwrap();
                    let mut surface = Surface::new(&context,window_ref)
                        .expect("error in surface definition");
                    println!("Setup time: {:?}", start_time.elapsed());
                    let start_time = std::time::Instant::now();


                    // define widths and heights
                    let window_width = window_ref.inner_size().width;
                    let window_height = window_ref.inner_size().height;
                    let window_size = Size {
                        width: window_width,
                        height: window_height,
                    };



                    // resize everything
                    if window_width.is_zero() || window_height.is_zero() {
                        return;
                    }
                    surface.resize(NonZeroU32::try_from(window_width).unwrap(), NonZeroU32::try_from(window_height).unwrap()).unwrap(); // resize buffer

                    println!("Surface resize time: {:?}", start_time.elapsed());
                    let start_time = Instant::now();


                    let start_img = self.img.as_ref().unwrap().clone();




                    let middle_img = if self.panning_data.zoom_level <= 0 {
                        // if zoomed out, perform the zooming then the panning, otherwise, do panning then zooming
                        let zoomed_img = zoom_img(start_img, self.panning_data); // zoom image
                        // println!("zoom - pan");

                        pan_img(zoomed_img, self.panning_data) // pan image and return
                    } else {
                        let panned_img = pan_img(start_img, self.panning_data); // pan image

                        // println!("pan - zoom");
                        // dbg!((zoomed_img.get_width(), zoomed_img.get_height()));
                        // dbg!((panned_img.get_width(), panned_img.get_height()));
                        zoom_img(panned_img.clone(), self.panning_data) // zoom image
                    };

                    println!("Zoom and pan time: {:?}", start_time.elapsed());
                    let start_time = time::Instant::now();


                    let original_img_size = Size{ width: middle_img.get_width(), height: middle_img.get_height(), };
                    // dbg!(original_img_size);
                    let new_img_size = calculate_max_buffer_size(window_width, window_height, original_img_size);
                    // dbg!(new_img_size);
                    // dbg!(Size{width: window_width, height:window_height,});

                    println!("Calc max buffer time: {:?}", start_time.elapsed());
                    let start_time = time::Instant::now();


                    let resized_img = fast_resize_photon_img(&middle_img, new_img_size);

                    // let resized_img = resize(&middle_img, new_img_size.width, new_img_size.height, SamplingFilter::Nearest); // resize image
                    println!("Resize time: {:?}", start_time.elapsed());
                    let start_time = time::Instant::now();

                    let padded_img = pad_img(resized_img, window_size); // pad image
                    println!("Padding time: {:?}", start_time.elapsed());
                    let start_time = time::Instant::now();

                    let mut buffer = surface.buffer_mut().unwrap();
                    let raw_pixel_vec = padded_img.get_raw_pixels();
                    let raw_pixel_slice = raw_pixel_vec.as_slice();
                    let casted_pixel_slice = cast_slice::<u8, u32>(raw_pixel_slice);
                    buffer.copy_from_slice(casted_pixel_slice);

                    window_ref.pre_present_notify();
                    buffer.present().unwrap();
                    println!("Buffer copy and present time: {:?}", start_time.elapsed());
                    println!("Total Frame Time: {:?}", total_time.elapsed());
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
            self.img = Some(current_frame.frame.clone());

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
            self.img = Some(current_frame.frame.clone());

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

    // ControlFlow::Wait 
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("error running event loop");
}
