
use std::env;
use std::num::NonZeroU32;
use std::ops::{Div, Mul};
use std::sync::Mutex;
use bytemuck::cast_slice;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::windows::{BackdropType, WindowAttributesExtWindows};
use winit::window::{Window, WindowId, WindowButtons};
use softbuffer;
use photon_rs;
use photon_rs::{PhotonImage, Rgba};
use photon_rs::transform::{crop, padding_bottom, padding_left, padding_right, padding_top, resize, SamplingFilter};
use softbuffer::{Buffer, Context, Surface};
use winit::raw_window_handle::{DisplayHandle, HasDisplayHandle};
use fraction::{Fraction, Integer, ToPrimitive};
use image::GrayAlphaImage;
use winit::keyboard::KeyCode::Convert;
use winit::keyboard::NamedKey::Print;

#[derive(Default)]
struct App {
    window: Option<Box<Window>>,
    // pixels: Option<Pixels<'static>>,
    img: Option<PhotonImage>,
}
#[derive(Copy, Clone, Default, Debug)]
struct Size {
    width: u32,
    height: u32,
}

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

fn padding_generator_bc_rust_retarded_idfk() -> Rgba {
    Rgba::new(0_u8, 0_u8, 0_u8, 0_u8) // transparent padding
    // Rgba::new(255_u8, 0_u8, 0_u8, 255_u8) // red padding (debug)
}
fn pad_img(img: PhotonImage, old_size: Size, new_size: Size) -> PhotonImage {
    let total_pad_vertical = new_size.height - old_size.height;
    let total_pad_horizontal = new_size.width - old_size.width;
    
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
    
    
    let new_img = 
        padding_left(
            &padding_right(
                &padding_bottom(
                    &padding_top(
                        &img,
                        pad_top, padding_generator_bc_rust_retarded_idfk()),
                    pad_bottom, padding_generator_bc_rust_retarded_idfk()),
                pad_right, padding_generator_bc_rust_retarded_idfk()),
            pad_left, padding_generator_bc_rust_retarded_idfk());
    new_img
}

impl ApplicationHandler for App {
    // init function
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // get args
        let args: Vec<String> = env::args().collect();
        let image_path = &args[1];
        dbg!(image_path);
        // loading image -- load image with image crate
        let mut img = photon_rs::native::open_image(image_path).expect("failed to load image");
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
        
        // FREAKY FIX
        // SAFETY: We are extending the lifetime of the window reference to 'static because both window and pixels are owned by App and dropped together.
        // let window_ptr: &'static Window = unsafe { &*(window.as_ref() as *const Window) };
        
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

            WindowEvent::RedrawRequested => {
                // setup
                let display = window_ref.display_handle().unwrap();
                let context = Context::new(display).unwrap();
                let mut surface = Surface::new(&context,window_ref)
                    .expect("error in surface definition");
                
                
                // define widths and heights
                let window_width = window_ref.inner_size().width;
                let window_height = window_ref.inner_size().height;
                let window_size = Size {
                    width: window_width, 
                    height: window_height,
                };
                
                let original_img_size = Size{
                    width: self.img.as_ref().unwrap().get_width(),
                    height: self.img.as_ref().unwrap().get_height(),
                };
                let new_img_size = calculate_max_buffer_size(window_width, window_height, original_img_size.clone());
                
                // resize everything
                surface.resize(NonZeroU32::try_from(window_width).unwrap(), NonZeroU32::try_from(window_height).unwrap()).unwrap();
                let resized_img = resize(&self.img.as_ref().unwrap(), new_img_size.width, new_img_size.height, SamplingFilter::Nearest);
                let padded_img = pad_img(resized_img, new_img_size, window_size);
                
                let mut buffer = surface.buffer_mut().unwrap();
                let raw_pixel_vec = padded_img.get_raw_pixels();
                let raw_pixel_slice = raw_pixel_vec.as_slice();
                let casted_pixel_slice = cast_slice::<u8, u32>(raw_pixel_slice);
                // dbg!(casted_pixel_slice); // FUCKING CRASHES DON'T UNCOMMENT
                // dbg!(casted_pixel_slice.len());
                buffer.copy_from_slice(casted_pixel_slice);

                window_ref.pre_present_notify();
                buffer.present().unwrap()
                
            }
            _ => (),
        }
    }
}



fn main() {
    // check if valid args before anything else
    // hehe turbofish     ::<>
    if env::args().collect::<Vec<_>>().len() != 2 {
        eprintln!("Usage: luminix <image_path>");
        return;
    };

    // ControlFlow::Wait 
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("wqadas");
}
