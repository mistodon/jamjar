#![allow(warnings)]

use std::mem::ManuallyDrop;

use crate::{
    gfx::{self, prelude::*},
    windowing::{
        dpi::{LogicalSize, PhysicalSize},
        window::Window,
    },
};

#[cfg(all(target_arch = "wasm32", not(feature = "opengl")))]
compile_error!("Web builds (wasm32) require the `opengl` feature to be enabled.");

#[cfg(not(target_arch = "wasm32"))]
const SHADER_SOURCES: (&'static [u8], &'static [u8]) = (
    include_bytes!("../assets/shaders/compiled/sloth.vert.spv"),
    include_bytes!("../assets/shaders/compiled/sloth.frag.spv"),
);

#[cfg(target_arch = "wasm32")]
const SHADER_SOURCES: (&'static [u8], &'static [u8]) = (
    include_bytes!("../assets/shaders/compiled/sloth.es.vert"),
    include_bytes!("../assets/shaders/compiled/sloth.es.frag"),
);

#[cfg(feature = "opengl")]
pub type OpenGL = gfx_backend_gl::Backend;
#[cfg(feature = "metal")]
pub type Metal = gfx_backend_metal::Backend;

#[cfg(feature = "metal")]
pub type Native = Metal;

fn wiperr<T>(_: T) -> () {}

pub trait SupportedBackend: Backend {
    unsafe fn make_shader_module(
        device: &<Self as Backend>::Device,
        source: &[u8],
        is_fragment: bool,
    ) -> <Self as Backend>::ShaderModule {
        debug_assert!(source.len() % 4 == 0, "SPIRV not aligned");
        let spirv = {
            let p = source.as_ptr() as *const u32;
            std::slice::from_raw_parts(p, source.len() / 4)
        };
        device.create_shader_module(spirv).unwrap()
    }
}

#[cfg(feature = "metal")]
impl SupportedBackend for Metal {}

#[cfg(feature = "opengl")]
impl SupportedBackend for OpenGL {
    #[cfg(target_arch = "wasm32")]
    unsafe fn make_shader_module(
        device: &<Self as Backend>::Device,
        source: &[u8],
        is_fragment: bool,
    ) -> <Self as Backend>::ShaderModule {
        let source = std::str::from_utf8_unchecked(source);
        let stage = if is_fragment {
            gfx_auxil::ShaderStage::Fragment
        } else {
            gfx_auxil::ShaderStage::Vertex
        };
        device
            .create_shader_module_from_source(source, stage)
            .expect("Failed to create shader module")
    }
}

struct Resources<B: SupportedBackend> {
    _instance: Option<B::Instance>,
    surface: B::Surface,
    command_pool: B::CommandPool,
    canvas_image: (B::Memory, B::Image, B::ImageView),
    sampler: Option<B::Sampler>,
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
        <B::Surface as PresentationSurface<B>>::SwapchainImage,
        B::Framebuffer,
    )>,
    viewport: gfx_hal::pso::Viewport,
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
                use gfx_hal::format::Aspects;
                use gfx_hal::image::Usage;

                gfx::make_image::<B>(
                    &self.context.device,
                    &self.context.adapter.physical_device,
                    dims,
                    self.context.surface_color_format,
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
                use gfx_hal::buffer::SubRange;
                use gfx_hal::pso::{Descriptor, DescriptorSetWrite};

                let image_write = DescriptorSetWrite {
                    set: &self.context.desc_set,
                    binding: 0,
                    array_offset: 0,
                    descriptors: Some(Descriptor::Image(
                        &canvas_image.2,
                        gfx_hal::image::Layout::Undefined,
                    )),
                };

                #[cfg(target_arch = "wasm32")]
                let writes = vec![image_write];

                #[cfg(not(target_arch = "wasm32"))]
                let writes = vec![
                    image_write,
                    DescriptorSetWrite {
                        set: &self.context.desc_set,
                        binding: 1,
                        array_offset: 0,
                        descriptors: Some(Descriptor::Sampler(sampler.as_ref().unwrap())),
                    },
                ];

                self.context.device.write_descriptor_sets(writes);
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

        if let Some((surface_image, framebuffer)) = self.framebuffer.take() {
            unsafe {
                use gfx_hal::command::{
                    ClearColor, ClearValue, CommandBuffer, CommandBufferFlags, SubpassContents,
                };

                self.context
                    .command_buffer
                    .begin_primary(CommandBufferFlags::ONE_TIME_SUBMIT);

                self.context
                    .command_buffer
                    .set_viewports(0, &[self.viewport.clone()]);
                self.context
                    .command_buffer
                    .set_scissors(0, &[self.viewport.rect]);

                self.context.command_buffer.bind_graphics_descriptor_sets(
                    pipeline_layout,
                    0,
                    vec![&self.context.desc_set],
                    &[],
                );

                // TODO: Has to come before render pass for OpenGL, but after
                // for Metal ...
                self.context.command_buffer.bind_graphics_pipeline(pipeline);

                self.context.command_buffer.begin_render_pass(
                    render_pass,
                    &framebuffer,
                    self.viewport.rect,
                    &[ClearValue {
                        color: ClearColor {
                            float32: self.clear_color,
                        },
                    }],
                    SubpassContents::Inline,
                );

                #[cfg(not(target_arch = "wasm32"))]
                let vertex_range = 0..6;
                #[cfg(target_arch = "wasm32")]
                let vertex_range = 6..12;

                self.context.command_buffer.draw(vertex_range, 0..1);

                self.context.command_buffer.end_render_pass();
                self.context.command_buffer.finish();

                use gfx_hal::queue::{CommandQueue, Submission};

                let submission = Submission {
                    command_buffers: vec![&self.context.command_buffer],
                    wait_semaphores: None,
                    signal_semaphores: vec![&rendering_complete_semaphore],
                };

                self.context.queue_group.queues[0]
                    .submit(submission, Some(&submission_complete_fence));

                let result = self.context.queue_group.queues[0].present(
                    surface,
                    surface_image,
                    Some(&rendering_complete_semaphore),
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
    surface_color_format: gfx_hal::format::Format,
    desc_set: B::DescriptorSet,
    surface_extent: gfx_hal::window::Extent2D,
    swapchain_invalidated: Option<()>,
    canvas_image_size: (u32, u32),
}

impl<B: SupportedBackend> DrawContext<B> {
    pub fn new(window: &Window) -> Result<Self, ()> {
        let instance = B::Instance::create("jamjar_drawsloth", 1).map_err(wiperr)?;
        let surface = unsafe { instance.create_surface(window).map_err(wiperr)? };
        let adapter = instance.enumerate_adapters().remove(0);

        let (device, mut queue_group) = {
            use gfx_hal::queue::QueueFamily;

            let queue_family = adapter
                .queue_families
                .iter()
                .find(|family| {
                    surface.supports_queue_family(family) && family.queue_type().supports_graphics()
                })
                .ok_or(())?;

            let mut gpu = unsafe {
                adapter
                    .physical_device
                    .open(&[(queue_family, &[1.0])], gfx_hal::Features::empty())
                    .expect("Failed to open device")
            };

            (gpu.device, gpu.queue_groups.pop().unwrap())
        };

        let vs_module = unsafe { B::make_shader_module(&device, SHADER_SOURCES.0, false) };
        let fs_module = unsafe { B::make_shader_module(&device, SHADER_SOURCES.1, true) };

        Self::inner_new(
            window,
            Some(instance),
            surface,
            adapter,
            device,
            queue_group,
            vs_module,
            fs_module,
        )
    }

    fn inner_new(
        window: &Window,
        instance: Option<B::Instance>,
        surface: B::Surface,
        adapter: Adapter<B>,
        device: B::Device,
        queue_group: QueueGroup<B>,
        vs_module: B::ShaderModule,
        fs_module: B::ShaderModule,
    ) -> Result<Self, ()> {
        let (mut command_pool, mut command_buffer) = unsafe {
            use gfx_hal::command::Level;
            use gfx_hal::pool::{CommandPool, CommandPoolCreateFlags};

            let mut command_pool = device
                .create_command_pool(queue_group.family, CommandPoolCreateFlags::empty())
                .expect("Out of memory");

            let command_buffer = command_pool.allocate_one(Level::Primary);

            (command_pool, command_buffer)
        };

        let surface_color_format = {
            use gfx_hal::format::{ChannelType, Format};

            let supported_formats = surface
                .supported_formats(&adapter.physical_device)
                .unwrap_or(vec![]);

            let default_format = *supported_formats.get(0).unwrap_or(&Format::Rgba8Srgb);

            supported_formats
                .into_iter()
                .find(|format| format.base_format().1 == ChannelType::Srgb)
                .unwrap_or(default_format)
        };

        let dpi = window.scale_factor();
        let physical_size: PhysicalSize<u32> = window.inner_size();
        let logical_size: LogicalSize<u32> = physical_size.to_logical(dpi);
        let mut surface_extent = gfx_hal::window::Extent2D {
            width: physical_size.width,
            height: physical_size.height,
        };

        let canvas_image_size = logical_size.into();
        let canvas_image = unsafe {
            use gfx_hal::format::{Aspects, Format};
            use gfx_hal::image::Usage;

            gfx::make_image::<B>(
                &device,
                &adapter.physical_device,
                canvas_image_size,
                Format::Rgba8Srgb,
                Usage::SAMPLED | Usage::TRANSFER_DST,
                Aspects::COLOR,
            )
        };

        let sampler = match cfg!(target_arch = "wasm32") {
            true => None,
            false => Some(unsafe {
                use gfx_hal::image::{Filter, SamplerDesc, Usage, WrapMode};

                device
                    .create_sampler(&SamplerDesc::new(Filter::Linear, WrapMode::Tile))
                    .expect("TODO")
            }),
        };

        let render_pass = {
            use gfx_hal::image::Layout;
            use gfx_hal::pass::{
                Attachment, AttachmentLoadOp, AttachmentOps, AttachmentStoreOp, SubpassDesc,
            };

            let color_attachment = Attachment {
                format: Some(surface_color_format),
                samples: 1,
                ops: AttachmentOps::new(AttachmentLoadOp::Clear, AttachmentStoreOp::Store),
                stencil_ops: AttachmentOps::DONT_CARE,
                layouts: Layout::Undefined..Layout::Present,
            };

            let subpass = SubpassDesc {
                colors: &[(0, Layout::ColorAttachmentOptimal)],
                depth_stencil: None,
                inputs: &[],
                resolves: &[],
                preserves: &[],
            };

            unsafe {
                device
                    .create_render_pass(&[color_attachment], &[subpass], &[])
                    .expect("Out of memory")
            }
        };

        let desc_set_layout = unsafe {
            use gfx_hal::pso::{
                BufferDescriptorFormat, BufferDescriptorType, DescriptorSetLayoutBinding,
                DescriptorType, ShaderStageFlags,
            };

            let image_binding = DescriptorSetLayoutBinding {
                binding: 0,
                ty: DescriptorType::Image {
                    ty: gfx_hal::pso::ImageDescriptorType::Sampled {
                        with_sampler: cfg!(target_arch = "wasm32"),
                    },
                },
                count: 1,
                stage_flags: ShaderStageFlags::FRAGMENT,
                immutable_samplers: false,
            };

            #[cfg(target_arch = "wasm32")]
            let bindings = &[image_binding];

            #[cfg(not(target_arch = "wasm32"))]
            let bindings = &[
                image_binding,
                DescriptorSetLayoutBinding {
                    binding: 1,
                    ty: DescriptorType::Sampler,
                    count: 1,
                    stage_flags: ShaderStageFlags::FRAGMENT,
                    immutable_samplers: false,
                },
            ];

            device
                .create_descriptor_set_layout(bindings, &[])
                .expect("TODO")
        };

        let mut desc_set_pool = unsafe {
            use gfx_hal::pso::{
                BufferDescriptorFormat, BufferDescriptorType, DescriptorPoolCreateFlags,
                DescriptorRangeDesc, DescriptorType,
            };

            let image_desc = DescriptorRangeDesc {
                ty: DescriptorType::Image {
                    ty: gfx_hal::pso::ImageDescriptorType::Sampled {
                        with_sampler: cfg!(target_arch = "wasm32"),
                    },
                },
                count: 1,
            };

            #[cfg(target_arch = "wasm32")]
            let desc = &[image_desc];

            #[cfg(not(target_arch = "wasm32"))]
            let desc = &[
                image_desc,
                DescriptorRangeDesc {
                    ty: DescriptorType::Sampler,
                    count: 1,
                },
            ];

            device
                .create_descriptor_pool(1, desc, DescriptorPoolCreateFlags::empty())
                .expect("TODO")
        };

        let desc_set = unsafe {
            use gfx_hal::pso::DescriptorPool;

            desc_set_pool
                .allocate_set(&desc_set_layout)
                .expect("Failed to allocate descriptor set")
        };

        unsafe {
            use gfx_hal::buffer::SubRange;
            use gfx_hal::pso::{Descriptor, DescriptorSetWrite};

            let image_write = DescriptorSetWrite {
                set: &desc_set,
                binding: 0,
                array_offset: 0,
                descriptors: Some(Descriptor::Image(
                    &canvas_image.2,
                    gfx_hal::image::Layout::Undefined,
                )),
            };

            #[cfg(target_arch = "wasm32")]
            let writes = vec![image_write];

            #[cfg(not(target_arch = "wasm32"))]
            let writes = vec![
                image_write,
                DescriptorSetWrite {
                    set: &desc_set,
                    binding: 1,
                    array_offset: 0,
                    descriptors: Some(Descriptor::Sampler(sampler.as_ref().unwrap())),
                },
            ];

            device.write_descriptor_sets(writes);
        }

        let pipeline_layout = unsafe {
            device
                .create_pipeline_layout(vec![&desc_set_layout], &[])
                .expect("Out of memory")
        };

        let (vs_entry, fs_entry) = (
            gfx_hal::pso::EntryPoint {
                entry: "main",
                module: &vs_module,
                specialization: gfx_hal::pso::Specialization::default(),
            },
            gfx_hal::pso::EntryPoint {
                entry: "main",
                module: &fs_module,
                specialization: gfx_hal::pso::Specialization::default(),
            },
        );

        use gfx_hal::pso::{
            AttributeDesc, BlendState, ColorBlendDesc, ColorMask, Element, Face,
            GraphicsPipelineDesc, InputAssemblerDesc, Primitive, PrimitiveAssemblerDesc,
            Rasterizer, VertexBufferDesc, VertexInputRate,
        };

        let primitive_assembler = {
            use gfx_hal::format::Format;

            PrimitiveAssemblerDesc::Vertex {
                buffers: &[],
                attributes: &[],
                input_assembler: InputAssemblerDesc::new(Primitive::TriangleList),
                vertex: vs_entry,
                tessellation: None,
                geometry: None,
            }
        };

        let mut pipeline_desc = GraphicsPipelineDesc::new(
            primitive_assembler,
            Rasterizer {
                cull_face: Face::NONE,
                ..Rasterizer::FILL
            },
            Some(fs_entry),
            &pipeline_layout,
            gfx_hal::pass::Subpass {
                index: 0,
                main_pass: &render_pass,
            },
        );

        pipeline_desc.blender.targets.push(ColorBlendDesc {
            mask: ColorMask::ALL,
            blend: Some(BlendState::REPLACE),
        });

        let pipeline = unsafe {
            let pipeline = device
                .create_graphics_pipeline(&pipeline_desc, None)
                .expect("Failed to create graphics pipeline");

            device.destroy_shader_module(vs_module);
            device.destroy_shader_module(fs_module);
            pipeline
        };

        let submission_complete_fence = device.create_fence(true).expect("Out of memory");
        let rendering_complete_semaphore = device.create_semaphore().expect("Out of memory");

        Ok(DrawContext {
            resources: ManuallyDrop::new(Resources {
                _instance: instance,
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
            desc_set,
            swapchain_invalidated: Some(()),
            canvas_image_size,
        })
    }

    pub fn resolution_changed(&mut self, resolution: (u32, u32)) {
        self.surface_extent = gfx_hal::window::Extent2D {
            width: resolution.0,
            height: resolution.1,
        };
        self.swapchain_invalidated = Some(());
    }

    pub fn scale_factor_changed(&mut self, scale_factor: f64, resolution: (u32, u32)) {
        self.resolution_changed(resolution);
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
            use gfx_hal::pool::CommandPool;

            // We refuse to wait more than a second, to avoid hanging.
            let render_timeout_ns = 1_000_000_000;

            self.device
                .wait_for_fence(&submission_complete_fence, render_timeout_ns)
                .expect("Out of memory or device lost");

            self.device
                .reset_fence(&submission_complete_fence)
                .expect("Out of memory");

            command_pool.reset(false);
        }

        if self.swapchain_invalidated.take().is_some() {
            use gfx_hal::window::SwapchainConfig;

            let caps = surface.capabilities(&self.adapter.physical_device);

            let mut swapchain_config =
                SwapchainConfig::from_caps(&caps, self.surface_color_format, self.surface_extent);

            // This seems to fix some fullscreen slowdown on macOS.
            if caps.image_count.contains(&3) {
                swapchain_config.image_count = 3;
            }

            self.surface_extent = swapchain_config.extent;

            unsafe {
                surface
                    .configure_swapchain(&self.device, swapchain_config)
                    .expect("Failed to configure swapchain");
            };
        }

        // We refuse to wait more than a second, to avoid hanging.
        let acquire_timeout_ns = 1_000_000_000;
        let framebuffer = match unsafe { surface.acquire_image(acquire_timeout_ns) } {
            Ok((surface_image, _)) => Some(unsafe {
                use std::borrow::Borrow;

                use gfx_hal::image::Extent;

                let framebuffer = self
                    .device
                    .create_framebuffer(
                        render_pass,
                        vec![surface_image.borrow()],
                        Extent {
                            width: self.surface_extent.width,
                            height: self.surface_extent.height,
                            depth: 1,
                        },
                    )
                    .unwrap();

                (surface_image, framebuffer)
            }),
            Err(_) => {
                self.swapchain_invalidated = Some(());
                None
            }
        };

        let viewport = {
            use gfx_hal::pso::{Rect, Viewport};

            Viewport {
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: self.surface_extent.width as i16,
                    h: self.surface_extent.height as i16,
                },
                depth: 0.0..1.0,
            }
        };

        let mut renderer = Renderer {
            context: self,
            clear_color,
            framebuffer,
            viewport,
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
            if let Some(sampler) = sampler {
                self.device.destroy_sampler(sampler);
            }
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
