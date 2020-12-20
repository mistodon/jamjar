use super::*;

pub fn init<B: Backend>(
    window: &crate::windowing::window::Window,
    name: &str,
    version: u32,
) -> Result<
    (
        B::Instance,
        B::Surface,
        gfx_hal::format::Format,
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

    let (device, queue_group) = {
        use gfx_hal::queue::QueueFamily;

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
    sets: usize,
    ubos: usize,
    images: usize,
    samplers: usize,
    values: Vec<(Vec<B::Buffer>, Vec<B::ImageView>, Vec<B::Sampler>)>,
) -> (
    B::DescriptorSetLayout,
    B::DescriptorPool,
    Vec<B::DescriptorSet>,
) {
    use gfx_hal::pso::*;

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
            count: sets,
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
            count: sets,
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
            count: sets,
            stage_flags: ShaderStageFlags::FRAGMENT,
            immutable_samplers: false,
        });
        ranges.push(DescriptorRangeDesc {
            ty: DescriptorType::Sampler,
            count: sets,
        });
        binding_number += 1;
    }

    let (layout, pool, desc_sets) = unsafe {
        let layout = device.create_descriptor_set_layout(bindings, &[]).unwrap();
        let mut pool = device
            .create_descriptor_pool(sets, ranges, DescriptorPoolCreateFlags::empty())
            .unwrap();
        let mut desc_sets = Vec::with_capacity(sets);
        for _ in 0..sets {
            desc_sets.push(pool.allocate_set(&layout).unwrap());
        }
        (layout, pool, desc_sets)
    };

    let mut writes = vec![];
    for (set_number, desc_set) in desc_sets.iter().enumerate() {
        use gfx_hal::buffer::SubRange;

        let mut binding_number = 0;
        for i in 0..ubos {
            writes.push(DescriptorSetWrite {
                set: desc_set,
                binding: binding_number,
                array_offset: 0,
                descriptors: Some(Descriptor::Buffer(
                    &values[set_number].0[i],
                    SubRange::WHOLE,
                )),
            });
            binding_number += 1;
        }
        for i in 0..images {
            writes.push(DescriptorSetWrite {
                set: desc_set,
                binding: binding_number,
                array_offset: 0,
                descriptors: Some(Descriptor::Image(
                    &values[set_number].1[i],
                    gfx_hal::image::Layout::Undefined,
                )),
            });
            binding_number += 1;
        }
        for i in 0..samplers {
            writes.push(DescriptorSetWrite {
                set: desc_set,
                binding: binding_number,
                array_offset: 0,
                descriptors: Some(Descriptor::Sampler(&values[set_number].2[i])),
            });
            binding_number += 1;
        }
    }

    unsafe {
        device.write_descriptor_sets(writes);
    }

    (layout, pool, desc_sets)
}

pub fn reconfigure_swapchain<B: Backend>(
    surface: &mut B::Surface,
    adapter: &Adapter<B>,
    device: &B::Device,
    surface_color_format: gfx_hal::format::Format,
    surface_extent: &mut gfx_hal::window::Extent2D,
) {
    use gfx_hal::window::SwapchainConfig;

    let caps = surface.capabilities(&adapter.physical_device);

    let mut swapchain_config =
        SwapchainConfig::from_caps(&caps, surface_color_format, *surface_extent);

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
}

// TODO: take shaders, attributes, push constant layout, desc set layout
pub fn create_pipeline<B: Backend>() {
}

pub fn acquire_framebuffer<B: Backend>(
    device: &B::Device,
    surface: &mut B::Surface,
    surface_extent: &gfx_hal::window::Extent2D,
    render_pass: &B::RenderPass,
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
            use std::borrow::Borrow;

            use gfx_hal::image::Extent;

            let framebuffer = device
                .create_framebuffer(
                    render_pass,
                    vec![surface_image.borrow()],
                    Extent {
                        width: surface_extent.width,
                        height: surface_extent.height,
                        depth: 1,
                    },
                )
                .unwrap();

            let viewport = {
                use gfx_hal::pso::{Rect, Viewport};

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
