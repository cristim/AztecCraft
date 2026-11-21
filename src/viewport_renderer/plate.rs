//! The plate's GPU side (#459 P7b): one texture for the current frame, a quad, and two pipelines
//! (textured fill, outline). Layouts and checks live in `azteccraft::image_plane::draw`.
//! A plate failure is remembered here and never reaches the viewport's terminal error.
//! PROVISIONAL (#5): the fill writes depth with `LessEqual`, so nearer geometry draws over the
//! plate and farther geometry is hidden by it, transparent texels included; Outline writes none.

use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Waker},
    time::Duration,
};

use azteccraft::image_plane::{
    cache::FrameKey,
    decode::PlateFrame,
    draw::{
        FILL_VERTICES, OUTLINE_VERTICES, PlateStyle, PlateUploadError, QUAD_BYTES, UNIFORM_BYTES,
        VERTEX_FLOATS, check_frame, quad_vertex_bytes, uniform_bytes,
    },
    fit::PlateQuad,
    model::PlateDisplayMode,
};
use eframe::wgpu;

/// One plate to draw: the shared frame, its identity (an unchanged key skips the upload), the
/// geometry from `plate_quad` and the display settings.
#[derive(Clone)]
#[cfg_attr(not(test), allow(dead_code))] // P8 builds it from the editor; until then only tests do
pub(crate) struct PlateDraw {
    pub(crate) key: FrameKey,
    pub(crate) frame: Arc<PlateFrame>,
    pub(crate) quad: PlateQuad,
    pub(crate) mode: PlateDisplayMode,
    pub(crate) alpha_gain: f32,
}

pub(super) struct TextureSlot {
    texture: wgpu::Texture,
    size: (u32, u32),
    key: FrameKey,
    bind_group: wgpu::BindGroup,
}

pub(super) struct PlateGpu {
    fill: wgpu::RenderPipeline,
    outline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    vertices: wgpu::Buffer,
    pub(super) slot: Option<TextureSlot>,
    output_srgb: bool,
    status: Result<Option<PlateStyle>, PlateUploadError>,
}

/// Runs `create` inside an OutOfMemory error scope so an allocation failure fails the plate only.
fn within_oom_scope<T>(
    device: &wgpu::Device,
    create: impl FnOnce() -> T,
) -> Result<T, PlateUploadError> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    let value = create();
    let mut popped = std::pin::pin!(scope.pop());
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_millis(100)),
    });
    // Anything but "finished with no error" fails the plate: failing loud beats drawing from a
    // texture that may not exist.
    let done = popped
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()));
    match done {
        Poll::Ready(None) => Ok(value),
        _ => Err(PlateUploadError::GpuMemory),
    }
}

impl PlateGpu {
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("camera image plane"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../plate_shader.wgsl").into()),
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera image plane"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    2,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("camera image plane"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        // The outline reads no binding, so it gets an empty layout: it needs no bind group.
        let outline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("camera image plane outline"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = |fragment, topology, depth_write, blend, layout| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("camera image plane"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: (VERTEX_FLOATS * 4) as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x2],
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            fill: pipeline(
                "fragment",
                wgpu::PrimitiveTopology::TriangleList,
                true,
                Some(wgpu::BlendState::ALPHA_BLENDING),
                &pipeline_layout,
            ),
            outline: pipeline(
                "outline",
                wgpu::PrimitiveTopology::LineList,
                false,
                None,
                &outline_layout,
            ),
            layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("camera image plane"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            uniform: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("camera image plane params"),
                size: UNIFORM_BYTES as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            vertices: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("camera image plane quad"),
                size: QUAD_BYTES as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            slot: None,
            output_srgb: format.is_srgb(),
            status: Ok(None),
        }
    }

    /// Validates, then writes the quad and uniform and uploads the frame when its key or size
    /// changed. On an error nothing is drawable.
    pub(super) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        draw: &PlateDraw,
    ) -> Result<(), PlateUploadError> {
        let result = self.try_upload(device, queue, draw);
        self.status = result.map(|()| match PlateStyle::of(draw.mode) {
            PlateStyle::Skip => None,
            style => Some(style),
        });
        result
    }

    fn try_upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        draw: &PlateDraw,
    ) -> Result<(), PlateUploadError> {
        let style = PlateStyle::of(draw.mode);
        let quad = quad_vertex_bytes(&draw.quad)?;
        let shading = match style {
            PlateStyle::Skip => return Ok(()),
            PlateStyle::Outline => {
                queue.write_buffer(&self.vertices, 0, &quad);
                return Ok(());
            }
            PlateStyle::Textured(shading) => shading,
        };
        let uniform = uniform_bytes(shading, draw.alpha_gain, self.output_srgb)?;
        let frame = &*draw.frame;
        check_frame(
            frame.width,
            frame.height,
            frame.rgba.len(),
            device.limits().max_texture_dimension_2d,
        )?;
        let size = (frame.width, frame.height);
        let reusable = self.slot.as_ref().is_some_and(|slot| slot.size == size);
        if !reusable {
            self.slot = None;
            let created = within_oom_scope(device, || {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("camera image plane frame"),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    // The bytes pass through unchanged; an sRGB target is handled in the shader.
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                let view = texture.create_view(&Default::default());
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("camera image plane"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.uniform.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                });
                (texture, bind_group)
            })?;
            self.slot = Some(TextureSlot {
                texture: created.0,
                size,
                key: draw.key,
                bind_group: created.1,
            });
            self.write_frame(queue, frame);
        } else if let Some(slot) = self.slot.as_mut().filter(|slot| slot.key != draw.key) {
            slot.key = draw.key;
            self.write_frame(queue, frame);
        }
        queue.write_buffer(&self.uniform, 0, &uniform);
        queue.write_buffer(&self.vertices, 0, &quad);
        Ok(())
    }

    fn write_frame(&self, queue: &wgpu::Queue, frame: &PlateFrame) {
        let Some(slot) = &self.slot else { return };
        queue.write_texture(
            slot.texture.as_image_copy(),
            &frame.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(frame.width * 4),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, draw: &PlateDraw) {
        match self.status {
            Ok(Some(PlateStyle::Outline)) => {
                pass.set_pipeline(&self.outline);
                pass.set_vertex_buffer(0, self.vertices.slice(..));
                pass.draw(FILL_VERTICES..FILL_VERTICES + OUTLINE_VERTICES, 0..1);
            }
            Ok(Some(PlateStyle::Textured(_))) => {
                let Some(slot) = self.slot.as_ref().filter(|slot| slot.key == draw.key) else {
                    return;
                };
                pass.set_pipeline(&self.fill);
                pass.set_bind_group(0, &slot.bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertices.slice(..));
                pass.draw(0..FILL_VERTICES, 0..1);
            }
            Ok(Some(PlateStyle::Skip)) | Ok(None) | Err(_) => {}
        }
    }

    /// Releases the frame texture (up to 128 MiB) once no plate is submitted.
    pub(super) fn clear(&mut self) {
        self.slot = None;
        self.status = Ok(None);
    }
}
