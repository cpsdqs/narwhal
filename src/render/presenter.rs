use crate::data::{ACES_CG, SRGB};
use crate::platform::NarwhalSurface;
use crate::render::fx::ColorTransform;
use crate::render::swapchain_renderer::SwapchainRenderer;
use crate::render::{Texture, COLOR_FORMAT};
use failure::Error;
use lcms_prime::{Intent, Profile, Transform};
use std::sync::Arc;
use vulkano::command_buffer::{
    AutoCommandBuffer, AutoCommandBufferBuilder, CommandBufferExecFuture,
};
use vulkano::device::{Device, DeviceCreationError, DeviceExtensions, Features, Queue};
use vulkano::image::{Dimensions, ImageUsage, StorageImage, SwapchainImage};
use vulkano::instance::{Instance, PhysicalDevice};
use vulkano::swapchain::{
    self, AcquireError, ColorSpace, PresentFuture, PresentMode, Surface, SurfaceTransform,
    Swapchain, SwapchainAcquireFuture,
};
use vulkano::sync::GpuFuture;

/// Errors that may occur when presenting a frame.
#[derive(Debug, Fail)]
pub enum PresentError {
    /// An arbitrary internal error.
    #[fail(display = "internal error: {}", _0)]
    Internal(Arc<Error>),
}

impl From<Error> for PresentError {
    fn from(err: Error) -> PresentError {
        PresentError::Internal(Arc::new(err))
    }
}

/// Presents a texture on a narwhal surface.
///
/// Assumes sRGB output by default.
pub struct Presenter {
    device: Arc<Device>,
    queue: Arc<Queue>,
    phys_dev: usize,
    surface: Arc<Surface<NarwhalSurface>>,
    swapchain: Arc<Swapchain<NarwhalSurface>>,
    chain_images: Vec<Arc<SwapchainImage<NarwhalSurface>>>,
    color_transform: ColorTransform,
    color_transform_enabled: bool,
    tex_renderer: SwapchainRenderer,
}

#[derive(Debug, Fail)]
enum ColorTransformEncodeError {
    #[fail(display = "color transform failed: {}", _0)]
    TransformFailed(String),
}

impl Presenter {
    /// Creates a new presenter.
    ///
    /// The output color space will be set to sRGB by default.
    pub fn new(
        phys_dev: &PhysicalDevice,
        surface: Arc<Surface<NarwhalSurface>>,
        device: Arc<Device>,
        queue: Arc<Queue>,
    ) -> Result<Presenter, Error> {
        let caps = surface.capabilities(*phys_dev)?;
        let alpha = caps.supported_composite_alpha.iter().next().unwrap();

        let extent = Self::get_extent(&device, phys_dev.index(), &surface);

        let output_format = if let Some((_, cs)) = caps
            .supported_formats
            .iter()
            .find(|(x, _)| *x == COLOR_FORMAT)
        {
            (COLOR_FORMAT, *cs)
        } else {
            // fallback
            (caps.supported_formats[0].0, ColorSpace::SrgbNonLinear)
        };

        let (swapchain, chain_images) = Swapchain::new(
            Arc::clone(&device),
            Arc::clone(&surface),
            caps.min_image_count,
            output_format.0,
            extent,
            1,
            caps.supported_usage_flags,
            &queue,
            SurfaceTransform::Identity,
            alpha,
            PresentMode::Fifo,
            true,
            None,
        )?;

        let color_transform = ColorTransform::new(Arc::clone(&device), &queue, 1024, (0., 1.))?;
        let tex_renderer = SwapchainRenderer::new(Arc::clone(&device), output_format.0)?;

        let mut presenter = Presenter {
            device,
            queue,
            phys_dev: phys_dev.index(),
            surface,
            swapchain,
            chain_images,
            color_transform,
            color_transform_enabled: true,
            tex_renderer,
        };
        presenter.set_profile(SRGB.clone())?;
        Ok(presenter)
    }

    /// Sets the output color profile.
    pub fn set_profile(&mut self, profile: Profile) -> Result<(), Error> {
        let transform = match Transform::new(&ACES_CG, &profile, Intent::Perceptual) {
            Ok(t) => t,
            Err(err) => return Err(ColorTransformEncodeError::TransformFailed(err).into()),
        };

        // check if the profile is ACEScg
        // FIXME: this is a terrible heuristic
        // sample a few colors and see if the transform is rougly an identity transform
        self.color_transform_enabled = false;
        let samples = [0., 0.1, 0.2, 1., 0.5, 0.9, 0.3, 1., 0.8, 0.2, 0.4, 1.];
        let mut output: Vec<f32> = Vec::new();
        output.resize(samples.len(), 0.);
        transform.convert(&samples, &mut output);
        for (i, (a, b)) in samples.iter().zip(output.iter()).enumerate() {
            if i % 4 == 3 {
                // skip alpha
                continue;
            }
            if (a - b).abs() > 0.0001 {
                self.color_transform_enabled = true;
                break;
            }
        }

        self.color_transform.set_transform(transform)
    }

    fn get_extent(
        device: &Arc<Device>,
        phys_dev: usize,
        surface: &Arc<Surface<NarwhalSurface>>,
    ) -> [u32; 2] {
        let instance = device.instance();
        // TODO: handle case where physical device disappears
        let phys_dev = PhysicalDevice::from_index(instance, phys_dev)
            .expect("Physical device has disappeared");
        let caps = surface
            .capabilities(phys_dev)
            .expect("Failed to get surface capabilities");

        // extent must equal CALayer size
        #[cfg(target_os = "macos")]
        let extent = caps.current_extent.unwrap_or(caps.min_image_extent);

        #[cfg(target_os = "linux")]
        let extent = {
            let (viewport, resolution) = surface
                .window()
                .new_size
                .lock()
                .unwrap()
                .take()
                .unwrap_or(((100, 100).into(), 1.));

            [
                ((viewport.x as f32 * resolution) as u32)
                    .max(caps.min_image_extent[0])
                    .min(caps.max_image_extent[0]),
                ((viewport.y as f32 * resolution) as u32)
                    .max(caps.min_image_extent[1])
                    .min(caps.max_image_extent[1]),
            ]
        };

        extent
    }

    fn reacquire_swapchain(&mut self) -> Result<(), PresentError> {
        let extent = Self::get_extent(&self.device, self.phys_dev, &self.surface);

        let (new_chain, new_images) = self
            .swapchain
            .recreate_with_dimension(extent)
            .map_err(|e| Error::from(e))?;
        self.swapchain = new_chain;
        self.chain_images = new_images;

        Ok(())
    }

    /// Presents a texture on screen using the given command buffer.
    pub fn present(
        &mut self,
        mut cmd_buffer: AutoCommandBufferBuilder,
        tex: &Texture,
    ) -> Result<
        PresentFuture<
            CommandBufferExecFuture<SwapchainAcquireFuture<NarwhalSurface>, AutoCommandBuffer>,
            NarwhalSurface,
        >,
        PresentError,
    > {
        #[cfg(target_os = "linux")]
        {
            if self.surface.window().new_size.lock().unwrap().is_some() {
                self.reacquire_swapchain()?;
            }
        }

        let (index, acq) = match swapchain::acquire_next_image(Arc::clone(&self.swapchain), None) {
            Ok(v) => v,
            Err(AcquireError::OutOfDate) => {
                self.reacquire_swapchain()?;
                // the following may error when not rendering from the main thread
                swapchain::acquire_next_image(Arc::clone(&self.swapchain), None)
                    .map_err(|e| PresentError::Internal(Arc::new(e.into())))?
            }
            Err(e) => return Err(Error::from(e).into()),
        };

        let surf_image = &self.chain_images[index];
        let size = surf_image.dimensions();

        if self.color_transform_enabled {
            // TODO: don't recreate this every frame
            let intermediate = StorageImage::with_usage(
                Arc::clone(&self.device),
                Dimensions::Dim2d {
                    width: size[0],
                    height: size[1],
                },
                COLOR_FORMAT,
                ImageUsage {
                    sampled: true,
                    storage: true,
                    ..ImageUsage::none()
                },
                Some(self.queue.family()),
            )
            .map_err(|e| Error::from(e))?;

            cmd_buffer = self
                .color_transform
                .dispatch(cmd_buffer, tex, &intermediate)?;

            cmd_buffer = self.tex_renderer.render(
                cmd_buffer,
                &Texture::Storage(intermediate),
                surf_image,
            )?;
        } else {
            cmd_buffer = self.tex_renderer.render(cmd_buffer, tex, surf_image)?;
        }

        let cmd_buffer = cmd_buffer.build().map_err(|e| Error::from(e))?;

        Ok(acq
            .then_execute(Arc::clone(&self.queue), cmd_buffer)
            .map_err(|e| Error::from(e))?
            .then_swapchain_present(Arc::clone(&self.queue), Arc::clone(&self.swapchain), index))
    }
}

/// Errors that can occur when choosing a device.
#[derive(Debug, Fail)]
pub enum DeviceRetrievalError {
    /// No suitable device was found.
    #[fail(display = "no suitable device found")]
    NoSuitableDevice,

    /// There was a creation error.
    #[fail(display = "creation error: {}", _0)]
    CreationError(DeviceCreationError),
}

impl From<DeviceCreationError> for DeviceRetrievalError {
    fn from(err: DeviceCreationError) -> DeviceRetrievalError {
        DeviceRetrievalError::CreationError(err)
    }
}

impl Presenter {
    /// Chooses and creates suitable device.
    pub fn choose_device(
        instance: &Arc<Instance>,
    ) -> Result<(usize, Arc<Device>, Arc<Queue>), DeviceRetrievalError> {
        for dev in PhysicalDevice::enumerate(instance) {
            if let Some(queue_family) = dev
                .queue_families()
                .find(|q| q.supports_graphics() && q.supports_compute())
            {
                debug!(target: "narwhal", "Using device {}", dev.name());

                let (device, mut queues) = Device::new(
                    dev,
                    &Features { ..Features::none() },
                    &DeviceExtensions {
                        khr_swapchain: true,
                        ..DeviceExtensions::none()
                    },
                    [(queue_family, 0.5)].iter().cloned(),
                )?;

                return Ok((dev.index(), device, queues.next().expect("No device queue")));
            }
        }

        Err(DeviceRetrievalError::NoSuitableDevice)
    }
}
