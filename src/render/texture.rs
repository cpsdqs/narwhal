use crate::eval::{EvalError, EvalResult};
use crate::render::{COLOR_FORMAT, DEPTH_FORMAT};
use cgmath::{Matrix4, SquareMatrix, Vector2};
use failure::Error;
use fnv::FnvHashMap;
use std::sync::Arc;
use std::{fmt, mem};
use vulkano::device::{Device, Queue};
use vulkano::format::Format;
use vulkano::image::{self, AttachmentImage, Dimensions, ImageUsage, StorageImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum TexType {
    Attachment,
    Storage,
}

// TODO: also make this per-node so data can be cached
pub(crate) struct TexturePool {
    device: Arc<Device>,
    queue: Arc<Queue>,
    sizes: FnvHashMap<(u32, u32, TexType), Vec<TextureRef>>,
    texture_id_counter: u64,
}

impl TexturePool {
    pub fn new(device: Arc<Device>, queue: Arc<Queue>) -> TexturePool {
        TexturePool {
            device,
            queue,
            sizes: FnvHashMap::default(),
            texture_id_counter: 0,
        }
    }

    pub fn drop_unused(&mut self) {
        let sizes = mem::replace(&mut self.sizes, unsafe { mem::uninitialized() });
        let new_sizes = sizes
            .into_iter()
            .map(|(k, pool)| {
                (
                    k,
                    pool.into_iter()
                        .filter(|x| x.is_shared())
                        .collect::<Vec<_>>(),
                )
            })
            .filter(|(_, pool)| !pool.is_empty())
            .collect();
        mem::forget(mem::replace(&mut self.sizes, new_sizes));
    }

    /// Retrieves a free attachment from the pool or creates a new one otherwise.
    pub fn attachment(
        &mut self,
        width: f32,
        height: f32,
        resolution: f32,
    ) -> Result<TextureRef, Error> {
        self.texture(width, height, resolution, TexType::Attachment)
    }

    /// Retrieves a free storage texture from the pool or creates a new one otherwise.
    pub fn storage(
        &mut self,
        width: f32,
        height: f32,
        resolution: f32,
    ) -> Result<TextureRef, Error> {
        self.texture(width, height, resolution, TexType::Storage)
    }

    fn texture(
        &mut self,
        width: f32,
        height: f32,
        resolution: f32,
        ty: TexType,
    ) -> Result<TextureRef, Error> {
        let px_width = (width * resolution) as u32;
        let px_height = (height * resolution) as u32;
        let key = (px_width, px_height, ty);

        if self.sizes.contains_key(&key) {
            if let Some(pool_textures) = self.sizes.get(&key) {
                for pool_texture in pool_textures {
                    if !pool_texture.is_shared() {
                        return Ok(pool_texture.clone());
                    }
                }
            }
        }

        let color = match ty {
            TexType::Attachment => Texture::Attachment(AttachmentImage::multisampled_with_usage(
                Arc::clone(&self.device),
                [px_width, px_height],
                1, // vulkano has no support for vkCmdResolveImage yet
                COLOR_FORMAT,
                ImageUsage {
                    sampled: true,
                    ..ImageUsage::none()
                },
            )?),
            TexType::Storage => Texture::Storage(StorageImage::with_usage(
                Arc::clone(&self.device),
                Dimensions::Dim2d {
                    width: px_width,
                    height: px_height,
                },
                COLOR_FORMAT,
                ImageUsage {
                    sampled: true,
                    storage: true,
                    ..ImageUsage::none()
                },
                Some(self.queue.family()),
            )?),
        };

        let depth = match ty {
            TexType::Attachment => Some(Texture::Attachment(
                AttachmentImage::multisampled_with_usage(
                    Arc::clone(&self.device),
                    [px_width, px_height],
                    1, // vulkano has no support for vkCmdResolveImage yet
                    DEPTH_FORMAT,
                    ImageUsage {
                        sampled: true,
                        ..ImageUsage::none()
                    },
                )?,
            )),
            TexType::Storage => Some(Texture::Storage(StorageImage::with_usage(
                Arc::clone(&self.device),
                Dimensions::Dim2d {
                    width: px_width,
                    height: px_height,
                },
                DEPTH_FORMAT,
                ImageUsage {
                    sampled: true,
                    // storage: true, not supported sometimes?
                    ..ImageUsage::none()
                },
                Some(self.queue.family()),
            )?)),
        };

        let tex_ref = TextureRef {
            texture_id: self.texture_id_counter,
            color,
            depth,
            transform: Matrix4::identity(),
            resolution,
        };
        self.texture_id_counter += 1;

        if !self.sizes.contains_key(&key) {
            self.sizes.insert(key, vec![tex_ref]);
        } else {
            self.sizes.get_mut(&key).unwrap().push(tex_ref);
        }

        Ok(self.sizes[&key].last().unwrap().clone())
    }
}

#[derive(Debug, Fail)]
enum TextureConversionError {
    #[fail(display = "texture is not a storage image")]
    IsNotStorageImage,
}

/// A texture.
#[derive(Debug, Clone)]
pub enum Texture {
    Attachment(Arc<AttachmentImage<Format>>),
    Storage(Arc<StorageImage<Format>>),
}

impl Texture {
    pub fn dimensions(&self) -> [u32; 2] {
        match self {
            Texture::Attachment(s) => s.dimensions(),
            Texture::Storage(s) => match s.dimensions() {
                Dimensions::Dim2d { width, height } => [width, height],
                _ => panic!("Invalid storage texture size"),
            },
        }
    }

    pub fn as_storage(&self) -> EvalResult<&Arc<StorageImage<Format>>> {
        match self {
            Texture::Storage(s) => Ok(s),
            _ => Err(EvalError::Internal(Arc::new(
                TextureConversionError::IsNotStorageImage.into(),
            ))),
        }
    }

    fn is_shared(&self) -> bool {
        match self {
            Texture::Attachment(arc) => Arc::strong_count(arc) + Arc::weak_count(arc) > 1,
            Texture::Storage(arc) => Arc::strong_count(arc) + Arc::weak_count(arc) > 1,
        }
    }
}

macro_rules! impl_image_view_access_for_texture {
    ($(fn $id:ident($($argn:ident, $argt:ty),*) -> $ret:ty);+;) => {
        unsafe impl image::ImageViewAccess for Texture {
            $(
                fn $id(&self, $($argn: $argt),*) -> $ret {
                    match self {
                        Texture::Attachment(s) => image::ImageViewAccess::$id(s, $($argn),*),
                        Texture::Storage(s) => image::ImageViewAccess::$id(s, $($argn),*),
                    }
                }
            )+
        }
    }
}

impl_image_view_access_for_texture! {
    fn parent() -> &dyn image::ImageAccess;
    fn dimensions() -> Dimensions;
    fn inner() -> &image::sys::UnsafeImageView;
    fn descriptor_set_storage_image_layout() -> image::ImageLayout;
    fn descriptor_set_combined_image_sampler_layout() -> image::ImageLayout;
    fn descriptor_set_sampled_image_layout() -> image::ImageLayout;
    fn descriptor_set_input_attachment_layout() -> image::ImageLayout;
    fn identity_swizzle() -> bool;
}

/// A texture reference.
///
/// Also contains a transform (not shared).
#[derive(Clone)]
pub struct TextureRef {
    texture_id: u64,
    color: Texture,
    depth: Option<Texture>,
    transform: Matrix4<f32>,
    resolution: f32,
}

impl TextureRef {
    /// Returns the color texture.
    pub fn color(&self) -> &Texture {
        &self.color
    }

    /// True if this also contains a depth texture.
    pub fn has_depth(&self) -> bool {
        self.depth.is_some()
    }

    /// Returns the depth texture.
    pub fn depth(&self) -> Option<&Texture> {
        self.depth.as_ref()
    }

    /// Returns the transform.
    pub fn transform(&self) -> &Matrix4<f32> {
        &self.transform
    }

    /// Returns a mutable reference to the transform.
    pub fn transform_mut(&mut self) -> &mut Matrix4<f32> {
        &mut self.transform
    }

    /// Returns the texture size.
    pub fn size(&self) -> Vector2<f32> {
        let [w, h] = self.color.dimensions();
        (w as f32 / self.resolution, h as f32 / self.resolution).into()
    }

    /// Returns the resolution.
    pub fn resolution(&self) -> f32 {
        self.resolution
    }

    fn is_shared(&self) -> bool {
        self.color.is_shared() || self.depth.as_ref().map_or(false, |depth| depth.is_shared())
    }
}

impl PartialEq for TextureRef {
    fn eq(&self, other: &TextureRef) -> bool {
        self.texture_id == other.texture_id
    }
}

impl fmt::Debug for TextureRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "TextureRef {{ #{}, color, ", self.texture_id)?;
        if self.depth.is_some() {
            write!(f, "depth, ")?;
        } else {
            write!(f, "no depth, ")?;
        }
        write!(
            f,
            "transform: {:?}, resolution: {} }}",
            self.transform, self.resolution
        )
    }
}
