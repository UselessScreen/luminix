use egui::RichText;
use egui::{self, hex_color, Align, Context, InputState, Key, KeyboardShortcut, Layout, ModifierNames, PointerButton, Separator, Ui, Vec2, ViewportBuilder};
use egui_extras::{Column, TableBuilder};
use egui_keybind::{Bind, Keybind};
use egui_winit::State;
use wgpu::{self, Adapter, Device, Instance, Queue, Surface, SurfaceConfiguration};
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::KeyCode;
use winit::platform::windows::{IconExtWindows, WindowExtWindows};
use winit::window::Icon;

pub struct SettingsWindow {
    pub state: State,
    configurable_settings: ConfigurableSettings,
    pub ctx: Context,
    pub window: winit::window::Window,
    // WGPU components
    instance: Option<Instance>,
    surface: Option<Surface<'static>>,
    adapter: Option<Adapter>,
    device: Option<Device>,
    queue: Option<Queue>,
    config: Option<SurfaceConfiguration>,
    egui_rpass: Option<egui_wgpu::Renderer>,
}

#[derive(Clone)]
pub struct Keys {
    pub settings: KeyWrapper,
    pub pause: KeyWrapper,
    pub next_frame: KeyWrapper,
    pub prev_frame: KeyWrapper,
}

#[derive(Clone)]
pub struct ConfigurableSettings {
    pub keys: Keys
}

#[derive(Clone)]
pub struct KeyWrapper {
    key_code: KeyCode
}
impl KeyWrapper {
    pub fn get_keycode(&self) -> KeyCode {
        self.key_code
    }
    
}


impl Bind for KeyWrapper {
    fn set(&mut self, keyboard: Option<KeyboardShortcut>, _pointer: Option<PointerButton>) {
        if let Some(keyboard) = keyboard {
            *self = KeyWrapper{key_code: egui_key_to_winit(keyboard.logical_key)};
            
        }
    }

    fn format(&self, _names: &ModifierNames<'_>, _is_mac: bool) -> String {
        format!("{:?}", self.key_code)
    }

    fn pressed(&self, input: &mut InputState) -> bool {
        let egui_keycode = winit_keycode_to_egui(self.key_code);
        input.key_pressed(egui_keycode)
    }
}

impl Default for ConfigurableSettings {
    fn default() -> Self {
        ConfigurableSettings {
            keys: Keys {
                settings: KeyWrapper { key_code: KeyCode::KeyK },
                pause: KeyWrapper { key_code: KeyCode::Space },
                next_frame: KeyWrapper { key_code: KeyCode::Period },
                prev_frame: KeyWrapper { key_code: KeyCode::Comma },
            }
        }
    }
}

impl SettingsWindow {
    pub fn new(event_loop: &ActiveEventLoop) -> Self {
        let ctx = Context::default();
        
        let viewport_builder = ViewportBuilder::default().with_title("Luminix Settings").with_active(false).with_visible(false).with_min_inner_size(Vec2::new(256_f32, 226_f32)); // .with_icon(Icon::from_resource(1, Some(PhysicalSize::new(128, 128))).ok())
        let window = egui_winit::create_window(&ctx, event_loop, &viewport_builder).expect("Error creating settings window");
        window.set_window_icon(Icon::from_resource(1, Some(PhysicalSize::new(128, 128))).ok());
        let egui_bg_color = Some(winit::platform::windows::Color::from_rgb(0x1b, 0x1b, 0x1b));
        window.set_border_color(egui_bg_color);
        window.set_title_background_color(egui_bg_color);
        let state = State::new(
            ctx.clone(),
            ctx.viewport_id(),
            &window,               // window implements HasDisplayHandle
            None,                  // native_pixels_per_point: None means use system default
            None,                  // theme: None means use system default
            None                   // max_texture_side: None means use egui default
        );
        
        let instance_descriptor = wgpu::InstanceDescriptor  { backends: wgpu::Backends::all(), ..Default::default() };
        let instance = Some(Instance::new(&instance_descriptor));
        
        let mut settings_window = Self {
            ctx,
            window,
            state,
            instance,
            surface: None,
            adapter: None,
            device: None,
            queue: None,
            config: None,
            egui_rpass: None,
            configurable_settings: ConfigurableSettings::default(),
        };
        
        // Initialize WGPU
        pollster::block_on(settings_window.initialize_wgpu());
        settings_window.window.set_visible(true);
        settings_window
    }
    
    async fn initialize_wgpu(&mut self) {
        if self.instance.is_none() {
            return;
        }
        
        let instance = self.instance.as_ref().unwrap();
        
        // Create surface with proper lifetime handling
        let surface = unsafe { 
            let surface = instance.create_surface(&self.window).expect("Failed to create surface");
            // SAFETY: We're extending the lifetime to 'static because we know the window
            // will live as long as the SettingsWindow struct exists
            std::mem::transmute::<Surface<'_>, Surface<'static>>(surface)
        };
        
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).await.expect("Failed to find an appropriate adapter");
        
        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            },
        ).await.expect("Failed to create device");
        
        let size = self.window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps.formats.iter().find(|f| f.is_srgb())
            .unwrap_or(&surface_caps.formats[0]);
        
        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: *format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        
        let egui_rpass = egui_wgpu::Renderer::new(
            &device, 
            *format,
            None, 
            1,
            false
        );
        
        self.surface = Some(surface);
        self.adapter = Some(adapter);
        self.device = Some(device);
        self.queue = Some(queue);
        self.config = Some(config);
        self.egui_rpass = Some(egui_rpass);
    }
    
    pub fn on_window_event(&mut self, event: &WindowEvent) -> egui_winit::EventResponse {
        if let WindowEvent::Resized(size) = event {
            if let (Some(surface), Some(device), Some(config)) = (&mut self.surface, &self.device, &mut self.config) {
                if size.width > 0 && size.height > 0 {
                    config.width = size.width;
                    config.height = size.height;
                    surface.configure(device, config);
                }
            }
        }
        if let WindowEvent::CloseRequested = event {
            self.window.set_visible(false);
        }
        self.state.on_window_event(&self.window, event)
    }
    
    pub fn on_redraw(&mut self) {
        if self.window.inner_size().width == 0 || self.window.inner_size().height == 0 {
            // println!("size is zero");
            return;
        }
        let ctx = self.ctx.clone();
        

        // Begin frame and create UI
        let input = self.state.take_egui_input(&self.window);
        let output = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.style_mut().visuals.faint_bg_color = hex_color!("#282828"); // change table background
                ui.style_mut().override_text_valign = Some(Align::Center);

                
                ui.group(|ui| {
                    egui::CollapsingHeader::new(RichText::new("Keybinds").heading())
                        .default_open(true)
                        .show_unindented(ui, |ui| {
                            ui.add(Separator::default().grow(6.0));
                            self.keybind_table(ui);
                        });
                });
                
                // settings_painter.rect_filled(rect, 10, Color32::from_gray(20));
                
                // ui.add_space(10.0);
                // ui.separator();
                ui.add_space(10.0);
                
                ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
                    if ui.button("Apply").clicked() {
                        // TODO: Apply the settings to the image
                        // println!("Applied settings: brightness={}, contrast={}", self.brightness, self.contrast);
                    }
                });
            });
        });
        
        // Handle platform output (clipboard, cursor, etc.)
        self.state.handle_platform_output(&self.window, output.platform_output);
        
        // Tessellate the egui UI
        let tesselated = ctx.tessellate(output.shapes, output.pixels_per_point);

        // Check if we have all necessary WGPU components
        if let (Some(device), Some(queue), Some(surface), Some(egui_rpass)) = 
            (&self.device, &self.queue, &mut self.surface, &mut self.egui_rpass) {
            
            let frame = match surface.get_current_texture() {
                Ok(frame) => frame,
                Err(e) => {
                    eprintln!("Failed to acquire next swap chain texture: {e:?}");
                    return;
                }
            };
            
            let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
            
            // Process the textures delta
            for (id, image_delta) in &output.textures_delta.set {
                egui_rpass.update_texture(device, queue, *id, image_delta);
            }
            

            let screen_descriptor = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [self.window.inner_size().width, self.window.inner_size().height],
                pixels_per_point: output.pixels_per_point,
            };
            
            // Create encoder, update buffers and render
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("egui_encoder"),
            });
            
            // Update buffers
            egui_rpass.update_buffers(device, queue, &mut encoder, &tesselated, &screen_descriptor);
            
            // Begin render pass with lifetime workaround
            {
                let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui_render"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.1,
                                g: 0.1,
                                b: 0.1,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                

                egui_rpass.render(&mut render_pass.forget_lifetime(), &tesselated, &screen_descriptor);
                
            }
            
            queue.submit([encoder.finish()]);
            frame.present();
            
            // Free deleted textures
            for id in &output.textures_delta.free {
                egui_rpass.free_texture(id);
            }
        }
    }

    fn keybind_table(&mut self, ui: &mut Ui) {
        TableBuilder::new(ui)
            .column(Column::remainder())
            .column(Column::remainder())
            .striped(true)
            .id_salt("keys")
            .cell_layout(Layout::default().with_cross_align(Align::LEFT).with_main_justify(true))
            .body(|mut body| {
                // settings
                body.row(20.0, |mut row| {
                    row.col(|ui| {
                        ui.label("Settings");
                    });

                    row.col(|ui| {
                        ui.add(Keybind::new(&mut self.configurable_settings.keys.settings, "settings_key").with_reset(KeyWrapper{key_code: KeyCode::KeyK}).with_reset_key(Some(Key::Escape)));
                    });
                });
                // pause
                body.row(20.0, |mut row| {
                    row.col(|ui| {
                        ui.label("Pause gif");
                    });

                    row.col(|ui| {
                        ui.add(Keybind::new(&mut self.configurable_settings.keys.pause, "pause_key").with_reset(KeyWrapper{key_code: KeyCode::Space}).with_reset_key(Some(Key::Escape)));
                    });
                });
                // forward frame
                body.row(20.0, |mut row| {
                    row.col(|ui| {
                        ui.label("Gif next frame");
                    });

                    row.col(|ui| {
                        ui.add(Keybind::new(&mut self.configurable_settings.keys.next_frame, "fw_key").with_reset(KeyWrapper{key_code: KeyCode::Period}).with_reset_key(Some(Key::Escape)));
                    });
                });
                // back frame
                body.row(20.0, |mut row| {
                    row.col(|ui| {
                        ui.label("Gif previous frame");
                    });

                    row.col(|ui| {
                        ui.add(Keybind::new(&mut self.configurable_settings.keys.prev_frame, "rw_key").with_reset(KeyWrapper{key_code: KeyCode::Comma}).with_reset_key(Some(Key::Escape)));
                    });
                });
            });
    }

    pub fn get_settings(&self) -> ConfigurableSettings {
        self.configurable_settings.clone()
    }

    pub fn show(&mut self) {
        println!("opening settings window");
        self.window.set_visible(true);
    }
}

#[allow(clippy::too_many_lines, clippy::enum_glob_use)]
fn winit_keycode_to_egui(key_code: KeyCode) -> Key {
    
    use Key::*;
    match key_code {
        KeyCode::Backquote => Backtick,
        KeyCode::Backslash | KeyCode::IntlBackslash | KeyCode::IntlRo | KeyCode::IntlYen => Backslash,
        KeyCode::BracketLeft => OpenBracket,
        KeyCode::BracketRight => CloseBracket,
        KeyCode::Comma | KeyCode::NumpadComma => Comma,
        KeyCode::Digit0 | KeyCode::Numpad0 => Num0,
        KeyCode::Digit1 | KeyCode::Numpad1 => Num1,
        KeyCode::Digit2 | KeyCode::Numpad2 => Num2,
        KeyCode::Digit3 | KeyCode::Numpad3 => Num3,
        KeyCode::Digit4 | KeyCode::Numpad4 => Num4,
        KeyCode::Digit5 | KeyCode::Numpad5 => Num5,
        KeyCode::Digit6 | KeyCode::Numpad6 => Num6,
        KeyCode::Digit7 | KeyCode::Numpad7 => Num7,
        KeyCode::Digit8 | KeyCode::Numpad8 => Num8,
        KeyCode::Digit9 | KeyCode::Numpad9 => Num9,
        KeyCode::Equal | KeyCode::NumpadEqual => Equals,
        KeyCode::KeyA => A,
        KeyCode::KeyB => B,
        KeyCode::KeyC => C,
        KeyCode::KeyD => D,
        KeyCode::KeyE => E,
        KeyCode::KeyF => F,
        KeyCode::KeyG => G,
        KeyCode::KeyH => H,
        KeyCode::KeyI => I,
        KeyCode::KeyJ => J,
        KeyCode::KeyK => K,
        KeyCode::KeyL => L,
        KeyCode::KeyM => M,
        KeyCode::KeyN => N,
        KeyCode::KeyO => O,
        KeyCode::KeyP => P,
        KeyCode::KeyQ => Q,
        KeyCode::KeyR => R,
        KeyCode::KeyS => S,
        KeyCode::KeyT => T,
        KeyCode::KeyU => U,
        KeyCode::KeyV => V,
        KeyCode::KeyW => W,
        KeyCode::KeyX => X,
        KeyCode::KeyY => Y,
        KeyCode::KeyZ => Z,
        KeyCode::Minus | KeyCode::NumpadSubtract => Minus,
        KeyCode::Period | KeyCode::NumpadDecimal => Period,
        KeyCode::Quote => Quote,
        KeyCode::Semicolon => Semicolon,
        KeyCode::Slash | KeyCode::NumpadDivide => Slash,
        KeyCode::Backspace | KeyCode::NumpadBackspace => Backspace,
        KeyCode::Enter | KeyCode::NumpadEnter => Enter,
        KeyCode::Space => Space,
        KeyCode::Tab => Tab,
        KeyCode::Delete => Delete,
        KeyCode::End => End,
        KeyCode::Home => Home ,
        KeyCode::Insert => Insert ,
        KeyCode::PageDown => PageDown ,
        KeyCode::PageUp => PageUp ,
        KeyCode::ArrowDown => ArrowDown ,
        KeyCode::ArrowLeft => ArrowLeft ,
        KeyCode::ArrowRight => ArrowRight ,
        KeyCode::ArrowUp => ArrowUp ,
        KeyCode::NumpadAdd => Plus ,
        KeyCode::Escape => Escape ,
        KeyCode::BrowserBack => BrowserBack ,
        KeyCode::Copy => Copy ,
        KeyCode::Cut => Cut ,
        KeyCode::Paste => Paste ,
        KeyCode::F1 => F1 ,
        KeyCode::F2 => F2 ,
        KeyCode::F3 => F3 ,
        KeyCode::F4 => F4 ,
        KeyCode::F5 => F5 ,
        KeyCode::F6 => F6 ,
        KeyCode::F7 => F7 ,
        KeyCode::F8 => F8 ,
        KeyCode::F9 => F9 ,
        KeyCode::F10 => F10 ,
        KeyCode::F11 => F11 ,
        KeyCode::F12 => F12 ,
        KeyCode::F13 => F13 ,
        KeyCode::F14 => F14 ,
        KeyCode::F15 => F15 ,
        KeyCode::F16 => F16 ,
        KeyCode::F17 => F17 ,
        KeyCode::F18 => F18 ,
        KeyCode::F19 => F19 ,
        KeyCode::F20 => F20 ,
        KeyCode::F21 => F21 ,
        KeyCode::F22 => F22 ,
        KeyCode::F23 => F23 ,
        KeyCode::F24 => F24 ,
        KeyCode::F25 => F25 ,
        KeyCode::F26 => F26 ,
        KeyCode::F27 => F27 ,
        KeyCode::F28 => F28 ,
        KeyCode::F29 => F29 ,
        KeyCode::F30 => F30 ,
        KeyCode::F31 => F31 ,
        KeyCode::F32 => F32 ,
        KeyCode::F33 => F33 ,
        KeyCode::F34 => F34 ,
        KeyCode::F35 => F35 ,
        _ => Exclamationmark
    }
}
#[allow(clippy::too_many_lines)]
fn egui_key_to_winit(key: Key) -> winit::keyboard::KeyCode {
    match key {
        // Arrows
        Key::ArrowDown => KeyCode::ArrowDown,
        Key::ArrowLeft => KeyCode::ArrowLeft,
        Key::ArrowRight => KeyCode::ArrowRight,
        Key::ArrowUp => KeyCode::ArrowUp,

        // Control keys
        Key::Escape => KeyCode::Escape,
        Key::Tab => KeyCode::Tab,
        Key::Backspace => KeyCode::Backspace,
        Key::Enter => KeyCode::Enter,
        Key::Space => KeyCode::Space,

        // Navigation
        Key::Insert => KeyCode::Insert,
        Key::Delete => KeyCode::Delete,
        Key::Home => KeyCode::Home,
        Key::End => KeyCode::End,
        Key::PageUp => KeyCode::PageUp,
        Key::PageDown => KeyCode::PageDown,

        // Function keys
        Key::F1 => KeyCode::F1,
        Key::F2 => KeyCode::F2,
        Key::F3 => KeyCode::F3,
        Key::F4 => KeyCode::F4,
        Key::F5 => KeyCode::F5,
        Key::F6 => KeyCode::F6,
        Key::F7 => KeyCode::F7,
        Key::F8 => KeyCode::F8,
        Key::F9 => KeyCode::F9,
        Key::F10 => KeyCode::F10,
        Key::F11 => KeyCode::F11,
        Key::F12 => KeyCode::F12,
        Key::F13 => KeyCode::F13,
        Key::F14 => KeyCode::F14,
        Key::F15 => KeyCode::F15,
        Key::F16 => KeyCode::F16,
        Key::F17 => KeyCode::F17,
        Key::F18 => KeyCode::F18,
        Key::F19 => KeyCode::F19,
        Key::F20 => KeyCode::F20,
        Key::F21 => KeyCode::F21,
        Key::F22 => KeyCode::F22,
        Key::F23 => KeyCode::F23,
        Key::F24 => KeyCode::F24,
        Key::F25 => KeyCode::F25,
        Key::F26 => KeyCode::F26,
        Key::F27 => KeyCode::F27,
        Key::F28 => KeyCode::F28,
        Key::F29 => KeyCode::F29,
        Key::F30 => KeyCode::F30,
        Key::F31 => KeyCode::F31,
        Key::F32 => KeyCode::F32,
        Key::F33 => KeyCode::F33,
        Key::F34 => KeyCode::F34,
        Key::F35 => KeyCode::F35,

        // Letters A–Z
        Key::A => KeyCode::KeyA,
        Key::B => KeyCode::KeyB,
        Key::C => KeyCode::KeyC,
        Key::D => KeyCode::KeyD,
        Key::E => KeyCode::KeyE,
        Key::F => KeyCode::KeyF,
        Key::G => KeyCode::KeyG,
        Key::H => KeyCode::KeyH,
        Key::I => KeyCode::KeyI,
        Key::J => KeyCode::KeyJ,
        Key::K => KeyCode::KeyK,
        Key::L => KeyCode::KeyL,
        Key::M => KeyCode::KeyM,
        Key::N => KeyCode::KeyN,
        Key::O => KeyCode::KeyO,
        Key::P => KeyCode::KeyP,
        Key::Q => KeyCode::KeyQ,
        Key::R => KeyCode::KeyR,
        Key::S => KeyCode::KeyS,
        Key::T => KeyCode::KeyT,
        Key::U => KeyCode::KeyU,
        Key::V => KeyCode::KeyV,
        Key::W => KeyCode::KeyW,
        Key::X => KeyCode::KeyX,
        Key::Y => KeyCode::KeyY,
        Key::Z => KeyCode::KeyZ,

        Key::Copy => KeyCode::Copy,
        Key::Cut => KeyCode::Cut,
        Key::Paste => KeyCode::Paste,
        Key::Colon | Key::Semicolon => KeyCode::Semicolon,
        Key::Comma => KeyCode::Comma,
        Key::Backslash | Key::Pipe => KeyCode::Backslash,
        Key::Slash | Key::Questionmark => KeyCode::Slash,
        Key::Exclamationmark => KeyCode::Digit1,
        Key::OpenBracket | Key::OpenCurlyBracket => KeyCode::BracketLeft,
        Key::CloseBracket | Key::CloseCurlyBracket => KeyCode::BracketRight,
        Key::Backtick => KeyCode::Backquote,
        Key::Minus => KeyCode::Minus,
        Key::Period => KeyCode::Period,
        Key::Plus | Key::Equals => KeyCode::Equal,
        Key::Quote => KeyCode::Quote,
        Key::Num0 => KeyCode::Numpad0,
        Key::Num1 => KeyCode::Numpad1,
        Key::Num2 => KeyCode::Numpad2,
        Key::Num3 => KeyCode::Numpad3,
        Key::Num4 => KeyCode::Numpad4,
        Key::Num5 => KeyCode::Numpad5,
        Key::Num6 => KeyCode::Numpad6,
        Key::Num7 => KeyCode::Numpad7,
        Key::Num8 => KeyCode::Numpad8,
        Key::Num9 => KeyCode::Numpad9,
        Key::BrowserBack => KeyCode::BrowserBack
    }
}