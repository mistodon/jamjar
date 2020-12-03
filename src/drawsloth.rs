#![allow(warnings)]
use std::mem::ManuallyDrop;

use crate::{gfx::*, windowing::window::Window};

fn wiperr<T>(_: T) -> () {}

struct Resources<B: Backend> {
    _instance: Option<B::Instance>,
    surface: B::Surface,
}

pub struct DrawContext<B: Backend> {
    resources: ManuallyDrop<Resources<B>>,
    adapter: Adapter<B>,
    device: B::Device,
    queue_group: QueueGroup<B>,
}

#[cfg(target_arch = "wasm32")]
impl DrawContext<gfx_backend_gl::Backend> {
    pub fn new(window: &Window) -> Result<Self, ()> {
        let surface = gfx_backend_gl::Surface::from_raw_handle(window);
        let adapter = surface.enumerate_adapters().remove(0);

        Self::inner_new(window, None, surface, adapter)
    }
}

impl<B: Backend> DrawContext<B> {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(window: &Window) -> Result<Self, ()> {
        let instance = B::Instance::create("jamjar_drawsloth", 1).map_err(wiperr)?;
        let surface = unsafe { instance.create_surface(window).map_err(wiperr)? };
        let adapter = instance.enumerate_adapters().remove(0);

        Self::inner_new(window, Some(instance), surface, adapter)
    }

    fn inner_new(
        window: &Window,
        instance: Option<B::Instance>,
        surface: B::Surface,
        adapter: Adapter<B>,
    ) -> Result<Self, ()> {
        let (device, mut queue_group) = {
            use gfx_hal::queue::QueueFamily;
            use gfx_hal::window::Surface;

            let queue_family = adapter
                .queue_families
                .iter()
                .find(|family| {
                    surface.supports_queue_family(family) && family.queue_type().supports_graphics()
                })
                .ok_or(())?;

            let mut gpu = unsafe {
                use gfx_hal::adapter::PhysicalDevice;

                adapter
                    .physical_device
                    .open(&[(queue_family, &[1.0])], gfx_hal::Features::empty())
                    .expect("Failed to open device")
            };

            (gpu.device, gpu.queue_groups.pop().unwrap())
        };

        Ok(DrawContext {
            resources: ManuallyDrop::new(Resources {
                _instance: instance,
                surface,
            }),
            adapter,
            device,
            queue_group,
        })
    }

    pub fn start_rendering(&mut self, clear_color: Color) -> Renderer<B> {
        let mut renderer = Renderer {
            _p: std::marker::PhantomData,
        };
        renderer.init(clear_color);
        renderer
    }
}

impl<B: Backend> Drop for DrawContext<B> {
    fn drop(&mut self) {
        unsafe {
            let Resources { _instance, surface } = ManuallyDrop::take(&mut self.resources);

            if let Some(instance) = _instance {
                instance.destroy_surface(surface);
            }
        }
    }
}

pub struct Renderer<B: Backend> {
    _p: std::marker::PhantomData<B>,
}

impl<B: Backend> Renderer<B> {
    fn init(&mut self, clear_color: Color) {}

    pub fn blit(self, image: image::RgbaImage) -> Result<(), ()> {
        Ok(())
    }
}

impl<B: Backend> Drop for Renderer<B> {
    fn drop(&mut self) {}
}
