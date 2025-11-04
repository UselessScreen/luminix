use crate::errors::{CommandExecutionError, RunActionError};
use crate::register_file_association::register_file_association;
use derivative::Derivative;
use egui::{self, hex_color, Align, Context, InputState, Key, KeyboardShortcut, Layout, ModifierNames, PointerButton, RichText, Separator, Style, Ui, Vec2, ViewportBuilder};
use egui_extras::{Column, TableBuilder};
use egui_keybind::{Bind, Keybind};
use egui_winit::State;
use serde::{Deserialize, Serialize};
use std::any::TypeId;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::ops::{Index, IndexMut, Range};
use std::{array, env, fmt};
use strum::{EnumCount, EnumIter, EnumMessage, IntoEnumIterator};
use wgpu::{self, Adapter, Device, Instance, Queue, Surface, SurfaceConfiguration};
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::KeyCode;
use winit::platform::windows::{IconExtWindows, WindowExtWindows};
use winit::window::Icon;


pub struct SettingsWindow {
    pub state: State,
    pub configurable_settings: ConfigurableSettings,
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

const ACTION_AMOUNT: usize = 2;
#[derive(Serialize, Deserialize)]
pub struct ConfigurableSettings {
    pub keys: Keys,
    pub actions: [Action; ACTION_AMOUNT],
}

#[derive(Serialize, Deserialize, Default, Clone, PartialEq,Debug, EnumIter)]
pub enum Action {
    #[default]
    None,
    Command(ShellCommand),
}
impl Display for Action {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Action::Command(_) => {write!(f, "Command")}
            Action::None => {write!(f, "None")}
        }
    }
}
impl Action {
    pub fn run_action(&self) -> Result<(), RunActionError> {
        match &self {
            Action::None => {Ok(())}
            Action::Command(shell_command) => {
                shell_command.execute().map_err(RunActionError::from)
            }
        }
    }
}
#[derive(Serialize, Deserialize, Default, Debug, Derivative)]
#[derivative(PartialEq, Clone)]
pub struct ShellCommand(
    String,
    #[serde(skip)]
    #[derivative(PartialEq="ignore",Clone(clone_with="clone_none"))]
    Option<CommandExecutionError>
);
impl egui::TextBuffer for ShellCommand {
    fn is_mutable(&self) -> bool { true }
    fn as_str(&self) -> &str { self.0.as_str() }
    fn insert_text(&mut self, text: &str, char_index: usize) -> usize { self.0.insert_text(text, char_index) }
    fn delete_char_range(&mut self, char_range: Range<usize>) { self.0.delete_char_range(char_range) }
    fn type_id(&self) -> TypeId { TypeId::of::<Self>() }
}
impl Display for ShellCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}
impl ShellCommand {
    fn execute(&self) -> Result<(), CommandExecutionError>{
        let image_path = env::args().nth(1).expect("THIS IS A BUG! Cannot access 2nd program argument, which is checked for validity at the start of the program.");
        let commmand_with_replaced_placeholder = self.0.replace("%1", &format!("\"{image_path}\""));
        let mut split_command = shell_words::split(&commmand_with_replaced_placeholder)?.into_iter();
        dbg!(split_command.clone());
        let executable = split_command.nth(0).ok_or(CommandExecutionError::InvalidArgs)?;
        std::process::Command::new(executable)
            .args(split_command)
            .spawn()?;
        Ok(())
    }
}
fn clone_none<T>(_: &Option<T>) -> Option<T> {
    None
}

#[derive(Clone, Serialize, Deserialize, EnumIter, EnumCount, EnumMessage)]
#[allow(non_camel_case_types)]
enum KeysValue {
    #[strum(message="Open settings")]
    settings,
    #[strum(message="Pause gif")]
    pause,
    #[strum(message="Next frame")]
    next_frame,
    #[strum(message="Previous frame")]
    prev_frame,
    #[strum(message="Actions")]
    actions(usize),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Keys {
    pub settings: KeyWrapper,
    pub pause: KeyWrapper,
    pub next_frame: KeyWrapper,
    pub prev_frame: KeyWrapper,
    pub actions: [KeyWrapper; ACTION_AMOUNT],
}
impl Index<KeysValue> for Keys {
    type Output = KeyWrapper;
    fn index(&self, index: KeysValue) -> &Self::Output {
        match index {
            KeysValue::settings => &self.settings,
            KeysValue::pause => &self.pause,
            KeysValue::next_frame => &self.next_frame,
            KeysValue::prev_frame => &self.prev_frame,
            KeysValue::actions(i) => &self.actions[i],
        }
    }
}
impl IndexMut<KeysValue> for Keys {
    fn index_mut(&mut self, index: KeysValue) -> &mut Self::Output {
        match index {
            KeysValue::settings => &mut self.settings,
            KeysValue::pause => &mut self.pause,
            KeysValue::next_frame => &mut self.next_frame,
            KeysValue::prev_frame => &mut self.prev_frame,
            KeysValue::actions(i) => &mut self.actions[i],
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct KeyWrapper {
    key_code: Option<KeyCode>
}
impl KeyWrapper {
    pub fn get_keycode(&self) -> Option<KeyCode> {
        self.key_code
    }
    pub fn new(key_code: KeyCode) -> KeyWrapper {
        KeyWrapper {key_code: Some(key_code)}
    }
    pub fn new_empty() -> KeyWrapper {
        KeyWrapper {key_code: None}
    }
}
impl Bind for KeyWrapper {
    fn set(&mut self, keyboard: Option<KeyboardShortcut>, _pointer: Option<PointerButton>) {
        if let Some(keyboard) = keyboard {
            *self = KeyWrapper{key_code: Some(egui_key_to_winit(keyboard.logical_key))};
        }
    }

    fn format(&self, _names: &ModifierNames<'_>, _is_mac: bool) -> String {
        match self.key_code {
            None => String::from("None"),
            Some(key) => {format!("{key:?}")}
        }
    }

    fn pressed(&self, input: &mut InputState) -> bool {
        match self.key_code {
            None => false,
            Some(key) => input.key_pressed(winit_keycode_to_egui(key)),
        }
    }
}
impl Default for ConfigurableSettings {
    fn default() -> Self {
        ConfigurableSettings {
            keys: Keys {
                settings: KeyWrapper::new(KeyCode::KeyK),
                pause: KeyWrapper::new(KeyCode::Space),
                next_frame: KeyWrapper::new(KeyCode::Period),
                prev_frame: KeyWrapper::new(KeyCode::Comma),
                actions: array::from_fn(|_| KeyWrapper::new_empty()),
            },
            actions: array::from_fn(|_| Action::default())
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
            configurable_settings: Self::load_settings(),
        };
        
        // Initialize WGPU
        pollster::block_on(settings_window.initialize_wgpu());
        // settings_window.window.set_visible(true);
        settings_window
    }
    // idk chatgpt wrote this part bc i tried and failed if anyone even sees this and wants to fix it please do i'm too scared to even look at it
    async fn initialize_wgpu(&mut self) {
        if self.instance.is_none() {
            return;
        }
        
        let instance = self.instance.as_ref().unwrap();
        
        // Create surface with proper lifetime handling
        // no idea how to do this safely
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
                // when ran in dev profile, enable debug menu
                #[cfg(debug_assertions)]
                {
                    ui.style_mut().debug = egui::style::DebugOptions::default();
                }
                ui.style_mut().visuals.faint_bg_color = hex_color!("#282828"); // change table background
                ui.style_mut().override_text_valign = Some(Align::Center);

                // Keybinds
                ui.group(|ui| {
                    egui::CollapsingHeader::new(RichText::new("Keybinds").heading())
                        .default_open(true)
                        .show_unindented(ui, |ui| {
                            ui.add(Separator::default().grow(6.0));
                            self.keybind_table(ui);
                        });
                });
                ui.add_space(5.0);
                // Actions
                ui.group(|ui| {
                    egui::CollapsingHeader::new(RichText::new("Actions").heading())
                        .default_open(true)
                        .show_unindented(ui, |ui| {
                            ui.add(Separator::default().grow(6.0));
                            self.action_table(ui);
                        });
                });
                
                ui.add_space(10.0);
                
                ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
                    if ui.button("Apply").clicked() {
                        self.save_settings();
                    }
                });
                
                // TODO: add linux & macos file association support
                #[cfg(target_os = "windows")]
                ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                    if ui.button("Register File association").clicked() {
                        register_file_association().expect("Error registering file association");
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
    
    fn action_table(&mut self, ui: &mut Ui) {
        TableBuilder::new(ui)
            .column(Column::remainder())
            .column(Column::remainder())
            .striped(true)
            .id_salt("actions_table")
            .cell_layout(Layout::default().with_cross_align(Align::LEFT).with_main_justify(true))
            .body(|body| {
                // Action 1
                let row_heights: Vec<f32> = self.configurable_settings.actions.iter().map(|action| {
                   match action {
                       Action::Command(command) => {
                           40.0 + match command.1 {
                               None => 0.0,
                               Some(_) => 20.0,
                           }
                       }
                       Action::None => {20.0}
                   }
                }).collect();
                body.heterogeneous_rows(row_heights.into_iter(), |mut row| {
                    let row_index = row.index();
                    // Label column
                    row.col(|ui| {
                        let label = format!("Action {}", row_index + 1);
                       ui.label(label);
                    });
                    
                    // Setting Column
                    row.col(|ui| {
                        ui.with_layout(Layout::top_down(Align::LEFT).with_main_align(Align::Center).with_main_justify(true), |ui| {
                            // action selection
                            ui.with_layout(Layout::top_down_justified(Align::LEFT),|ui| {
                                ui.set_height(ui.style().spacing.interact_size.y);
                                egui::ComboBox::from_id_salt(format!("action settings index {row_index}"))
                                    .selected_text(self.configurable_settings.actions[row_index].to_string())
                                    .show_ui(ui, |ui| {
                                        for action in Action::iter() {
                                            ui.selectable_value(&mut self.configurable_settings.actions[row_index], action.clone(), action.to_string());
                                        }
                                    });
                            });
                            
                            // if Command
                            let action = &mut self.configurable_settings.actions[row_index];
                            if let Action::Command(command) = action {
                                // help tooltip
                                let default_style = Style::default();
                                let mut layout_job = egui::text::LayoutJob::default();
                                RichText::new("Use ")
                                    .append_to(&mut layout_job, &default_style, egui::FontSelection::default(), Align::LEFT);
                                RichText::new("%1")
                                    .code()
                                    .append_to(&mut layout_job, &default_style, egui::FontSelection::default(), Align::LEFT);
                                RichText::new(" as placeholder for image path in command.")
                                    .append_to(&mut layout_job, &default_style, egui::FontSelection::default(), Align::LEFT);
                                // command selection menu
                                ui.with_layout(Layout::left_to_right(Align::TOP), |ui| {
                                    // actual textedit
                                    egui::TextEdit::singleline(command).code_editor().show(ui).response.on_hover_text(layout_job);
                                    let test_button = ui.button("Test command");
                                    if test_button.clicked() {
                                        if let Err(error) = command.execute() {
                                            dbg!(&error);
                                            command.1 = Some(error);
                                        } else {
                                            command.1 = None;
                                        }
                                    }
                                });
                                if let Some(error_message) = &command.1 {
                                    ui.label(error_message.to_string());
                                }
                            }
                        });
                    });
                });
            });
    }

    fn keybind_table(&mut self, ui: &mut Ui) {
        TableBuilder::new(ui)
            .column(Column::remainder())
            .column(Column::remainder())
            .striped(true)
            .id_salt("keys")
            .cell_layout(Layout::default().with_cross_align(Align::LEFT).with_main_justify(true))
            .body(|body| {
                let row_amount = (KeysValue::COUNT - 1) + (self.configurable_settings.keys.actions.len());
                body.rows(20.0, row_amount, |mut row| {
                    let row_index = row.index();
                    let keys_index = match row_index {
                        0 => KeysValue::settings,
                        1 => KeysValue::pause,
                        2 => KeysValue::next_frame,
                        3 => KeysValue::prev_frame,
                        _ => KeysValue::actions(row_index-4)
                    };
                    let row_label = if let KeysValue::actions(action_index) = keys_index {
                        format!("Action {}", action_index+1)
                    } else {
                        String::from(keys_index.get_message().unwrap())
                    };
                    // label row
                    row.col(|ui| {
                        ui.label(&row_label);
                    });
                    // keybind row
                    row.col(|ui| {
                        ui.add(Keybind::new(&mut self.configurable_settings.keys[keys_index], row_label).with_reset(KeyWrapper::new_empty()).with_reset_key(Some(Key::Escape)));
                    });
                });
            });
    }

    pub fn get_settings(&self) -> &ConfigurableSettings {
        &self.configurable_settings
    }
    
    fn save_settings(&self) {

        let binding = env::current_exe().unwrap().parent().unwrap().join("luminix-settings.ron");
        let input_path = binding.as_path();
        
        let f = File::options()
            .create(true)
            .truncate(true)
            .write(true)
            .open(input_path)
            .expect("Failed opening file for writing settings");
        
        ron::Options::default()
            .to_io_writer_pretty(f, &self.configurable_settings, ron::ser::PrettyConfig::new().compact_arrays(true))
            .expect("Failed to write to file");
    }
        
    fn load_settings() -> ConfigurableSettings {

        let binding = env::current_exe().unwrap().parent().unwrap().join("luminix-settings.ron");
        let input_path = binding.as_path();
        let f = File::open(input_path);
        
        if f.is_err() {
            eprintln!("Failed to load luminix-settings.ron, falling back to default configuration values. Error message: {}", f.unwrap_err());
            return ConfigurableSettings::default()
        }
        
        // return
        ron::de::from_reader(f.unwrap()).unwrap_or_else(|e| {
            eprintln!("Failed to load luminix-settings.ron, falling back to default configuration values. Error message: {e}");
            ConfigurableSettings::default()
        })
    }

    pub fn show(&self) {
        println!("opening settings window");
        self.window.set_visible(true);
        self.window.focus_window();
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
fn egui_key_to_winit(key: Key) -> KeyCode {
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