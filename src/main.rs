use bytemuck::cast_slice;
use fraction::{Fraction, Integer, ToPrimitive, Zero};
use photon_rs;
use photon_rs::transform::{padding_bottom, padding_left, padding_right, padding_top, resize, SamplingFilter};
use photon_rs::{PhotonImage, Rgba};
use softbuffer;
use softbuffer::{Context, Surface};
use std::env;
use std::num::NonZeroU32;
use std::ops::{Div, Mul};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::MouseScrollDelta::LineDelta;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::windows::{BackdropType, WindowAttributesExtWindows};
use winit::raw_window_handle::HasDisplayHandle;
use winit::window::{Window, WindowButtons, WindowId};

#[derive(Default)]
struct App {
    window: Option<Box<Window>>,
    // pixels: Option<Pixels<'static>>,
    img: Option<PhotonImage>,
    panning_data: PanningData,
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
fn calculate_max_buffer_size(width: u32, height: u32, img_size: Size) -> Size {
    if width > height {
         /*
         +--------+
         |        |
         +--------+
          */
        let aspect_ratio = Fraction::new(img_size.width, img_size.height);
        let new_width = aspect_ratio.mul(height);
        let new_width = new_width.to_u64().expect("failed to convert fraction back to number").to_u32().unwrap();

        Size{
            width: new_width,
            height,
        }
    } else if width < height {
        /*
        +--+
        |  |
        |  |
        +--+
         */
        let aspect_ratio = Fraction::new(img_size.height, img_size.width);
        let new_height = aspect_ratio.mul(width);
        let new_height = new_height.to_u64().expect("failed to convert fraction back to number").to_u32().unwrap();

        Size{
            width,
            height: new_height,
        }
    } else {
        /*
        +----+
        |    |
        +----+
         */
        Size {
            width,
            height,
        }
    }
}

fn rgba_transparent_generator() -> Rgba {
    Rgba::new(0_u8, 0_u8, 0_u8, 0_u8) // transparent padding
    // Rgba::new(255_u8, 0_u8, 0_u8, 255_u8) // red padding (debug)
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

    let start_time = std::time::Instant::now();

    let new_img = 
        padding_left(
            &padding_right(
                &padding_bottom(
                    &padding_top(
                        &img,
                        pad_top, rgba_transparent_generator()),
                    pad_bottom, rgba_transparent_generator()),
                pad_right, rgba_transparent_generator()),
            pad_left, rgba_transparent_generator());
    

    println!("paddingInThePadding Time: {:?}", start_time.elapsed());

    new_img
}

/// Applies panning to an image based on the provided panning data.
///
/// # Arguments
/// * `img` - The original image to be panned.
/// * `panning_data` - The data containing panning state and offset.
///
/// # Returns
/// A new `PhotonImage` instance with the applied panning.
fn pan_img(img: &PhotonImage, panning_data: PanningData) -> PhotonImage {
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

    dbg!(pos_x, neg_x, pos_y, neg_y);
    // if zoomed out, find zoom level and adjust crop to fit.
    
    let cropped_img = photon_rs::transform::crop(img, neg_x, neg_y, pos_x, pos_y);

    let padded_cropped_img = pad_img_sides(cropped_img, pad_left, pad_right, pad_top, pad_bottom);
    

    
    padded_cropped_img
}

fn pad_img_sides (cropped_img: PhotonImage, pad_left: u32, pad_right: u32, pad_top: u32, pad_bottom: u32) -> PhotonImage {
    let padded_cropped_img =
        padding_left(
            &padding_right(
                &padding_bottom(
                    &padding_top(
                        &cropped_img,
                        pad_top, rgba_transparent_generator()),
                    pad_bottom, rgba_transparent_generator()),
                pad_right, rgba_transparent_generator()),
            pad_left, rgba_transparent_generator());
    padded_cropped_img
}

/// Applies zooming to an image based on the provided panning data.
///
/// # Arguments
/// * `img` - The original image to be zoomed.
/// * `panning_data` - The data containing the zoom level.
///
/// # Returns
/// A new `PhotonImage` instance with the applied zoom.
fn zoom_img(img: PhotonImage, panning_data: PanningData) -> PhotonImage {
    let zoom_level = panning_data.zoom_level;
    
    // zoom level should be so that 10 zooms gets you to one pixel
    let zoom_level_multiplier = 
        if img.get_height() > img.get_width() {
            // use width
            img.get_width()/10    
        } else {
            // use height
            img.get_height()/10
        };

    let zoom_constant = zoom_level.unsigned_abs() * zoom_level_multiplier / 2;

    // dbg!(zoom_constant);
    
    let zoomed_img;
    if zoom_level.is_positive() {
        let mut pos_x = img.get_width();
        let mut pos_y = img.get_height();
        let mut neg_x = 0;
        let mut neg_y= 0;
        
        pos_x -= zoom_constant;
        pos_y -= zoom_constant;
        neg_x += zoom_constant;
        neg_y += zoom_constant;
        
        // dbg!(pos_x, neg_x, pos_y, neg_y);
        zoomed_img = photon_rs::transform::crop(&img, neg_x, neg_y, pos_x, pos_y);
    } else {
        let pad_top = zoom_constant;
        let pad_bottom = zoom_constant;
        let pad_left = zoom_constant;
        let pad_right = zoom_constant;
        // dbg!(pad_top);
        zoomed_img = pad_img_sides(img, pad_left, pad_right, pad_top, pad_bottom);
    }
    zoomed_img
}

impl ApplicationHandler for App {
    // init function
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // get args
        let args: Vec<String> = env::args().collect();
        let image_path = &args[1];
        dbg!(image_path);
        // loading image -- load image with image crate
        let img = photon_rs::native::open_image(image_path).expect("failed to load image");
        let (img_width, img_height) = (img.get_width(), img.get_height());
        dbg!(img_width, img_height);
        
        // creating window
        let window_attributes = Window::default_attributes()
            .with_min_inner_size(LogicalSize::new(img_width, img_height))
            .with_inner_size(LogicalSize::new(img_width, img_height))
            .with_active(true)
            .with_enabled_buttons(WindowButtons::CLOSE)
            .with_transparent(true)
            .with_system_backdrop(BackdropType::TransientWindow);
        let window = Box::new(event_loop.create_window(window_attributes).unwrap());
        
        //continue loading image
        // let surface_texture = SurfaceTexture::new(width, height, window_ptr);
        // let pixels = Pixels::new(width, height, surface_texture).expect("Failed to create pixel buffer");
        self.window = Some(window);
        // self.pixels = Some(pixels);
        self.img = Some(img);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let window_ref = self.window.as_ref().unwrap();
        match event {
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
                
            },
            
            WindowEvent::MouseInput {state, button, .. } => {
                dbg!(button, state);
                
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
                    LineDelta(x, y) => {
                        if y.is_sign_positive() {
                            if self.panning_data.zoom_level < max_zoom_level {
                                self.panning_data.zoom_level += 1;
                            }
                        } else {
                            if self.panning_data.zoom_level > -max_zoom_level {
                                self.panning_data.zoom_level -= 1;
                            }
                        }
                        dbg!(self.panning_data.zoom_level);
                    }
                    MouseScrollDelta::PixelDelta(position) => {
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
                    
                    dbg!(self.panning_data);
                    window_ref.request_redraw();
                    // dbg!((offset_x,offset_y));
                    
                    window_ref.set_cursor_position(PhysicalPosition::new(window_size_x/2, window_size_y/2)).expect("Error setting cursor position");
                }
            }

            WindowEvent::RedrawRequested => {
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
                surface.resize(NonZeroU32::try_from(window_width).unwrap(), NonZeroU32::try_from(window_height).unwrap()).unwrap(); // resize buffer

                println!("Surface resize time: {:?}", start_time.elapsed());
                let start_time = std::time::Instant::now();


                let start_img = self.img.as_ref().unwrap().clone();
                
                
                
                
                let middle_img = if self.panning_data.zoom_level <= 0 {
                    // if zoomed out, perform the zooming then the panning, otherwise, do panning then zooming
                    let zoomed_img = zoom_img(start_img, self.panning_data); // zoom image
                    let panned_img = pan_img(&zoomed_img, self.panning_data); // pan image
                    panned_img
                } else {
                    let panned_img = pan_img(&start_img, self.panning_data); // pan image
                    let zoomed_img = zoom_img(panned_img, self.panning_data); // zoom image
                    zoomed_img
                };

                println!("Zoom and pan time: {:?}", start_time.elapsed());
                let start_time = std::time::Instant::now();


                let original_img_size = Size{ width: middle_img.get_width(), height: middle_img.get_height(), };
                let new_img_size = calculate_max_buffer_size(window_width, window_height, original_img_size);
                println!("Calc max buffer time: {:?}", start_time.elapsed());
                let start_time = std::time::Instant::now();

                let resized_img = resize(&middle_img, new_img_size.width, new_img_size.height, SamplingFilter::Nearest); // resize image
                println!("Resize time: {:?}", start_time.elapsed());
                let start_time = std::time::Instant::now();

                let padded_img = pad_img(resized_img, window_size); // pad image
                println!("Padding time: {:?}", start_time.elapsed());
                let start_time = std::time::Instant::now();

                let mut buffer = surface.buffer_mut().unwrap();
                let raw_pixel_vec = padded_img.get_raw_pixels();
                let raw_pixel_slice = raw_pixel_vec.as_slice();
                let casted_pixel_slice = cast_slice::<u8, u32>(raw_pixel_slice);
                buffer.copy_from_slice(casted_pixel_slice);
                
                window_ref.pre_present_notify();
                buffer.present().unwrap();
                println!("Buffer copy and present time: {:?}", start_time.elapsed());


            }
            _ => (),
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
