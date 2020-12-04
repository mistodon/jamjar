pub mod prelude {
    pub use gfx_hal::{
        adapter::{Adapter, PhysicalDevice},
        device::Device,
        queue::QueueGroup,
        window::{PresentationSurface, Surface},
        Backend, Instance as _,
    };

    pub type Color = [f32; 4];
}

use prelude::*;

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

    let texture_fence = device.create_fence(false).expect("TODO");

    let limits = physical_device.limits();
    let non_coherent_alignment = limits.non_coherent_atom_size as u64;
    let row_alignment_mask = limits.optimal_buffer_copy_pitch_alignment as u32 - 1;

    let image_stride = 4usize;
    let row_pitch = (image_width * image_stride as u32 + row_alignment_mask) & !row_alignment_mask;
    let upload_size = (image_height * row_pitch) as u64;
    let padded_upload_size = ((upload_size + non_coherent_alignment - 1) / non_coherent_alignment)
        * non_coherent_alignment;

    let (buffer_memory, buffer) = make_buffer::<B>(
        device,
        physical_device,
        padded_upload_size as usize,
        gfx_hal::buffer::Usage::TRANSFER_SRC,
        Properties::CPU_VISIBLE,
    );

    let mapped_memory = device
        .map_memory(&buffer_memory, Segment::ALL)
        .expect("TODO");

    for y in 0..image_height as usize {
        let row = &(*image_bytes)[y * (image_width as usize) * image_stride
            ..(y + 1) * (image_width as usize) * image_stride];
        std::ptr::copy_nonoverlapping(
            row.as_ptr(),
            mapped_memory.offset(y as isize * row_pitch as isize),
            image_width as usize * image_stride,
        );
    }

    device
        .flush_mapped_memory_ranges(vec![(&buffer_memory, Segment::ALL)])
        .expect("TODO");

    device.unmap_memory(&buffer_memory);

    // TODO: Commands to transfer data
    let command_buffer = {
        use gfx_hal::command::{BufferImageCopy, CommandBufferFlags, Level};
        use gfx_hal::image::{Access, Extent, Layout, Offset, SubresourceLayers};
        use gfx_hal::memory::{Barrier, Dependencies};
        use gfx_hal::pool::CommandPool;
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
            &[image_barrier],
        );

        command_buffer.copy_buffer_to_image(
            &buffer,
            image_resource,
            Layout::TransferDstOptimal,
            &[BufferImageCopy {
                buffer_offset: 0,
                buffer_width: row_pitch / (image_stride as u32),
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
            }],
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
            &[image_barrier],
        );

        command_buffer.finish();
        command_buffer
    };

    use gfx_hal::queue::CommandQueue;
    queue.submit_without_semaphores(vec![&command_buffer], Some(&texture_fence));

    // TODO: Don't wait forever
    device.wait_for_fence(&texture_fence, !0).expect("TODO");

    use gfx_hal::command::CommandBuffer;
    // command_buffer.reset(true); // TODO: Why does this crash on DX12?

    // Cleanup staging resources
    device.destroy_buffer(buffer);
    device.free_memory(buffer_memory);
    device.destroy_fence(texture_fence);
}
