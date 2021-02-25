use wgpu::util::DeviceExt;
use wgpu::CommandEncoderDescriptor;
use zerocopy::AsBytes;

use super::data::{GlobalUniforms, Lights, LocalUniforms};
use crate::components::{Model3D, StaticModel};

// TODO: Have ass_man auto-load all shaders
const FRAG_SRC: &str = include_str!("../../shaders/forward.frag");
const VERT_SRC: &str = include_str!("../../shaders/forward.vert");

pub struct ModelRenderContext {
    depth_view: wgpu::TextureView,
    global_uniform_buf: wgpu::Buffer,
    pub lights_uniform_buf: wgpu::Buffer,
    global_bind_group_layout: wgpu::BindGroupLayout,
    pub local_bind_group_layout: wgpu::BindGroupLayout,
    global_bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    pipeline_layout: wgpu::PipelineLayout,
}

impl ModelRenderContext {
    // Ugly workaround since the OR operation on ShaderStages is not a const-friendly operation
    // Have a pull-request underway on the wgpu-types repo to fix this particular situation
    pub const VERTEX_FRAGMENT_VISIBILITY: wgpu::ShaderStage = wgpu::ShaderStage::from_bits_truncate(
        wgpu::ShaderStage::VERTEX.bits() | wgpu::ShaderStage::FRAGMENT.bits(),
    );

    pub const LOCAL_UNIFORM_BIND_GROUP_LAYOUT_ENTRY: wgpu::BindGroupLayoutEntry =
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: Self::VERTEX_FRAGMENT_VISIBILITY,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

    const GLOBAL_UNIFORM_BIND_GROUP_LAYOUT_ENTRY: wgpu::BindGroupLayoutEntry =
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: Self::VERTEX_FRAGMENT_VISIBILITY,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

    const LIGHTS_UNIFORM_BIND_GROUP_LAYOUT_ENTRY: wgpu::BindGroupLayoutEntry =
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStage::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

    pub fn new(device: &wgpu::Device, window_size: winit::dpi::PhysicalSize<u32>) -> Self {
        // Essentially our depth buffer, needed for keeping track of what objects
        // can be seen by the camera. (i.e. not occluded.)
        let depth_view = Self::create_depth_view(&device, window_size);

        // This describes the layout of bindings to buffers in the shader program
        let global_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    Self::GLOBAL_UNIFORM_BIND_GROUP_LAYOUT_ENTRY,
                    Self::LIGHTS_UNIFORM_BIND_GROUP_LAYOUT_ENTRY,
                ],
            });

        let local_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[Self::LOCAL_UNIFORM_BIND_GROUP_LAYOUT_ENTRY],
            });

        let global_uniforms: GlobalUniforms = Default::default();

        let global_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Global Shader Uniforms"),
            contents: global_uniforms.as_bytes(),
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        });

        let lights: Lights = Default::default();

        let lights_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Lights"),
            contents: lights.as_bytes(),
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        });

        let global_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &global_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &global_uniform_buf,
                        offset: 0,
                        size: None,
                    },
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &lights_uniform_buf,
                        offset: 0,
                        size: None,
                    },
                },
            ],
        });

        let (vs_module, fs_module) = {
            //Todo: Move shader compilation to ass_man
            let mut shader_compiler = shaderc::Compiler::new().unwrap();

            let vs_spirv = shader_compiler
                .compile_into_spirv(
                    VERT_SRC,
                    shaderc::ShaderKind::Vertex,
                    "forward.vert",
                    "main",
                    None,
                )
                .unwrap();
            let fs_spirv = shader_compiler
                .compile_into_spirv(
                    FRAG_SRC,
                    shaderc::ShaderKind::Fragment,
                    "forward.frag",
                    "main",
                    None,
                )
                .unwrap();

            let vs = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
                label: Some("Vertex Shader"),
                source: wgpu::util::make_spirv(&vs_spirv.as_binary_u8()),
                flags: wgpu::ShaderFlags::default(),
            });

            let fs = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
                label: Some("Fragment Shader"),
                source: wgpu::util::make_spirv(&fs_spirv.as_binary_u8()),
                flags: wgpu::ShaderFlags::default(),
            });

            (vs, fs)
        };

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&global_bind_group_layout, &local_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = Self::compile_pipeline(&device, &pipeline_layout, vs_module, fs_module);

        Self {
            depth_view,
            global_uniform_buf,
            lights_uniform_buf,
            global_bind_group_layout,
            local_bind_group_layout,
            global_bind_group,
            pipeline,
            pipeline_layout,
        }
    }

    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        ass_man: &crate::loader::AssetManager,
        model_queue: &super::ModelQueue,
        view: &wgpu::TextureView,
    ) {
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Model Render"),
        });

        for (i, model) in model_queue.model_desc.iter().enumerate() {
            queue.write_buffer(
                &model.uniform_buffer,
                0,
                model_queue.local_uniforms.get(i).unwrap().as_bytes(),
            );
        }

        // Do big boi render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: true,
                    },
                }],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                    attachment: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: true,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: true,
                    }),
                }),
            });

            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.global_bind_group, &[]);

            // render static meshes
            for model in &model_queue.static_models {
                render_pass.set_bind_group(1, &model.bind_group, &[]);
                for mesh in &ass_man.models[model.idx].meshes {
                    render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                    render_pass.draw(0..mesh.num_vertices as u32, 0..1)
                }
            }
            // render dynamic meshes
            for model_desc in &model_queue.model_desc {
                render_pass.set_bind_group(1, &model_desc.bind_group, &[]);
                for mesh in &ass_man.models[model_desc.idx].meshes {
                    render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                    render_pass.draw(0..mesh.num_vertices as u32, 0..1)
                }
            }
        }

        queue.submit(std::iter::once(encoder.finish()));
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: winit::dpi::PhysicalSize<u32>) {
        self.depth_view = Self::create_depth_view(device, size);
    }

    pub fn set_3d_camera(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        window_size: winit::dpi::PhysicalSize<u32>,
        camera: &crate::components::Camera,
        position: cgmath::Vector3<f32>,
        target: cgmath::Vector3<f32>,
    ) {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let proj_view_matrix = super::util::generate_view_matrix(
            camera,
            position,
            target,
            window_size.width as f32 / window_size.height as f32,
        );

        let global_uniforms = GlobalUniforms {
            projection_view_matrix: proj_view_matrix.into(),
            eye_position: [position.x, position.y, position.z, 0.0],
        };

        queue.write_buffer(&self.global_uniform_buf, 0, global_uniforms.as_bytes());
    }

    pub fn recompile_pipeline(
        &mut self,
        device: &wgpu::Device,
        vs_module: wgpu::ShaderModule,
        fs_module: wgpu::ShaderModule,
    ) {
        self.pipeline = Self::compile_pipeline(device, &self.pipeline_layout, vs_module, fs_module);
    }

    fn compile_pipeline(
        device: &wgpu::Device,
        pipeline_layout: &wgpu::PipelineLayout,
        vs_module: wgpu::ShaderModule,
        fs_module: wgpu::ShaderModule,
    ) -> wgpu::RenderPipeline {
        return device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Option::from(pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vs_module,
                entry_point: "main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<super::data::Vertex>() as u64,
                    step_mode: wgpu::InputStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float3,
                        1 => Float3,
                        2 => Float2
                    ],
                }],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: super::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
                clamp_depth: false,
            }),
            fragment: Some(wgpu::FragmentState {
                module: &fs_module,
                entry_point: "main",
                targets: &[wgpu::ColorTargetState {
                    format: super::COLOR_FORMAT,
                    alpha_blend: wgpu::BlendState::REPLACE, // For now
                    color_blend: wgpu::BlendState::REPLACE,
                    write_mask: wgpu::ColorWrite::ALL,
                }],
            }),
            multisample: wgpu::MultisampleState::default(),
        });
    }

    fn create_depth_view(
        device: &wgpu::Device,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> wgpu::TextureView {
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: super::DEPTH_FORMAT,
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
        });

        return depth_texture.create_view(&Default::default());
    }
}
