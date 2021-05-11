#![allow(warnings)]

use std::mem::ManuallyDrop;

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
    include_bytes!("../../assets/shaders/compiled/sloth.vert.spv"),
    include_bytes!("../../assets/shaders/compiled/sloth.frag.spv"),
);

#[cfg(all(target_arch = "wasm32", feature = "bypass_spirv_cross"))]
const SHADER_SOURCES: (&'static [u8], &'static [u8]) = (
    include_bytes!("../../assets/shaders/compiled/sloth.es.vert"),
    include_bytes!("../../assets/shaders/compiled/sloth.es.frag"),
);

fn wiperr<T>(_: T) -> () {}

fn texture_format(surface_format: Format) -> Format {
    if surface_format.base_format().1 == hal::format::ChannelType::Srgb {
        Format::Rgba8Srgb
    } else {
        Format::Rgba8Unorm
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
    surface_extent: hal::window::Extent2D,
    scale_factor: f64,
    framebuffer_attachment: Option<FramebufferAttachment>,
    swapchain_invalidated: Option<()>,
    canvas_image_size: (u32, u32),
    canvas_config: CanvasConfig,
}

impl<B: SupportedBackend> DrawContext<B> {
    pub fn new(window: &Window, canvas_config: CanvasConfig) -> Result<Self, ()> {
        let (
            instance,
            surface,
            surface_color_format,
            adapter,
            device,
            queue_group,
            mut command_pool,
        ) = easy::init::<B>(window, "jamjar_sloth", 1)
            .map_err(|msg| eprintln!("easy::init error: {}", msg))?;

        let mut command_buffer = unsafe { command_pool.allocate_one(hal::command::Level::Primary) };

        let scale_factor = window.scale_factor();
        let physical_size: PhysicalSize<u32> = window.inner_size();
        let logical_size: LogicalSize<u32> = physical_size.to_logical(scale_factor);
        let mut surface_extent = hal::window::Extent2D {
            width: physical_size.width,
            height: physical_size.height,
        };

        let canvas_image_size = logical_size.into();
        let canvas_image = unsafe {
            use hal::format::{Aspects, Format};
            use hal::image::Usage;

            gfx::make_image::<B>(
                &device,
                &adapter.physical_device,
                canvas_image_size,
                texture_format(surface_color_format),
                Usage::SAMPLED | Usage::TRANSFER_DST,
                Aspects::COLOR,
            )
        };

        let sampler = unsafe {
            use hal::image::{Filter, SamplerDesc, Usage, WrapMode};

            device
                .create_sampler(&SamplerDesc::new(Filter::Linear, WrapMode::Tile))
                .expect("TODO")
        };

        let render_pass = easy::render_pass::<B>(&device, surface_color_format, None, false);

        let (desc_set_layout, mut desc_set_pool, mut desc_sets) = easy::desc_sets::<B>(
            &device,
            vec![(vec![], vec![&canvas_image.2], vec![&sampler])],
        );
        let mut desc_set = desc_sets.remove(0);

        let (pipeline, pipeline_layout) = easy::pipeline::<B>(
            &device,
            Some(&desc_set_layout),
            0,
            SHADER_SOURCES.0,
            SHADER_SOURCES.1,
            &render_pass,
            None,
            &[],
        );

        let submission_complete_fence = device.create_fence(true).expect("Out of memory");
        let rendering_complete_semaphore = device.create_semaphore().expect("Out of memory");

        Ok(DrawContext {
            resources: ManuallyDrop::new(Resources {
                _instance: Some(instance),
                surface,
                command_pool,
                canvas_image,
                sampler,
                render_pass,
                desc_set_layout,
                desc_set_pool,
                pipeline_layout,
                pipeline,
                submission_complete_fence,
                rendering_complete_semaphore,
            }),
            adapter,
            device,
            queue_group,
            command_buffer,
            surface_color_format,
            surface_extent,
            scale_factor,
            desc_set,
            framebuffer_attachment: None,
            swapchain_invalidated: Some(()),
            canvas_image_size,
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

    pub fn set_canvas_config(&mut self, canvas_config: CanvasConfig) {
        self.canvas_config = canvas_config;
    }

    pub fn start_rendering(&mut self, clear_color: Color) -> Renderer<B> {
        let Resources {
            surface,
            submission_complete_fence,
            command_pool,
            render_pass,
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

        let framebuffer = easy::acquire_framebuffer::<B>(
            &self.device,
            surface,
            &self.surface_extent,
            &render_pass,
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
            framebuffer,
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
                canvas_image,
                sampler,
                render_pass,
                desc_set_layout,
                desc_set_pool,
                pipeline_layout,
                pipeline,
                submission_complete_fence,
                rendering_complete_semaphore,
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
                let (mem, img, view) = canvas_image;
                self.device.destroy_image_view(view);
                self.device.destroy_image(img);
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

struct Resources<B: SupportedBackend> {
    _instance: Option<B::Instance>,
    surface: B::Surface,
    command_pool: B::CommandPool,
    canvas_image: (B::Memory, B::Image, B::ImageView),
    sampler: B::Sampler,
    render_pass: B::RenderPass,
    desc_set_layout: B::DescriptorSetLayout,
    desc_set_pool: B::DescriptorPool,
    pipeline_layout: B::PipelineLayout,
    pipeline: B::GraphicsPipeline,
    submission_complete_fence: B::Fence,
    rendering_complete_semaphore: B::Semaphore,
}

pub struct Renderer<'a, B: SupportedBackend> {
    context: &'a mut DrawContext<B>,
    clear_color: Color,
    framebuffer: Option<(
        B::Framebuffer,
        <B::Surface as PresentationSurface<B>>::SwapchainImage,
        Viewport,
    )>,
}

impl<'a, B: SupportedBackend> Renderer<'a, B> {
    pub fn blit(self, image: &image::RgbaImage) {
        let Resources {
            command_pool,
            canvas_image,
            sampler,
            ..
        } = &mut *self.context.resources;

        let dims = image.dimensions();
        if dims != self.context.canvas_image_size {
            let replacement_image = unsafe {
                use hal::format::Aspects;
                use hal::image::Usage;

                gfx::make_image::<B>(
                    &self.context.device,
                    &self.context.adapter.physical_device,
                    dims,
                    texture_format(self.context.surface_color_format),
                    Usage::SAMPLED | Usage::TRANSFER_DST,
                    Aspects::COLOR,
                )
            };

            let old = std::mem::replace(canvas_image, replacement_image);
            unsafe {
                let (mem, img, view) = old;
                self.context.device.destroy_image_view(view);
                self.context.device.destroy_image(img);
                self.context.device.free_memory(mem);
            }

            unsafe {
                use hal::buffer::SubRange;
                use hal::pso::{Descriptor, DescriptorSetWrite};

                let descriptors = over([
                    Descriptor::Image(&canvas_image.2, hal::image::Layout::Undefined),
                    Descriptor::Sampler(sampler),
                ]);

                self.context
                    .device
                    .write_descriptor_set(DescriptorSetWrite {
                        set: &mut self.context.desc_set,
                        binding: 0,
                        array_offset: 0,
                        descriptors,
                    });
            }
        }

        unsafe {
            let extent = self.context.surface_extent;
            gfx::upload_image::<B>(
                &self.context.device,
                &self.context.adapter.physical_device,
                command_pool,
                &mut self.context.queue_group.queues[0],
                &canvas_image.1,
                image.dimensions(),
                image,
            );
        }
    }
}

impl<'a, B: SupportedBackend> Drop for Renderer<'a, B> {
    fn drop(&mut self) {
        let Resources {
            command_pool,
            surface,
            submission_complete_fence,
            rendering_complete_semaphore,
            pipeline_layout,
            pipeline,
            render_pass,
            ..
        } = &mut *self.context.resources;

        if let Some((framebuffer, surface_image, _)) = self.framebuffer.take() {
            use std::borrow::Borrow;

            unsafe {
                use hal::command::{
                    ClearColor, ClearValue, CommandBuffer, CommandBufferFlags, SubpassContents,
                };

                let canvas_properties = self.context.canvas_config.canvas_properties(
                    [
                        self.context.surface_extent.width,
                        self.context.surface_extent.height,
                    ],
                    self.context.scale_factor,
                );

                self.context
                    .command_buffer
                    .begin_primary(CommandBufferFlags::ONE_TIME_SUBMIT);

                let ([x, y], [w, h]) = canvas_properties.viewport_scissor_rect;
                let rect = hal::pso::Rect { x, y, w, h };

                self.context.command_buffer.set_viewports(
                    0,
                    over([Viewport {
                        rect,
                        depth: 0.0..1.0,
                    }]),
                );
                self.context.command_buffer.set_scissors(0, over([rect]));

                self.context.command_buffer.bind_graphics_descriptor_sets(
                    pipeline_layout,
                    0,
                    over([&self.context.desc_set]),
                    over([]),
                );

                self.context.command_buffer.bind_graphics_pipeline(pipeline);

                self.context.command_buffer.begin_render_pass(
                    render_pass,
                    &framebuffer,
                    rect,
                    over([RenderAttachmentInfo {
                        image_view: surface_image.borrow(),
                        clear_value: ClearValue {
                            color: ClearColor {
                                float32: self.clear_color,
                            },
                        },
                    }]),
                    SubpassContents::Inline,
                );

                #[cfg(not(all(target_arch = "wasm32", feature = "bypass_spirv_cross")))]
                let vertex_range = 0..6;

                #[cfg(all(target_arch = "wasm32", feature = "bypass_spirv_cross"))]
                let vertex_range = 6..12;

                self.context.command_buffer.draw(vertex_range, 0..1);

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
