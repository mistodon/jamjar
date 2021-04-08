use super::*;

use crate::utils::over;

pub fn init<B: Backend>(
    window: &crate::windowing::window::Window,
    name: &str,
    version: u32,
) -> Result<
    (
        B::Instance,
        B::Surface,
        Format,
        Adapter<B>,
        B::Device,
        QueueGroup<B>,
        B::CommandPool,
    ),
    &'static str,
> {
    let instance = B::Instance::create(name, version).map_err(|_| "unsupported backend")?;
    let surface = unsafe {
        instance
            .create_surface(window)
            .map_err(|_| "create_surface failed")?
    };
    let adapter = instance.enumerate_adapters().remove(0);

    let surface_color_format = {
        use gfx_hal::format::ChannelType;

        let supported_formats = surface
            .supported_formats(&adapter.physical_device)
            .unwrap_or(vec![]);

        let default_format = *supported_formats.get(0).unwrap_or(&Format::Rgba8Srgb);

        supported_formats
            .into_iter()
            .find(|format| format.base_format().1 == ChannelType::Srgb)
            .unwrap_or(default_format)
    };

    let (device, queue_group) = {
        let queue_family = adapter
            .queue_families
            .iter()
            .find(|family| {
                surface.supports_queue_family(family) && family.queue_type().supports_graphics()
            })
            .ok_or("failed to find queue family")?;

        let mut gpu = unsafe {
            adapter
                .physical_device
                .open(&[(queue_family, &[1.0])], gfx_hal::Features::empty())
                .expect("Failed to open device")
        };

        (gpu.device, gpu.queue_groups.pop().unwrap())
    };

    let command_pool = unsafe {
        use gfx_hal::pool::CommandPoolCreateFlags;

        device
            .create_command_pool(queue_group.family, CommandPoolCreateFlags::empty())
            .expect("out of memory")
    };

    Ok((
        instance,
        surface,
        surface_color_format,
        adapter,
        device,
        queue_group,
        command_pool,
    ))
}

pub fn desc_sets<B: Backend>(
    device: &B::Device,
    values: Vec<(Vec<&B::Buffer>, Vec<&B::ImageView>, Vec<&B::Sampler>)>,
) -> (
    B::DescriptorSetLayout,
    B::DescriptorPool,
    Vec<B::DescriptorSet>,
) {
    use gfx_hal::pso::*;

    let sets = values.len();
    let ubos = values.get(0).map(|set| set.0.len()).unwrap_or(0);
    let images = values.get(0).map(|set| set.1.len()).unwrap_or(0);
    let samplers = values.get(0).map(|set| set.2.len()).unwrap_or(0);

    assert!(values.iter().all(|set| set.0.len() == ubos && set.1.len() == images && set.2.len() == samplers), "All desc_sets must have the same layout of values");

    let mut binding_number = 0;
    let mut bindings = vec![];
    let mut ranges = vec![];
    for _ in 0..ubos {
        bindings.push(DescriptorSetLayoutBinding {
            binding: binding_number,
            ty: DescriptorType::Buffer {
                ty: BufferDescriptorType::Uniform,
                format: BufferDescriptorFormat::Structured {
                    dynamic_offset: false,
                },
            },
            count: 1,
            stage_flags: ShaderStageFlags::FRAGMENT,
            immutable_samplers: false,
        });
        ranges.push(DescriptorRangeDesc {
            ty: DescriptorType::Buffer {
                ty: BufferDescriptorType::Uniform,
                format: BufferDescriptorFormat::Structured {
                    dynamic_offset: false,
                },
            },
            count: sets,
        });
        binding_number += 1;
    }
    for _ in 0..images {
        bindings.push(DescriptorSetLayoutBinding {
            binding: binding_number,
            ty: DescriptorType::Image {
                ty: gfx_hal::pso::ImageDescriptorType::Sampled {
                    with_sampler: false,
                },
            },
            count: 1,
            stage_flags: ShaderStageFlags::FRAGMENT,
            immutable_samplers: false,
        });
        ranges.push(DescriptorRangeDesc {
            ty: DescriptorType::Image {
                ty: gfx_hal::pso::ImageDescriptorType::Sampled {
                    with_sampler: false,
                },
            },
            count: sets,
        });
        binding_number += 1;
    }
    for _ in 0..samplers {
        bindings.push(DescriptorSetLayoutBinding {
            binding: binding_number,
            ty: DescriptorType::Sampler,
            count: 1,
            stage_flags: ShaderStageFlags::FRAGMENT,
            immutable_samplers: false,
        });
        ranges.push(DescriptorRangeDesc {
            ty: DescriptorType::Sampler,
            count: sets,
        });
        binding_number += 1;
    }

    let (layout, pool, mut desc_sets) = unsafe {
        let layout = device
            .create_descriptor_set_layout(bindings.into_iter(), over([]))
            .unwrap();
        let mut pool = device
            .create_descriptor_pool(sets, ranges.into_iter(), DescriptorPoolCreateFlags::empty())
            .unwrap();
        let mut desc_sets = Vec::with_capacity(sets);
        for _ in 0..sets {
            desc_sets.push(pool.allocate_one(&layout).unwrap());
        }
        (layout, pool, desc_sets)
    };

    write_desc_sets::<B>(device, desc_sets.iter_mut().collect(), values);

    (layout, pool, desc_sets)
}

pub fn write_desc_sets<B: Backend>(
    device: &B::Device,
    desc_sets: Vec<&mut B::DescriptorSet>,
    values: Vec<(Vec<&B::Buffer>, Vec<&B::ImageView>, Vec<&B::Sampler>)>,
) {
    use gfx_hal::pso::*;

    assert!(desc_sets.len() == values.len() && !values.is_empty(), "Must supply a matching, non-zero number of desc_sets and values");

    let ubos = values.get(0).map(|set| set.0.len()).unwrap_or(0);
    let images = values.get(0).map(|set| set.1.len()).unwrap_or(0);
    let samplers = values.get(0).map(|set| set.2.len()).unwrap_or(0);

    assert!(values.iter().all(|set| set.0.len() == ubos && set.1.len() == images && set.2.len() == samplers), "All desc_sets must have the same layout of values");

    for (set_values, desc_set) in values.into_iter().zip(desc_sets.into_iter()) {
        use gfx_hal::buffer::SubRange;

        let mut descriptors = Vec::with_capacity(ubos + images + samplers);

        for buffer in set_values.0 {
            descriptors.push(Descriptor::Buffer(buffer, SubRange::WHOLE));
        }
        for image in set_values.1 {
            descriptors.push(Descriptor::Image(
                image,
                gfx_hal::image::Layout::Undefined,
            ));
        }
        for sampler in set_values.2 {
            descriptors.push(Descriptor::Sampler(sampler));
        }

        unsafe {
            if !descriptors.is_empty() {
                device.write_descriptor_set(DescriptorSetWrite {
                    set: desc_set,
                    binding: 0,
                    array_offset: 0,
                    descriptors: descriptors.into_iter(),
                });
            }
        }
    }

}

pub fn render_pass<B: Backend>(
    device: &B::Device,
    surface_color_format: Format,
    depth_format: Option<Format>,
    intermediate: bool,
) -> B::RenderPass {
    use gfx_hal::image::Layout;
    use gfx_hal::pass::{
        Attachment, AttachmentLoadOp, AttachmentOps, AttachmentStoreOp, SubpassDesc,
    };

    let end_layout = if intermediate { Layout::ShaderReadOnlyOptimal } else { Layout::Present };

    let color_attachment = Attachment {
        format: Some(surface_color_format),
        samples: 1,
        ops: AttachmentOps::new(AttachmentLoadOp::Clear, AttachmentStoreOp::Store),
        stencil_ops: AttachmentOps::DONT_CARE,
        layouts: Layout::Undefined..end_layout,
    };

    let depth_attachment = depth_format.map(|surface_depth_format| Attachment {
        format: Some(surface_depth_format),
        samples: 1,
        ops: AttachmentOps::new(AttachmentLoadOp::Clear, AttachmentStoreOp::DontCare),
        stencil_ops: AttachmentOps::new(AttachmentLoadOp::Clear, AttachmentStoreOp::DontCare),
        layouts: Layout::Undefined..Layout::DepthStencilAttachmentOptimal,
    });

    let subpass = SubpassDesc {
        colors: &[(0, Layout::ColorAttachmentOptimal)],
        depth_stencil: depth_format.map(|_| &(1, Layout::DepthStencilAttachmentOptimal)),
        inputs: &[],
        resolves: &[],
        preserves: &[],
    };

    unsafe {
        let attachments = match depth_attachment {
            Some(depth_attachment) => vec![color_attachment, depth_attachment],
            None => vec![color_attachment],
        };
        device
            .create_render_pass(attachments.into_iter(), over([subpass]), over([]))
            .expect("out of memory")
    }
}

pub fn pipeline<B: SupportedBackend>(
    device: &B::Device,
    desc_layout: Option<&B::DescriptorSetLayout>,
    push_constant_size: u32,
    vs_bytes: &[u8],
    fs_bytes: &[u8],
    render_pass: &B::RenderPass,
    depth_format: Option<Format>,
    attribute_sizes: &[u32],
) -> (B::GraphicsPipeline, B::PipelineLayout) {
    use gfx_hal::pso::*;

    let push = vec![(
        ShaderStageFlags::VERTEX | ShaderStageFlags::FRAGMENT,
        0..push_constant_size,
    )];
    let push = if push_constant_size > 0 { push } else { vec![] };

    let pipeline_layout = unsafe {
        device
            .create_pipeline_layout(desc_layout.into_iter(), push.into_iter())
            .expect("out of memory")
    };

    let shader_modules = [(vs_bytes, false), (fs_bytes, true)]
        .iter()
        .map(|&(bytes, is_frag)| unsafe { B::make_shader_module(device, bytes, is_frag) })
        .collect::<Vec<_>>();
    let mut entries = shader_modules.iter().map(|module| EntryPoint::<B> {
        entry: "main",
        module,
        specialization: Default::default(),
    });

    let stride = attribute_sizes.iter().sum::<u32>() * std::mem::size_of::<f32>() as u32;
    let buffer_desc = if stride > 0 {
        vec![VertexBufferDesc {
            binding: 0,
            stride,
            rate: VertexInputRate::Vertex,
        }]
    } else {
        vec![]
    };

    let mut offset = 0;
    let mut attrs = vec![];
    for (index, &size) in attribute_sizes.iter().enumerate() {
        attrs.push(AttributeDesc {
            location: index as u32,
            binding: 0,
            element: Element {
                format: match size {
                    1 => Format::R32Sfloat,
                    2 => Format::Rg32Sfloat,
                    3 => Format::Rgb32Sfloat,
                    4 => Format::Rgba32Sfloat,
                    n => panic!("invalid attribute size {}", n),
                },
                offset,
            },
        });
        offset += size * std::mem::size_of::<f32>() as u32;
    }

    let primitive_assembler = PrimitiveAssemblerDesc::Vertex {
        buffers: &buffer_desc,
        attributes: &attrs,
        input_assembler: InputAssemblerDesc::new(Primitive::TriangleList),
        vertex: entries.next().unwrap(),
        tessellation: None,
        geometry: None,
    };

    let mut pipeline_desc = GraphicsPipelineDesc::new(
        primitive_assembler,
        Rasterizer {
            cull_face: Face::BACK,
            ..Rasterizer::FILL
        },
        entries.next(),
        &pipeline_layout,
        gfx_hal::pass::Subpass {
            index: 0,
            main_pass: &render_pass,
        },
    );

    pipeline_desc.blender.targets.push(ColorBlendDesc {
        mask: ColorMask::ALL,
        blend: Some(BlendState::ALPHA),
    });

    if depth_format.is_some() {
        pipeline_desc.depth_stencil = DepthStencilDesc {
            depth: Some(DepthTest {
                fun: Comparison::LessEqual,
                write: true,
            }),
            depth_bounds: false,
            stencil: None,
        };
    }

    let pipeline = unsafe {
        let pipeline = device
            .create_graphics_pipeline(&pipeline_desc, None)
            .expect("failed to create graphics pipeline");

        for module in shader_modules {
            device.destroy_shader_module(module);
        }
        pipeline
    };

    (pipeline, pipeline_layout)
}

pub fn reconfigure_swapchain<B: Backend>(
    surface: &mut B::Surface,
    adapter: &Adapter<B>,
    device: &B::Device,
    surface_color_format: Format,
    surface_extent: &mut gfx_hal::window::Extent2D,
) -> FramebufferAttachment {
    use gfx_hal::window::SwapchainConfig;

    let caps = surface.capabilities(&adapter.physical_device);

    let mut swapchain_config =
        SwapchainConfig::from_caps(&caps, surface_color_format, *surface_extent);

    let framebuffer_attachment = swapchain_config.framebuffer_attachment();

    // This seems to fix some fullscreen slowdown on macOS.
    if caps.image_count.contains(&3) {
        swapchain_config.image_count = 3;
    }

    *surface_extent = swapchain_config.extent;

    unsafe {
        surface
            .configure_swapchain(device, swapchain_config)
            .expect("failed to configure swapchain");
    };

    framebuffer_attachment
}

// TODO: Remove viewport pls
pub fn acquire_framebuffer<B: Backend>(
    device: &B::Device,
    surface: &mut B::Surface,
    surface_extent: &gfx_hal::window::Extent2D,
    render_pass: &B::RenderPass,
    framebuffer_attachment: gfx_hal::image::FramebufferAttachment,
) -> Result<
    (
        B::Framebuffer,
        <B::Surface as PresentationSurface<B>>::SwapchainImage,
        gfx_hal::pso::Viewport,
    ),
    (),
> {
    let acquire_timeout_ns = 1_000_000_000;
    match unsafe { surface.acquire_image(acquire_timeout_ns) } {
        Ok((surface_image, _)) => unsafe {
            use gfx_hal::image::Extent;

            let framebuffer = device
                .create_framebuffer(
                    render_pass,
                    over([framebuffer_attachment]),
                    Extent {
                        width: surface_extent.width,
                        height: surface_extent.height,
                        depth: 1,
                    },
                )
                .unwrap();

            let viewport = {
                use gfx_hal::pso::Rect;

                Viewport {
                    rect: Rect {
                        x: 0,
                        y: 0,
                        w: surface_extent.width as i16,
                        h: surface_extent.height as i16,
                    },
                    depth: 0.0..1.0,
                }
            };

            Ok((framebuffer, surface_image, viewport))
        },
        Err(_) => Err(()),
    }
}
