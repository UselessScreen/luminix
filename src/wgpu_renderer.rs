use std::sync::Arc;
use wgpu::util::DeviceExt;
use wgpu::wgt::Dx12SwapchainKind;
use winit::dpi::PhysicalPosition;
use winit::window::Window;

pub struct WgpuRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,

    // Texture resources - must be kept alive
    _texture: Option<wgpu::Texture>,
    _texture_view: Option<wgpu::TextureView>,
    _sampler: Option<wgpu::Sampler>,
    texture_bind_group: Option<wgpu::BindGroup>,

    uniform_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,

    // Transform state
    pub pan_offset: PhysicalPosition<f32>,
    pub zoom_level: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

const VERTICES: &[Vertex] = &[
    Vertex { position: [-1.0, 1.0, 0.0], tex_coords: [0.0, 0.0] },   // top-left
    Vertex { position: [1.0, 1.0, 0.0], tex_coords: [1.0, 0.0] },    // top-right
    Vertex { position: [1.0, -1.0, 0.0], tex_coords: [1.0, 1.0] },   // bottom-right
    Vertex { position: [-1.0, -1.0, 0.0], tex_coords: [0.0, 1.0] },  // bottom-left
];

const INDICES: &[u16] = &[
    0, 1, 2,
    2, 3, 0,
];

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    image_aspect: f32,
    window_aspect: f32,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    _padding: [f32; 3],
}

impl WgpuRenderer {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        
        // Use DX12 on Windows for transparency support
        let backends = if cfg!(target_os = "windows") {
            wgpu::Backends::DX12
        } else {
            wgpu::Backends::PRIMARY
        };
        
        // Configure DX12 to use DxgiFromVisual for transparency on Windows
        let mut backend_options = wgpu::BackendOptions::default();
        #[cfg(target_os = "windows")]
        {
            backend_options.dx12.presentation_system = Dx12SwapchainKind::DxgiFromVisual;
        }
        
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            backend_options,
            ..Default::default()
        });

        let surface = instance.create_surface(window).unwrap();

                let adapter = instance.request_adapter(
                    &wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::HighPerformance,
                        compatible_surface: Some(&surface),
                        force_fallback_adapter: false,
                    },
        ).await.unwrap();

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                label: None,
                memory_hints: Default::default(),
                trace: Default::default(),
                experimental_features: Default::default(),
            },
        ).await.unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        
        let surface_format = surface_caps.formats.iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        
        // Use PreMultiplied alpha mode for transparency (enabled via DxgiFromVisual on Windows)
        let alpha_mode = if surface_caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else if surface_caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PostMultiplied) {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else {
            surface_caps.alpha_modes[0]
        };

        // Use Immediate present mode for better resize performance on Windows
        // This allows rendering during resize operations
        let present_mode = if surface_caps.present_modes.contains(&wgpu::PresentMode::Immediate) {
            wgpu::PresentMode::Immediate
        } else if surface_caps.present_modes.contains(&wgpu::PresentMode::AutoNoVsync) {
            wgpu::PresentMode::AutoNoVsync
        } else {
            surface_caps.present_modes[0]
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode,
            alpha_mode,
            view_formats: vec![surface_format.add_srgb_suffix()],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        // Create shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        // Create texture bind group layout
                let texture_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                    label: Some("texture_bind_group_layout"),
                });

                // Create uniform bind group layout
                let uniform_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                    label: Some("uniform_bind_group_layout"),
                });

                // Create uniform buffer
                let uniforms = Uniforms {
                    image_aspect: 1.0,
                    window_aspect: size.width as f32 / size.height as f32,
                    zoom: 1.0,
                    pan_x: 0.0,
                    pan_y: 0.0,
                    _padding: [0.0; 3],
                };

                let uniform_buffer = device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("Uniform Buffer"),
                        contents: bytemuck::cast_slice(&[uniforms]),
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    }
                );

                let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &uniform_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform_buffer.as_entire_binding(),
                        },
                    ],
                    label: Some("uniform_bind_group"),
                });

                // Create render pipeline
                let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Render Pipeline Layout"),
                    bind_group_layouts: &[&texture_bind_group_layout, &uniform_bind_group_layout],
                    push_constant_ranges: &[],
                });

                let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Render Pipeline"),
                    layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: config.format,
                            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None, // Disable culling - we're rendering a 2D quad
                        polygon_mode: wgpu::PolygonMode::Fill,
                        unclipped_depth: false,
                        conservative: false,
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState {
                        count: 1,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview: None,
                    cache: None,
                });

                let vertex_buffer = device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("Vertex Buffer"),
                        contents: bytemuck::cast_slice(VERTICES),
                        usage: wgpu::BufferUsages::VERTEX,
                    }
                );

                let index_buffer = device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("Index Buffer"),
                        contents: bytemuck::cast_slice(INDICES),
                        usage: wgpu::BufferUsages::INDEX,
                    }
                );

        let num_indices = INDICES.len() as u32;

        Self {
                    surface,
                    device,
                    queue,
                    config,
                    render_pipeline,
                    vertex_buffer,
                    index_buffer,
                    num_indices,
                    _texture: None,
                    _texture_view: None,
                    _sampler: None,
                    texture_bind_group: None,
                    uniform_bind_group,
                    uniform_buffer,
                    pan_offset: PhysicalPosition::new(0.0, 0.0),
                    zoom_level: 1.0,
                }
            }

            pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
                if new_size.width > 0 && new_size.height > 0 {
                    self.config.width = new_size.width;
                    self.config.height = new_size.height;
                    self.surface.configure(&self.device, &self.config);
                }
            }

    pub fn load_texture(&mut self, image_data: &[u8], width: u32, height: u32) {
        // Convert RGBA to BGRA with pre-multiplied alpha for transparency
        let mut bgra_data = Vec::with_capacity(image_data.len());
        for chunk in image_data.chunks_exact(4) {
            // ...existing code...
                    let r = chunk[0] as f32 / 255.0;
                    let g = chunk[1] as f32 / 255.0;
                    let b = chunk[2] as f32 / 255.0;
                    let a = chunk[3] as f32 / 255.0;

                    // Pre-multiply RGB by alpha
                    let r_pre = (r * a * 255.0) as u8;
                    let g_pre = (g * a * 255.0) as u8;
                    let b_pre = (b * a * 255.0) as u8;
                    let a_byte = (a * 255.0) as u8;

                    // BGRA format
                    bgra_data.push(b_pre); // B
                    bgra_data.push(g_pre); // G
                    bgra_data.push(r_pre); // R
            bgra_data.push(a_byte); // A
        }

        let texture_size = wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                };

                let texture = self.device.create_texture(
                    &wgpu::TextureDescriptor {
                        size: texture_size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Bgra8UnormSrgb,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        label: Some("image_texture"),
                        view_formats: &[],
                    }
                );

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &bgra_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            texture_size,
        );

                let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                    address_mode_u: wgpu::AddressMode::ClampToEdge,
                    address_mode_v: wgpu::AddressMode::ClampToEdge,
                    address_mode_w: wgpu::AddressMode::ClampToEdge,
                    mag_filter: wgpu::FilterMode::Nearest,
                    min_filter: wgpu::FilterMode::Nearest,
                    mipmap_filter: wgpu::FilterMode::Nearest,
                    ..Default::default()
                });

                let texture_bind_group = self.device.create_bind_group(
                    &wgpu::BindGroupDescriptor {
                        layout: &self.render_pipeline.get_bind_group_layout(0),
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&texture_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&sampler),
                            }
                        ],
                        label: Some("texture_bind_group"),
                    }
                );

                // Store the resources to prevent them from being dropped
                self._texture = Some(texture);
                self._texture_view = Some(texture_view);
                self._sampler = Some(sampler);
                self.texture_bind_group = Some(texture_bind_group);

        // Update image aspect ratio in uniforms
        let image_aspect = width as f32 / height as f32;
        self.update_uniforms(image_aspect);
    }

    fn update_uniforms(&mut self, image_aspect: f32) {
                let window_aspect = self.config.width as f32 / self.config.height as f32;

                let uniforms = Uniforms {
                    image_aspect,
                    window_aspect,
                    zoom: self.zoom_level,
                    pan_x: self.pan_offset.x,
                    pan_y: self.pan_offset.y,
                    _padding: [0.0; 3],
                };

                self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
            }

            pub fn set_zoom(&mut self, zoom_level: i32, image_aspect: f32) {
                // Convert zoom level (-10 to 10) to zoom factor
                // Positive zoom = zoom in (factor > 1)
                // Negative zoom = zoom out (factor < 1)
                let zoom_factor = if zoom_level >= 0 {
                    1.0 + (zoom_level as f32 * 0.2)  // zoom in: 1.0 to 3.0
                } else {
                    1.0 / (1.0 + (-zoom_level as f32 * 0.2))  // zoom out: 1.0 to ~0.33
                };

                self.zoom_level = zoom_factor;
                self.update_uniforms(image_aspect);
            }

    pub fn set_pan(&mut self, pan_offset: PhysicalPosition<f32>, image_width: u32, image_height: u32) {
        // Normalize pan offset to -1.0 to 1.0 range based on image size
        let norm_x = pan_offset.x / image_width as f32;
        let norm_y = pan_offset.y / image_height as f32;

        self.pan_offset = PhysicalPosition::new(norm_x, norm_y);

        let image_aspect = image_width as f32 / image_height as f32;
        self.update_uniforms(image_aspect);
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.config.format.add_srgb_suffix()),
            ..Default::default()
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
        });

        if let Some(texture_bind_group) = &self.texture_bind_group {
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, texture_bind_group, &[]);
            render_pass.set_bind_group(1, &self.uniform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
        }
    }

    self.queue.submit(std::iter::once(encoder.finish()));
    output.present();

    Ok(())
}
}

