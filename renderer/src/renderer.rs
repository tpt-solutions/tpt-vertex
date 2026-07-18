//! WebGPU renderer core: device/surface/pipeline setup and frame rendering.
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use tpt_vertex_kernel::assembly::Assembly;
use wgpu::util::DeviceExt;

use crate::camera::Camera;
use crate::mesh::Mesh;
use crate::scene::{MaterialId, Scene};

/// Uniform block shared by every draw call: camera matrices + lighting.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GlobalUniforms {
    /// Column-major view-projection matrix.
    view_proj: [[f32; 4]; 4],
    /// World-space camera eye, for specular terms.
    eye: [f32; 4],
    /// Directional light direction (normalized, world space).
    light_dir: [f32; 4],
    /// Ambient + directional intensities packed as (ambient, diffuse, _, _).
    light_intensity: [f32; 4],
}

/// Per-draw-instance uniforms: the model matrix + base color.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ModelUniforms {
    model: [[f32; 4]; 4],
    color: [f32; 4],
    /// Reserved: selection highlight factor (0 = none, 1 = fully highlighted).
    highlight: [f32; 4],
}

/// Rendering mode for a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// Shaded solids with simple directional lighting.
    Shaded,
    /// Wireframe overlay only.
    Wireframe,
    /// Shaded solids with a wireframe overlay on top.
    ShadedWithWireframe,
}

/// A GPU-resident mesh (vertex + index buffers) plus a CPU-side [`Solid`]
/// copy used for ray picking.
struct GpuMesh {
    vertex: Arc<wgpu::Buffer>,
    tri_index: Arc<wgpu::Buffer>,
    tri_count: u32,
    line_index: Arc<wgpu::Buffer>,
    line_count: u32,
    pick_solid: tpt_vertex_kernel::geometry::solid::Solid,
}

/// An entry used for CPU-side picking: which scene node this is, the mesh it
/// draws, and the world transform applied to it.
#[derive(Debug, Clone)]
struct PickEntry {
    node_id: u64,
    world: glam::Mat4,
    mesh_index: usize,
    material: MaterialId,
}

/// The renderer owns the device, surface, pipelines, and the current scene's
/// GPU buffers. It is backend-agnostic; the surface/adapter are created from
/// whatever the host provides (a canvas on wasm, a window on native).
pub struct Renderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    shaded_pipeline: wgpu::RenderPipeline,
    wireframe_pipeline: wgpu::RenderPipeline,
    global_buffer: wgpu::Buffer,

    meshes: Vec<GpuMesh>,
    model_buffers: Vec<wgpu::Buffer>,
    model_bind_groups: Vec<wgpu::BindGroup>,
    pick_list: Vec<PickEntry>,
    highlighted: Option<u64>,
    mode: RenderMode,
}

impl Renderer {
    /// Create a renderer bound to the given surface and adapter.
    ///
    /// `width`/`height` are the initial surface size in physical pixels.
    pub async fn new(
        _instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        adapter: &wgpu::Adapter,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Result<Self, RendererError> {
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("tpt-vertex-renderer device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .map_err(RendererError::DeviceRequest)?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vertex-shaded"),
            source: wgpu::ShaderSource::Wgsl(SHADED_WGSL.into()),
        });

        let global_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("global uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let model_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shaded pipeline layout"),
            bind_group_layouts: &[&global_bgl, &model_bgl],
            push_constant_ranges: &[],
        });

        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<crate::mesh::Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        };

        let compilation_options = wgpu::PipelineCompilationOptions::default();

        let shaded_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shaded pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: std::slice::from_ref(&vertex_buffer_layout),
                compilation_options: compilation_options.clone(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: compilation_options.clone(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let wireframe_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("wireframe pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[vertex_buffer_layout],
                compilation_options: compilation_options.clone(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_wire",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options,
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                cull_mode: None,
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let global_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("global uniforms"),
            size: std::mem::size_of::<GlobalUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Renderer {
            device,
            queue,
            surface,
            surface_config,
            shaded_pipeline,
            wireframe_pipeline,
            global_buffer,
            meshes: Vec::new(),
            model_buffers: Vec::new(),
            model_bind_groups: Vec::new(),
            pick_list: Vec::new(),
            highlighted: None,
            mode: RenderMode::Shaded,
        })
    }

    /// Reconfigure the swap chain for a new surface size.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.surface_config.width = width.max(1);
        self.surface_config.height = height.max(1);
        self.surface.configure(&self.device, &self.surface_config);
    }

    /// Set the outline/wireframe rendering mode.
    pub fn set_mode(&mut self, mode: RenderMode) {
        self.mode = mode;
    }

    pub fn mode(&self) -> RenderMode {
        self.mode
    }

    fn depth_texture(&self) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: self.surface_config.width,
                height: self.surface_config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    }

    /// Upload a scene, replacing any previously uploaded geometry.
    pub fn upload_scene(&mut self, scene: &Scene) {
        let mut meshes = Vec::with_capacity(scene.meshes.len());
        for m in &scene.meshes {
            meshes.push(self.upload_mesh(m));
        }

        let mut model_buffers = Vec::with_capacity(scene.draw_items.len());
        let mut model_bind_groups = Vec::with_capacity(scene.draw_items.len());
        let mut pick_list = Vec::with_capacity(scene.draw_items.len());
        let model_bgl = self.shaded_pipeline.get_bind_group_layout(1);
        for item in &scene.draw_items {
            let model = ModelUniforms {
                model: item.world.to_cols_array_2d(),
                color: material_color(item.material),
                highlight: [0.0; 4],
            };
            let buf = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("model uniforms"),
                    contents: bytemuck::bytes_of(&model),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("model bind group"),
                layout: &model_bgl,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                }],
            });
            pick_list.push(PickEntry {
                node_id: item.node_id,
                world: item.world,
                mesh_index: item.mesh_index,
                material: item.material,
            });
            model_buffers.push(buf);
            model_bind_groups.push(bg);
        }

        self.meshes = meshes;
        self.model_buffers = model_buffers;
        self.model_bind_groups = model_bind_groups;
        self.pick_list = pick_list;
        self.highlighted = None;
    }

    /// Convenience: build a scene from an [`Assembly`] and upload it.
    pub fn upload_assembly(&mut self, asm: &Assembly) {
        let scene = crate::scene::scene_from_assembly(asm);
        self.upload_scene(&scene);
    }

    /// Set the highlighted node (used for hover/selection feedback). Pass
    /// `None` to clear. Updates the `highlight` field of every model uniform
    /// on the GPU so the next frame renders the emphasis.
    pub fn set_highlighted(&mut self, node_id: Option<u64>) {
        if self.highlighted == node_id {
            return;
        }
        self.highlighted = node_id;
        for (i, entry) in self.pick_list.iter().enumerate() {
            let amount = if Some(entry.node_id) == node_id {
                1.0
            } else {
                0.0
            };
            let model = ModelUniforms {
                model: entry.world.to_cols_array_2d(),
                color: material_color(entry.material),
                highlight: [amount, 0.0, 0.0, 0.0],
            };
            self.queue
                .write_buffer(&self.model_buffers[i], 0, bytemuck::bytes_of(&model));
        }
    }

    /// Pick the nearest scene node under the given normalized device
    /// coordinates (`ndc_x`, `ndc_y` in -1..1, y up). Returns the node id of
    /// the closest hit, or `None` if nothing was hit.
    pub fn pick(&self, camera: &Camera, ndc_x: f32, ndc_y: f32) -> Option<u64> {
        let inv_vp = camera.view_proj().inverse();
        let eye = camera.eye();
        let ray = crate::picking::screen_ray(inv_vp, ndc_x, ndc_y, eye);
        let mut best: Option<(f32, u64)> = None;
        for entry in &self.pick_list {
            let mesh = &self.meshes[entry.mesh_index];
            let solid = &mesh.pick_solid;
            if let Some((t, _)) = crate::picking::ray_vs_solid(ray, solid, entry.world) {
                if best.map(|(bt, _)| t < bt).unwrap_or(true) {
                    best = Some((t, entry.node_id));
                }
            }
        }
        best.map(|(_, id)| id)
    }

    fn upload_mesh(&self, mesh: &Mesh) -> GpuMesh {
        let vertex = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh vertices"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let tri_index = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh indices"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        let line_index = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh line indices"),
                contents: bytemuck::cast_slice(&mesh.line_indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        let tri_count = mesh.index_count();
        let line_count = mesh.line_indices.len() as u32;
        let pick_solid = mesh.solid();
        GpuMesh {
            vertex: Arc::new(vertex),
            tri_index: Arc::new(tri_index),
            tri_count,
            line_index: Arc::new(line_index),
            line_count,
            pick_solid,
        }
    }

    /// Render one frame using the given camera. Returns an error if the
    /// surface is lost or the swap chain is out of date.
    pub fn render_frame(&mut self, camera: &Camera) -> Result<(), RendererError> {
        let view_proj = camera.view_proj().to_cols_array_2d();
        let eye = camera.eye();
        let globals = GlobalUniforms {
            view_proj,
            eye: [eye.x, eye.y, eye.z, 1.0],
            light_dir: [0.4, 0.8, 0.6, 0.0],
            light_intensity: [0.25, 0.9, 0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.global_buffer, 0, bytemuck::bytes_of(&globals));

        let frame = self
            .surface
            .get_current_texture()
            .map_err(RendererError::Surface)?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth = self.depth_texture();
        let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

        let global_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("global bind group"),
            layout: &self.shaded_pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.global_buffer.as_entire_binding(),
            }],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.12,
                            g: 0.13,
                            b: 0.15,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            if self.mode != RenderMode::Wireframe {
                pass.set_pipeline(&self.shaded_pipeline);
                pass.set_bind_group(0, &global_bg, &[]);
                self.draw_all(&mut pass, false);
            }

            if self.mode != RenderMode::Shaded {
                pass.set_pipeline(&self.wireframe_pipeline);
                pass.set_bind_group(0, &global_bg, &[]);
                self.draw_all(&mut pass, true);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }

    fn draw_all<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, wireframe: bool) {
        for (bg, mesh) in self.model_bind_groups.iter().zip(self.meshes.iter()) {
            pass.set_bind_group(1, bg, &[]);
            pass.set_vertex_buffer(0, mesh.vertex.slice(..));
            if wireframe {
                pass.set_index_buffer(mesh.line_index.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.line_count, 0, 0..1);
            } else {
                pass.set_index_buffer(mesh.tri_index.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.tri_count, 0, 0..1);
            }
        }
    }
}

/// Map a material id to a base (albedo) color.
#[allow(dead_code)]
fn material_color(id: MaterialId) -> [f32; 4] {
    let palette: &[[f32; 4]] = &[
        [0.75, 0.77, 0.80, 1.0], // default steel
        [0.80, 0.55, 0.35, 1.0], // warm
        [0.45, 0.65, 0.80, 1.0], // cool
        [0.70, 0.70, 0.40, 1.0], // brass
    ];
    palette[(id as usize) % palette.len()]
}

/// Errors surfaced by the renderer.
#[derive(Debug, thiserror::Error)]
pub enum RendererError {
    #[error("failed to acquire GPU device: {0}")]
    DeviceRequest(#[from] wgpu::RequestDeviceError),
    #[error("surface lost or out of date: {0}")]
    Surface(#[from] wgpu::SurfaceError),
}

const SHADED_WGSL: &str = r#"
struct Global {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    light_dir: vec4<f32>,
    light_intensity: vec4<f32>,
};
struct Model {
    model: mat4x4<f32>,
    color: vec4<f32>,
    highlight: vec4<f32>,
};
@group(0) @binding(0) var<uniform> G: Global;
@group(1) @binding(0) var<uniform> M: Model;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world: vec3<f32>,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>) -> VsOut {
    var out: VsOut;
    let world = M.model * vec4<f32>(position, 1.0);
    out.world = world.xyz;
    out.clip = G.view_proj * world;
    // Normals use the upper-left 3x3 of the model matrix (rigid transform only).
    out.normal = normalize((M.model * vec4<f32>(normal, 0.0)).xyz);
    return out;
}

fn shade(base: vec3<f32>, N: vec3<f32>, V: vec3<f32>) -> vec3<f32> {
    let L = normalize(-G.light_dir.xyz);
    let ambient = G.light_intensity.x;
    let diffuse = G.light_intensity.y * max(dot(N, L), 0.0);
    let H = normalize(L + V);
    let spec = pow(max(dot(N, H), 0.0), 32.0) * 0.25;
    return base * (ambient + diffuse) + vec3<f32>(spec);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let N = normalize(in.normal);
    let V = normalize(G.eye.xyz - in.world);
    var color = shade(M.color.rgb, N, V);
    color = mix(color, vec3<f32>(1.0, 0.85, 0.2), M.highlight.x);
    return vec4<f32>(color, M.color.a);
}

@fragment
fn fs_wire(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(0.05, 0.06, 0.08, 0.6);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_blocks_are_16_byte_aligned() {
        // wgpu uniform buffers require 16-byte alignment for mat4/vec4 members.
        assert_eq!(std::mem::size_of::<GlobalUniforms>() % 16, 0);
        assert_eq!(std::mem::size_of::<ModelUniforms>() % 16, 0);
        // They must be plain-old-data so bytemuck can cast them to bytes.
        fn assert_pod<T: bytemuck::Pod>() {}
        assert_pod::<GlobalUniforms>();
        assert_pod::<ModelUniforms>();
    }

    #[test]
    fn material_palette_is_stable() {
        // material_color must return a valid (alpha = 1) color for the default.
        let c = material_color(0);
        assert_eq!(c[3], 1.0);
    }

    #[test]
    fn render_mode_is_copy_eq() {
        let mut m = RenderMode::Shaded;
        assert_eq!(m, RenderMode::Shaded);
        m = RenderMode::Wireframe;
        assert_eq!(m, RenderMode::Wireframe);
        assert_ne!(m, RenderMode::ShadedWithWireframe);
    }
}
