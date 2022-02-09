use crate::{draw::backend, utils::over};

pub mod prelude {
    pub use gfx_hal as hal;
    pub use gfx_hal::{
        adapter::{Adapter, PhysicalDevice},
        command::RenderAttachmentInfo,
        device::Device,
        format::Format,
        image::FramebufferAttachment,
        pso::{ShaderStageFlags, Viewport},
        queue::QueueGroup,
        window::{PresentationSurface, Surface},
        Backend, Instance as _,
    };
    pub use hal::prelude::*;

    pub type Color = [f32; 4];
}

pub mod easy;

use prelude::*;

pub trait SupportedBackend: Backend {
    unsafe fn make_shader_module(
        device: &<Self as Backend>::Device,
        source: &[u8],
        _is_fragment: bool,
    ) -> <Self as Backend>::ShaderModule {
        debug_assert!(source.len() % 4 == 0, "SPIRV not aligned");
        let spirv = {
            let p = source.as_ptr() as *const u32;
            std::slice::from_raw_parts(p, source.len() / 4)
        };
        device.create_shader_module(spirv).unwrap()
    }
}

#[cfg(feature = "dx12")]
impl SupportedBackend for backend::Dx12 {}

#[cfg(feature = "metal")]
impl SupportedBackend for backend::Metal {}

#[cfg(feature = "vulkan")]
impl SupportedBackend for backend::Vulkan {}

#[cfg(feature = "opengl")]
impl SupportedBackend for backend::OpenGL {
    #[cfg(all(target_arch = "wasm32", feature = "bypass_spirv_cross"))]
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

pub unsafe fn make_buffer<B: Backend>(
    device: &B::Device,
    physical_device: &B::PhysicalDevice,
    buffer_len: usize,
    usage: gfx_hal::buffer::Usage,
    properties: gfx_hal::memory::Properties,
) -> (B::Memory, B::Buffer) {
    use gfx_hal::MemoryTypeId;

    let mut buffer = device
        .create_buffer(buffer_len as u64, usage)
        .expect("Failed to create buffer");

    let req = device.get_buffer_requirements(&buffer);

    let memory_types = physical_device.memory_properties().memory_types;

    let memory_type = memory_types
        .iter()
        .enumerate()
        .find(|(id, mem_type)| {
            let type_supported = req.type_mask & (1_u32 << id) != 0;
            type_supported && mem_type.properties.contains(properties)
        })
        .map(|(id, _ty)| MemoryTypeId(id))
        .expect("No compatible memory type available");

    let buffer_memory = device
        .allocate_memory(memory_type, req.size)
        .expect("Failed to allocate buffer memory");

    device
        .bind_buffer_memory(&buffer_memory, 0, &mut buffer)
        .expect("Failed to bind buffer memory");

    (buffer_memory, buffer)
}

pub unsafe fn make_image<B: Backend>(
    device: &B::Device,
    physical_device: &B::PhysicalDevice,
    image_size: (u32, u32),
    format: gfx_hal::format::Format,
    usage: gfx_hal::image::Usage,
    aspects: gfx_hal::format::Aspects,
) -> (B::Memory, B::Image, B::ImageView) {
    use gfx_hal::format::Swizzle;
    use gfx_hal::image::{Kind, SubresourceRange, Tiling, ViewCapabilities, ViewKind};
    use gfx_hal::memory::Properties;

    let (width, height) = image_size;
    let image_kind = Kind::D2(width, height, 1, 1);

    let mut image = device
        .create_image(
            image_kind,
            1,
            format,
            Tiling::Optimal,
            usage,
            ViewCapabilities::empty(),
        )
        .expect("TODO");

    let req = device.get_image_requirements(&image);
    let memory_types = physical_device.memory_properties().memory_types;
    let device_type = memory_types
        .iter()
        .enumerate()
        .position(|(id, memory_type)| {
            req.type_mask & (1 << id) != 0
                && memory_type.properties.contains(Properties::DEVICE_LOCAL)
        })
        .unwrap()
        .into();

    let image_memory = device.allocate_memory(device_type, req.size).expect("TODO");

    device
        .bind_image_memory(&image_memory, 0, &mut image)
        .expect("TODO");

    let image_view = device
        .create_image_view(
            &image,
            ViewKind::D2,
            format,
            Swizzle::NO,
            SubresourceRange {
                aspects,
                level_start: 0,
                level_count: None,
                layer_start: 0,
                layer_count: None,
            },
        )
        .expect("Failed to create image view");

    (image_memory, image, image_view)
}

pub(crate) unsafe fn upload_image_part<B: Backend>(
    device: &B::Device,
    physical_device: &B::PhysicalDevice,
    command_pool: &mut B::CommandPool,
    queue: &mut B::CommandQueue,
    image_resource: &B::Image,
    image_width: u32,
    row_range: std::ops::RangeInclusive<u32>,
    row_bytes: &[u8],
) {
    use gfx_hal::format::Aspects;
    use gfx_hal::image::SubresourceRange;
    use gfx_hal::memory::{Properties, Segment};

    let mut texture_fence = device.create_fence(false).expect("TODO");

    fn pad_to_align(n: u64, align: u64) -> u64 {
        debug_assert!(
            align.is_power_of_two(),
            "Cannot align to non-power-of-two value."
        );
        let mask = align - 1;
        (n + mask) & !mask
    }

    let row_count = row_range.end() - row_range.start() + 1;

    let limits = physical_device.limits();
    let non_coherent_alignment = limits.non_coherent_atom_size as u64;
    let row_alignment = limits.optimal_buffer_copy_pitch_alignment;

    let pixel_size = 4usize;
    let row_size = pad_to_align(image_width as u64 * pixel_size as u64, row_alignment) as u32;
    let upload_size = (row_count * row_size) as u64;
    let padded_upload_size = pad_to_align(upload_size, non_coherent_alignment);

    let (mut buffer_memory, buffer) = make_buffer::<B>(
        device,
        physical_device,
        padded_upload_size as usize,
        gfx_hal::buffer::Usage::TRANSFER_SRC,
        Properties::CPU_VISIBLE,
    );

    let mapped_memory = device
        .map_memory(&mut buffer_memory, Segment::ALL)
        .expect("TODO");

    for y in 0..row_count as usize {
        let row = &(*row_bytes)[y * (image_width as usize) * pixel_size
            ..(y + 1) * (image_width as usize) * pixel_size];
        std::ptr::copy_nonoverlapping(
            row.as_ptr(),
            mapped_memory.offset(y as isize * row_size as isize),
            image_width as usize * pixel_size,
        );
    }

    device
        .flush_mapped_memory_ranges(over([(&buffer_memory, Segment::ALL)]))
        .expect("TODO");

    device.unmap_memory(&mut buffer_memory);

    // TODO: Commands to transfer data
    let command_buffer = {
        use gfx_hal::command::{BufferImageCopy, CommandBufferFlags, Level};
        use gfx_hal::image::{Access, Extent, Layout, Offset, SubresourceLayers};
        use gfx_hal::memory::{Barrier, Dependencies};
        use gfx_hal::pso::PipelineStage;

        let mut command_buffer = command_pool.allocate_one(Level::Primary);

        command_buffer.begin_primary(CommandBufferFlags::ONE_TIME_SUBMIT);

        let image_barrier = Barrier::Image {
            states: (Access::empty(), Layout::Undefined)
                ..(Access::TRANSFER_WRITE, Layout::TransferDstOptimal),
            target: image_resource,
            families: None,
            range: SubresourceRange {
                aspects: Aspects::COLOR,
                ..Default::default()
            },
        };

        command_buffer.pipeline_barrier(
            PipelineStage::TOP_OF_PIPE..PipelineStage::TRANSFER,
            Dependencies::empty(),
            over([image_barrier]),
        );

        command_buffer.copy_buffer_to_image(
            &buffer,
            image_resource,
            Layout::TransferDstOptimal,
            over([BufferImageCopy {
                buffer_offset: 0,
                buffer_width: row_size / (pixel_size as u32),
                buffer_height: row_count as u32,
                image_layers: SubresourceLayers {
                    aspects: Aspects::COLOR,
                    level: 0,
                    layers: 0..1,
                },
                image_offset: Offset {
                    x: 0,
                    y: *row_range.start() as i32,
                    z: 0,
                },
                image_extent: Extent {
                    width: image_width,
                    height: row_count,
                    depth: 1,
                },
            }]),
        );

        let image_barrier = Barrier::Image {
            states: (Access::TRANSFER_WRITE, Layout::TransferDstOptimal)
                ..(Access::SHADER_READ, Layout::ShaderReadOnlyOptimal),
            target: image_resource,
            families: None,
            range: SubresourceRange {
                aspects: Aspects::COLOR,
                ..Default::default()
            },
        };

        command_buffer.pipeline_barrier(
            PipelineStage::TRANSFER..PipelineStage::FRAGMENT_SHADER,
            Dependencies::empty(),
            over([image_barrier]),
        );

        command_buffer.finish();
        command_buffer
    };

    queue.submit(
        over([&command_buffer]),
        over([]),
        over([]),
        Some(&mut texture_fence),
    );

    // TODO: Don't wait forever
    device.wait_for_fence(&texture_fence, !0).expect("TODO");

    // Cleanup staging resources
    device.destroy_buffer(buffer);
    device.free_memory(buffer_memory);
    device.destroy_fence(texture_fence);
}

// TODO: Add upload that uploads a subset of rows!
pub unsafe fn upload_image<B: Backend>(
    device: &B::Device,
    physical_device: &B::PhysicalDevice,
    command_pool: &mut B::CommandPool,
    queue: &mut B::CommandQueue,
    image_resource: &B::Image,
    image_size: (u32, u32),
    image_bytes: &[u8],
) {
    use gfx_hal::format::Aspects;
    use gfx_hal::image::SubresourceRange;
    use gfx_hal::memory::{Properties, Segment};

    let (image_width, image_height) = image_size;

    let mut texture_fence = device.create_fence(false).expect("TODO");

    fn pad_to_align(n: u64, align: u64) -> u64 {
        debug_assert!(
            align.is_power_of_two(),
            "Cannot align to non-power-of-two value."
        );
        let mask = align - 1;
        (n + mask) & !mask
    }

    let limits = physical_device.limits();
    let non_coherent_alignment = limits.non_coherent_atom_size as u64;
    let row_alignment = limits.optimal_buffer_copy_pitch_alignment;

    let pixel_size = 4usize;
    let row_size = pad_to_align(image_width as u64 * pixel_size as u64, row_alignment) as u32;
    let upload_size = (image_height * row_size) as u64;
    let padded_upload_size = pad_to_align(upload_size, non_coherent_alignment);

    let (mut buffer_memory, buffer) = make_buffer::<B>(
        device,
        physical_device,
        padded_upload_size as usize,
        gfx_hal::buffer::Usage::TRANSFER_SRC,
        Properties::CPU_VISIBLE,
    );

    let mapped_memory = device
        .map_memory(&mut buffer_memory, Segment::ALL)
        .expect("TODO");

    for y in 0..image_height as usize {
        let row = &(*image_bytes)[y * (image_width as usize) * pixel_size
            ..(y + 1) * (image_width as usize) * pixel_size];
        std::ptr::copy_nonoverlapping(
            row.as_ptr(),
            mapped_memory.offset(y as isize * row_size as isize),
            image_width as usize * pixel_size,
        );
    }

    device
        .flush_mapped_memory_ranges(over([(&buffer_memory, Segment::ALL)]))
        .expect("TODO");

    device.unmap_memory(&mut buffer_memory);

    // TODO: Commands to transfer data
    let command_buffer = {
        use gfx_hal::command::{BufferImageCopy, CommandBufferFlags, Level};
        use gfx_hal::image::{Access, Extent, Layout, Offset, SubresourceLayers};
        use gfx_hal::memory::{Barrier, Dependencies};
        use gfx_hal::pso::PipelineStage;

        let mut command_buffer = command_pool.allocate_one(Level::Primary);

        command_buffer.begin_primary(CommandBufferFlags::ONE_TIME_SUBMIT);

        let image_barrier = Barrier::Image {
            states: (Access::empty(), Layout::Undefined)
                ..(Access::TRANSFER_WRITE, Layout::TransferDstOptimal),
            target: image_resource,
            families: None,
            range: SubresourceRange {
                aspects: Aspects::COLOR,
                ..Default::default()
            },
        };

        command_buffer.pipeline_barrier(
            PipelineStage::TOP_OF_PIPE..PipelineStage::TRANSFER,
            Dependencies::empty(),
            over([image_barrier]),
        );

        command_buffer.copy_buffer_to_image(
            &buffer,
            image_resource,
            Layout::TransferDstOptimal,
            over([BufferImageCopy {
                buffer_offset: 0,
                buffer_width: row_size / (pixel_size as u32),
                buffer_height: image_height as u32,
                image_layers: SubresourceLayers {
                    aspects: Aspects::COLOR,
                    level: 0,
                    layers: 0..1,
                },
                image_offset: Offset { x: 0, y: 0, z: 0 },
                image_extent: Extent {
                    width: image_width,
                    height: image_height,
                    depth: 1,
                },
            }]),
        );

        let image_barrier = Barrier::Image {
            states: (Access::TRANSFER_WRITE, Layout::TransferDstOptimal)
                ..(Access::SHADER_READ, Layout::ShaderReadOnlyOptimal),
            target: image_resource,
            families: None,
            range: SubresourceRange {
                aspects: Aspects::COLOR,
                ..Default::default()
            },
        };

        command_buffer.pipeline_barrier(
            PipelineStage::TRANSFER..PipelineStage::FRAGMENT_SHADER,
            Dependencies::empty(),
            over([image_barrier]),
        );

        command_buffer.finish();
        command_buffer
    };

    queue.submit(
        over([&command_buffer]),
        over([]),
        over([]),
        Some(&mut texture_fence),
    );

    // TODO: Don't wait forever
    device.wait_for_fence(&texture_fence, !0).expect("TODO");

    // Cleanup staging resources
    device.destroy_buffer(buffer);
    device.free_memory(buffer_memory);
    device.destroy_fence(texture_fence);
}

pub unsafe fn push_constant_bytes<T>(push_constants: &T) -> &[u32] {
    let size_in_bytes = std::mem::size_of::<T>();
    let push_constant_size = std::mem::size_of::<u32>();
    assert!(
        size_in_bytes % push_constant_size == 0,
        "push constant struct not a multiple of four bytes"
    );
    let size_in_u32s = size_in_bytes / push_constant_size;
    let start_ptr = push_constants as *const T as *const u32;
    std::slice::from_raw_parts(start_ptr, size_in_u32s)
}

pub fn srgb_to_linear(color: [f32; 4]) -> [f32; 4] {
    const FACTOR: f32 = 2.2;
    let [r, g, b, a] = color;
    [r.powf(FACTOR), g.powf(FACTOR), b.powf(FACTOR), a]
}
