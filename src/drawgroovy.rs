#![allow(warnings)]

use std::mem::ManuallyDrop;

use image::RgbaImage;

use crate::{
    draw::CanvasConfig,
    gfx::{self, easy, prelude::*, SupportedBackend},
    utils::over,
    windowing::{
        dpi::{LogicalSize, PhysicalSize},
        window::Window,
    },
};

#[cfg(all(target_arch = "wasm32", not(feature = "opengl")))]
compile_error!("Web builds (wasm32) require the `opengl` feature to be enabled.");

#[cfg(not(all(target_arch = "wasm32", feature = "bypass_spirv_cross")))]
const SHADER_SOURCES: (&'static [u8], &'static [u8]) = (
    include_bytes!("../assets/shaders/compiled/groovy.vert.spv"),
    include_bytes!("../assets/shaders/compiled/groovy.frag.spv"),
);

#[cfg(all(target_arch = "wasm32", feature = "bypass_spirv_cross"))]
const SHADER_SOURCES: (&'static [u8], &'static [u8]) = (
    include_bytes!("../assets/shaders/compiled/groovy.es.vert"),
    include_bytes!("../assets/shaders/compiled/groovy.es.frag"),
);

pub const MAX_SPRITES: usize = 10000;
const VERTEX_BUFFER_LEN: usize = MAX_SPRITES * 6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub tint: [f32; 4],
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[repr(C)]
struct Vertex {
    pub tint: [f32; 4],
    pub uv: [f32; 2],
    pub offset: [f32; 3],
}

fn wiperr<T>(_: T) -> () {}

fn is_srgb(surface_format: Format) -> bool {
    surface_format.base_format().1 == hal::format::ChannelType::Srgb
}

fn texture_format(surface_format: Format) -> Format {
    if is_srgb(surface_format) {
        Format::Rgba8Srgb
    } else {
        Format::Rgba8Unorm
    }
}

struct Resources<B: SupportedBackend> {
    _instance: Option<B::Instance>,
    surface: B::Surface,
    command_pool: B::CommandPool,
    vertex_buffer: (B::Memory, B::Buffer),
    atlas_image: (B::Memory, B::Image, B::ImageView),
    sampler: B::Sampler,
    render_pass: B::RenderPass,
    desc_set_layout: B::DescriptorSetLayout,
    desc_set_pool: B::DescriptorPool,
    pipeline_layout: B::PipelineLayout,
    pipeline: B::GraphicsPipeline,
    submission_complete_fence: B::Fence,
    rendering_complete_semaphore: B::Semaphore,
    intermediate_canvas: (B::Memory, B::Image, B::ImageView),
    blit_render_pass: B::RenderPass,
    blit_pipeline_layout: B::PipelineLayout,
    blit_pipeline: B::GraphicsPipeline,
}

pub struct Renderer<'a, B: SupportedBackend> {
    context: &'a mut DrawContext<B>,
    clear_color: Color,
    intermediate_framebuffer: B::Framebuffer,
    framebuffer: Option<(
        B::Framebuffer,
        <B::Surface as PresentationSurface<B>>::SwapchainImage,
        Viewport,
    )>,
    sprites: Vec<Sprite>,
}

impl<'a, B: SupportedBackend> Renderer<'a, B> {
    pub fn sprite(&mut self, sprite: Sprite) {
        self.sprites.push(sprite);
    }
}

impl<'a, B: SupportedBackend> Drop for Renderer<'a, B> {
    fn drop(&mut self) {
        let Resources {
            command_pool,
            vertex_buffer,
            surface,
            submission_complete_fence,
            rendering_complete_semaphore,
            pipeline_layout,
            pipeline,
            render_pass,
            intermediate_canvas,
            blit_pipeline_layout,
            blit_pipeline,
            blit_render_pass,
            ..
        } = &mut *self.context.resources;

        let canvas_properties = self.context.canvas_config.canvas_properties(
            [
                self.context.surface_extent.width,
                self.context.surface_extent.height,
            ],
            self.context.scale_factor,
        );


        // Fill vertex cache
        assert!(self.sprites.len() <= MAX_SPRITES);

        let verts = &mut self.context.vertex_cache;
        verts.clear(); // TODO: Maybe actually cache?

        let [canvas_width, canvas_height] = canvas_properties.physical_canvas_size;

        let scale_x =
            (2.0 / canvas_width as f64) as f32;
        let scale_y =
            (2.0 / canvas_height as f64) as f32;

        let project = |x, y| {
            #[cfg(all(target_arch = "wasm32", feature = "bypass_spirv_cross"))]
            {
                [(x * scale_x) - 1., -1. * ((y * scale_y) - 1.), 0.]
            }
            #[cfg(not(all(target_arch = "wasm32", feature = "bypass_spirv_cross")))]
            {
                [(x * scale_x) - 1., (y * scale_y) - 1., 0.]
            }
        };

        for sprite in &self.sprites {
            let tint = if is_srgb(self.context.surface_color_format) {
                gfx::srgb_to_linear(sprite.tint)
            } else {
                sprite.tint
            };
            let [x, y] = sprite.pos;
            let [w, h] = sprite.size;
            let p0 = Vertex {
                offset: project(x, y),
                tint: tint,
                uv: [0., 0.],
            };
            let p1 = Vertex {
                offset: project(x, y + h),
                tint: tint,
                uv: [0., 1.],
            };
            let p2 = Vertex {
                offset: project(x + w, y + h),
                tint: tint,
                uv: [1., 1.],
            };
            let p3 = Vertex {
                offset: project(x + w, y),
                tint: tint,
                uv: [1., 0.],
            };
            verts.push(p0);
            verts.push(p1);
            verts.push(p2);
            verts.push(p0);
            verts.push(p2);
            verts.push(p3);
        }

        // Upload to vertex buffer
        let vertex_bytes = verts.len() * std::mem::size_of::<Vertex>();
        unsafe {
            use gfx_hal::memory::Segment;

            let (memory, buffer) = vertex_buffer;
            let segment = Segment {
                offset: 0,
                size: Some(vertex_bytes as u64),
            };
            let mapped_memory = self
                .context
                .device
                .map_memory(memory, segment.clone())
                .expect("Failed to map memory");

            std::ptr::copy_nonoverlapping(verts.as_ptr() as *const u8, mapped_memory, vertex_bytes);

            self.context
                .device
                .flush_mapped_memory_ranges(over([(&*memory, segment)]))
                .expect("Out of memory");

            self.context.device.unmap_memory(memory);
        }

        let ([x, y], [w, h]) = canvas_properties.viewport_scissor_rect;
        let rect = hal::pso::Rect { x, y, w, h };

        if let Some((framebuffer, surface_image, _viewport)) = self.framebuffer.take() {
            use std::borrow::Borrow;

            unsafe {
                use hal::command::{
                    ClearColor, ClearValue, CommandBuffer, CommandBufferFlags, SubpassContents,
                };

                self.context
                    .command_buffer
                    .begin_primary(CommandBufferFlags::ONE_TIME_SUBMIT);

                self.context
                    .command_buffer
                    .set_viewports(0, over([Viewport {
                        rect,
                        depth: 0.0..1.0,
                    }]));
                self.context
                    .command_buffer
                    .set_scissors(0, over([rect]));

                self.context.command_buffer.begin_render_pass(
                    render_pass,
                    &self.intermediate_framebuffer,
                    viewport.rect,
                    over([RenderAttachmentInfo {
                        image_view: &intermediate_canvas.2,
                        clear_value: ClearValue {
                            color: ClearColor {
                                float32: self.clear_color,
                            },
                        },
                    }]),
                    SubpassContents::Inline,
                );

                self.context.command_buffer.bind_graphics_descriptor_sets(
                    pipeline_layout,
                    0,
                    over([&self.context.desc_set]),
                    over([]),
                );

                self.context.command_buffer.bind_vertex_buffers(
                    0,
                    over([(
                        &vertex_buffer.1,
                        gfx_hal::buffer::SubRange {
                            offset: 0,
                            size: Some(vertex_bytes as u64),
                        },
                    )]),
                );

                self.context.command_buffer.bind_graphics_pipeline(pipeline);

                self.context.command_buffer.begin_render_pass(
                    render_pass,
                    &self.intermediate_framebuffer,
                    rect,
                    over([RenderAttachmentInfo {
                        image_view: &intermediate_canvas.2,
                        clear_value: ClearValue {
                            color: ClearColor {
                                float32: self.clear_color,
                            },
                        },
                    }]),
                    SubpassContents::Inline,
                );

                let num_verts = verts.len() as u32;
                self.context.command_buffer.draw(6..num_verts, 0..1);

                self.context.command_buffer.end_render_pass();

                {
                    use gfx_hal::image::Access;
                    use gfx_hal::memory::{Barrier, Dependencies};
                    use gfx_hal::pso::PipelineStage;

                    self.context.command_buffer.pipeline_barrier(
                        PipelineStage::all()..PipelineStage::all(),
                        Dependencies::empty(),
                        over([Barrier::AllImages(
                            Access::SHADER_READ..Access::SHADER_WRITE,
                        )]),
                    );
                }

                self.context.command_buffer.begin_render_pass(
                    blit_render_pass,
                    &framebuffer,
                    viewport.rect,
                    over([RenderAttachmentInfo {
                        image_view: surface_image.borrow(),
                        clear_value: ClearValue {
                            color: ClearColor {
                                float32: [1., 0., 1., 1.],
                            },
                        },
                    }]),
                    SubpassContents::Inline,
                );

                self.context.command_buffer.bind_graphics_pipeline(blit_pipeline);

                self.context.command_buffer.bind_graphics_descriptor_sets(
                    blit_pipeline_layout,
                    0,
                    over([&self.context.blit_desc_set]),
                    over([]),
                );

                self.context.command_buffer.draw(0..6, 0..1);

                self.context.command_buffer.end_render_pass();

                self.context.command_buffer.finish();

                use hal::queue::CommandQueue;

                self.context.queue_group.queues[0].submit(
                    over([&self.context.command_buffer]),
                    over([]),
                    over([&*rendering_complete_semaphore]),
                    Some(submission_complete_fence),
                );

                let result = self.context.queue_group.queues[0].present(
                    surface,
                    surface_image,
                    Some(rendering_complete_semaphore),
                );

                if result.is_err() {
                    self.context.swapchain_invalidated = Some(());
                }

                self.context.device.destroy_framebuffer(framebuffer);
            }
        }
    }
}

pub struct DrawContext<B: SupportedBackend> {
    resources: ManuallyDrop<Resources<B>>,
    adapter: Adapter<B>,
    device: B::Device,
    queue_group: QueueGroup<B>,
    command_buffer: B::CommandBuffer,
    surface_color_format: hal::format::Format,
    desc_set: B::DescriptorSet,
    blit_desc_set: B::DescriptorSet,
    surface_extent: hal::window::Extent2D,
    scale_factor: f64,
    framebuffer_attachment: Option<FramebufferAttachment>,
    swapchain_invalidated: Option<()>,
    texture_atlas: RgbaImage,
    vertex_cache: Vec<Vertex>,
    canvas_config: CanvasConfig,
}

impl<B: SupportedBackend> DrawContext<B> {
    pub fn new(window: &Window, canvas_config: CanvasConfig, texture_atlas: RgbaImage) -> Result<Self, ()> {
        let (
            instance,
            surface,
            surface_color_format,
            adapter,
            device,
            mut queue_group,
            mut command_pool,
        ) = easy::init::<B>(window, "jamjar_drawgroovy", 1)
            .map_err(|msg| eprintln!("easy::init error: {}", msg))?;

        let mut command_buffer = unsafe { command_pool.allocate_one(hal::command::Level::Primary) };

        let dpi = window.scale_factor();
        let physical_size: PhysicalSize<u32> = window.inner_size();
        let logical_size: LogicalSize<u32> = physical_size.to_logical(dpi);
        let mut surface_extent = hal::window::Extent2D {
            width: physical_size.width,
            height: physical_size.height,
        };

        let vertex_buffer = unsafe {
            gfx::make_buffer::<B>(
                &device,
                &adapter.physical_device,
                VERTEX_BUFFER_LEN * std::mem::size_of::<Vertex>(),
                hal::buffer::Usage::VERTEX,
                hal::memory::Properties::CPU_VISIBLE,
            )
        };

        let atlas_image_size = texture_atlas.dimensions();
        let atlas_image = unsafe {
            use hal::format::{Aspects, Format};
            use hal::image::Usage;

            gfx::make_image::<B>(
                &device,
                &adapter.physical_device,
                atlas_image_size,
                texture_format(surface_color_format),
                Usage::SAMPLED | Usage::TRANSFER_DST,
                Aspects::COLOR,
            )
        };

        let intermediate_canvas = unsafe {
            use gfx_hal::format::{Aspects, Format};
            use gfx_hal::image::Usage;

            gfx::make_image::<B>(
                &device,
                &adapter.physical_device,
                (surface_extent.width, surface_extent.height),
                Format::Rgba8Srgb,
                Usage::COLOR_ATTACHMENT | Usage::SAMPLED,
                Aspects::COLOR,
            )
        };

        let sampler = unsafe {
            use hal::image::{Filter, SamplerDesc, Usage, WrapMode};

            device
                .create_sampler(&SamplerDesc::new(Filter::Nearest, WrapMode::Tile))
                .expect("TODO")
        };

        unsafe {
            gfx::upload_image::<B>(
                &device,
                &adapter.physical_device,
                &mut command_pool,
                &mut queue_group.queues[0],
                &atlas_image.1,
                atlas_image_size,
                &texture_atlas,
            );
        }

        let render_pass = easy::render_pass::<B>(&device, Format::Rgba8Srgb, None, true);
        let blit_render_pass = easy::render_pass::<B>(&device, surface_color_format, None, false);

        let (desc_set_layout, mut desc_set_pool, mut desc_sets) = easy::desc_sets::<B>(
            &device,
            2,
            0,
            1,
            1,
            vec![
                (vec![], vec![&atlas_image.2], vec![&sampler]),
                (vec![], vec![&intermediate_canvas.2], vec![&sampler]),
            ],
        );

        let mut desc_set = desc_sets.remove(0);
        let mut blit_desc_set = desc_sets.remove(0);

        let (pipeline, pipeline_layout) = easy::pipeline::<B>(
            &device,
            Some(&desc_set_layout),
            0,
            SHADER_SOURCES.0,
            SHADER_SOURCES.1,
            &render_pass,
            None,
            &[4, 2, 3],
            None,
            None,
        );

        let (blit_pipeline, blit_pipeline_layout) = easy::pipeline::<B>(
            &device,
            Some(&desc_set_layout),
            0,
            SHADER_SOURCES.0,
            SHADER_SOURCES.1,
            &blit_render_pass,
            None,
            &[4, 2, 3],
            None,
            None,
        );

        let submission_complete_fence = device.create_fence(true).expect("Out of memory");
        let rendering_complete_semaphore = device.create_semaphore().expect("Out of memory");

        Ok(DrawContext {
            resources: ManuallyDrop::new(Resources {
                _instance: Some(instance),
                surface,
                command_pool,
                vertex_buffer,
                atlas_image,
                sampler,
                render_pass,
                desc_set_layout,
                desc_set_pool,
                pipeline_layout,
                pipeline,
                submission_complete_fence,
                rendering_complete_semaphore,
                intermediate_canvas,
                blit_render_pass,
                blit_pipeline_layout,
                blit_pipeline,
            }),
            adapter,
            device,
            queue_group,
            command_buffer,
            surface_color_format,
            surface_extent,
            scale_factor: dpi,
            desc_set,
            blit_desc_set,
            framebuffer_attachment: None,
            swapchain_invalidated: Some(()),
            texture_atlas,
            vertex_cache: Vec::with_capacity(VERTEX_BUFFER_LEN),
            canvas_config,
        })
    }

    pub fn resolution_changed(&mut self, resolution: (u32, u32)) {
        self.surface_extent = hal::window::Extent2D {
            width: resolution.0,
            height: resolution.1,
        };
        self.swapchain_invalidated = Some(());
    }

    pub fn scale_factor_changed(&mut self, scale_factor: f64, resolution: (u32, u32)) {
        self.scale_factor = scale_factor;
        self.resolution_changed(resolution);
    }

    pub fn start_rendering(&mut self, clear_color: Color) -> Renderer<B> {
        let Resources {
            surface,
            submission_complete_fence,
            command_pool,
            render_pass,
            blit_render_pass,
            ..
        } = &mut *self.resources;

        unsafe {
            use hal::pool::CommandPool;

            // We refuse to wait more than a second, to avoid hanging.
            let render_timeout_ns = 1_000_000_000;

            self.device
                .wait_for_fence(&submission_complete_fence, render_timeout_ns)
                .expect("Out of memory or device lost");

            self.device
                .reset_fence(submission_complete_fence)
                .expect("Out of memory");

            command_pool.reset(false);
        }

        if self.swapchain_invalidated.take().is_some() {
            self.framebuffer_attachment = Some(easy::reconfigure_swapchain::<B>(
                    surface,
                    &self.adapter,
                    &self.device,
                    self.surface_color_format,
                    &mut self.surface_extent,
            ));
        }

        let intermediate_framebuffer = unsafe {
            use gfx_hal::image::Extent;

            self.device
                .create_framebuffer(
                    render_pass,
                    self.framebuffer_attachment.iter().cloned(),
                    Extent {
                        width: self.surface_extent.width,
                        height: self.surface_extent.height,
                        depth: 1,
                    },
                )
                .unwrap()
        };

        let framebuffer = easy::acquire_framebuffer::<B>(
            &self.device,
            surface,
            &self.surface_extent,
            &blit_render_pass,
            self.framebuffer_attachment.clone().unwrap(),
        );

        let framebuffer = match framebuffer {
            Ok(x) => Some(x),
            Err(msg) => {
                eprintln!("easy::acquire_framebuffer: {:?}", msg);
                self.swapchain_invalidated = Some(());
                None
            }
        };

        let mut renderer = Renderer {
            context: self,
            clear_color,
            intermediate_framebuffer,
            framebuffer,
            sprites: vec![
                Sprite { pos: [0., 0.], size: [512., 256.], tint: [1., 1., 1., 1.]}, // Note: Dummy sprite for fullscreen quad
            ],
        };
        renderer
    }
}

impl<B: SupportedBackend> Drop for DrawContext<B> {
    fn drop(&mut self) {
        unsafe {
            let Resources {
                _instance,
                mut surface,
                command_pool,
                vertex_buffer,
                atlas_image,
                sampler,
                render_pass,
                desc_set_layout,
                desc_set_pool,
                pipeline_layout,
                pipeline,
                submission_complete_fence,
                rendering_complete_semaphore,
                intermediate_canvas,
                blit_render_pass,
                blit_pipeline_layout,
                blit_pipeline,
            } = ManuallyDrop::take(&mut self.resources);

            self.device.destroy_semaphore(rendering_complete_semaphore);
            self.device.destroy_fence(submission_complete_fence);
            self.device.destroy_graphics_pipeline(pipeline);
            self.device.destroy_pipeline_layout(pipeline_layout);
            self.device.destroy_descriptor_pool(desc_set_pool);
            self.device.destroy_descriptor_set_layout(desc_set_layout);
            self.device.destroy_render_pass(render_pass);
            self.device.destroy_sampler(sampler);
            {
                let (mem, img, view) = atlas_image;
                self.device.destroy_image_view(view);
                self.device.destroy_image(img);
                self.device.free_memory(mem);
            }
            {
                let (mem, buf) = vertex_buffer;
                self.device.destroy_buffer(buf);
                self.device.free_memory(mem);
            }
            self.device.destroy_command_pool(command_pool);
            surface.unconfigure_swapchain(&self.device);
            if let Some(instance) = _instance {
                instance.destroy_surface(surface);
            }
        }
    }
}
