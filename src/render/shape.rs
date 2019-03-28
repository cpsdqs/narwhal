use self::shape_frag::ty::ShapePushConstants;
use self::shape_vert::ty::ShapeUniforms;
use crate::data::{Shape, StrokeWeight};
use crate::render::stroke_tess::{self, TessPoint};
use crate::util::{Interleaved, InterleavedItem};
use cgmath::{InnerSpace, Vector2, Vector3, Zero};
use cgmath::{Matrix4, SquareMatrix};
use failure::Error;
use fnv::{FnvHashMap, FnvHashSet};
use lyon::math::{Point, Transform2D};
use lyon::path::iterator::PathIterator;
use lyon::path::{PathEvent, PathState};
use lyon::tessellation::{
    geometry_builder, FillError, FillOptions, FillTessellator, OnError, VertexBuffers,
};
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Weak};
use std::{f32, mem};
use vulkano::buffer::cpu_pool::{CpuBufferPool, CpuBufferPoolSubbuffer};
use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer, TypedBufferAccess};
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::descriptor::descriptor_set::FixedSizeDescriptorSetsPool;
use vulkano::descriptor::{DescriptorSet, PipelineLayoutAbstract};
use vulkano::device::Device;
use vulkano::framebuffer::{RenderPassAbstract, Subpass};
use vulkano::memory::pool::StdMemoryPool;
use vulkano::memory::DeviceMemoryAllocError;
use vulkano::pipeline::vertex::SingleBufferDefinition;
use vulkano::pipeline::{GraphicsPipeline, GraphicsPipelineBuilder};

mod shape_vert {
    vulkano_shaders::shader!(ty: "vertex", path: "src/shaders/shape.vert");
}

mod shape_frag {
    vulkano_shaders::shader!(ty: "fragment", path: "src/shaders/shape.frag");
}

#[repr(C)]
struct ShapeVertex {
    a_position: [f32; 2],
}

impl_vertex!(ShapeVertex, a_position);

const STROKE_ARC_THRESHOLD: f32 = f32::consts::PI / 6.;
const MITER_LIMIT: f32 = 10.;

fn nan_to_zero(i: f32) -> f32 {
    if i.is_finite() {
        i
    } else {
        0.
    }
}
fn nan_to_zero_vec2(v: Vector2<f32>) -> Vector2<f32> {
    (nan_to_zero(v.x), nan_to_zero(v.y)).into()
}
fn nan_to_zero_vec3(v: Vector3<f32>) -> Vector3<f32> {
    (nan_to_zero(v.x), nan_to_zero(v.y), nan_to_zero(v.z)).into()
}

// PathIterator for [cgmath::Vector2] items.
struct VertIterator<'a>(usize, &'a [Vector2<f32>], Option<Transform2D>, PathState);

impl<'a> VertIterator<'a> {
    fn new(verts: &'a [Vector2<f32>]) -> VertIterator {
        let mut state = PathState::new();
        if let Some(vert) = verts.first() {
            state.first = Point::new(vert.x, vert.y);
        }
        VertIterator(0, verts, None, state)
    }
}

impl<'a> Iterator for VertIterator<'a> {
    type Item = PathEvent;

    fn next(&mut self) -> Option<PathEvent> {
        if let Some(item) = self.1.get(self.0) {
            self.0 += 1;
            let mut vert = Point::new(item.x, item.y);
            self.3.current = vert;
            self.3.last_ctrl = vert;
            if let Some(transform) = self.2 {
                vert = transform.transform_point(&vert);
            }
            Some(if self.0 == 1 {
                PathEvent::MoveTo(vert)
            } else {
                PathEvent::LineTo(vert)
            })
        } else {
            None
        }
    }
}

impl<'a> PathIterator for VertIterator<'a> {
    fn get_state(&self) -> &PathState {
        &self.3
    }
}

impl Shape {
    fn create_or_update_buffers(
        dev: &Arc<Device>,
        ibuf: Option<Arc<CpuAccessibleBuffer<[u16]>>>,
        vbuf: Option<Arc<CpuAccessibleBuffer<[ShapeVertex]>>>,
        indices: &[u16],
        verts: &[Vector2<f32>],
    ) -> Result<
        (
            Arc<CpuAccessibleBuffer<[u16]>>,
            Arc<CpuAccessibleBuffer<[ShapeVertex]>>,
        ),
        Error,
    > {
        let ibuf = if ibuf
            .as_ref()
            .map_or(false, |ibuf| ibuf.len() == indices.len())
        {
            let ibuf = ibuf.unwrap();
            {
                let mut buf = ibuf.write()?;
                for i in 0..indices.len() {
                    buf[i] = indices[i];
                }
            }
            ibuf
        } else {
            CpuAccessibleBuffer::from_iter(
                Arc::clone(&dev),
                BufferUsage::index_buffer(),
                indices.into_iter().map(|x| *x),
            )?
        };

        let vbuf = if vbuf
            .as_ref()
            .map_or(false, |vbuf| vbuf.len() == verts.len())
        {
            let vbuf = vbuf.unwrap();
            {
                let mut buf = vbuf.write()?;
                for i in 0..verts.len() {
                    buf[i] = ShapeVertex {
                        a_position: verts[i].into(),
                    };
                }
            }
            vbuf
        } else {
            CpuAccessibleBuffer::from_iter(
                Arc::clone(&dev),
                BufferUsage::vertex_buffer(),
                verts.into_iter().map(|vert| ShapeVertex {
                    a_position: (*vert).into(),
                }),
            )?
        };

        Ok((ibuf, vbuf))
    }

    fn tess_stroke(
        &self,
        dev: &Arc<Device>,
        ibuf: Option<Arc<CpuAccessibleBuffer<[u16]>>>,
        vbuf: Option<Arc<CpuAccessibleBuffer<[ShapeVertex]>>>,
    ) -> Result<
        Option<(
            Arc<CpuAccessibleBuffer<[u16]>>,
            Arc<CpuAccessibleBuffer<[ShapeVertex]>>,
        )>,
        Error,
    > {
        if let Some((weight, width, _)) = &self.stroke {
            let shape_verts = self.path.flatten_to_verts();

            let mut verts = Vec::new();
            let mut indices = Vec::new();
            for contiguous_shape in shape_verts {
                let (mut v, i) = stroke_tess::tessellate(
                    &Self::stroke_points(&weight, *width, &contiguous_shape),
                    STROKE_ARC_THRESHOLD,
                );
                verts.append(&mut v);
                let offset = indices.len() as u16;
                indices.reserve(i.len());
                i.into_iter()
                    .map(|i| i + offset)
                    .for_each(|i| indices.push(i));
            }
            Ok(Some(Self::create_or_update_buffers(
                dev, ibuf, vbuf, &indices, &verts,
            )?))
        } else {
            Ok(None)
        }
    }

    fn tess_fill(
        &self,
        dev: &Arc<Device>,
        ibuf: Option<Arc<CpuAccessibleBuffer<[u16]>>>,
        vbuf: Option<Arc<CpuAccessibleBuffer<[ShapeVertex]>>>,
    ) -> Result<
        Option<(
            Arc<CpuAccessibleBuffer<[u16]>>,
            Arc<CpuAccessibleBuffer<[ShapeVertex]>>,
        )>,
        Error,
    > {
        if let Some(_) = self.fill {
            let shape_verts: Vec<_> = self
                .path
                .flatten_to_verts()
                .into_iter()
                .flat_map(|s| s)
                .collect();

            let mut buffers = VertexBuffers::new();

            {
                let mut vertex_builder = geometry_builder::simple_builder(&mut buffers);
                let mut tessellator = FillTessellator::new();
                let opts = FillOptions::DEFAULT
                    .on_error(OnError::Recover)
                    .with_normals(false);

                #[derive(Debug, Fail)]
                #[fail(display = "tessellation error: {:?}", _0)]
                struct FillErr(FillError);

                tessellator
                    .tessellate_path(VertIterator::new(&shape_verts), &opts, &mut vertex_builder)
                    .map_err(FillErr)?;
            }

            let verts: Vec<_> = buffers
                .vertices
                .into_iter()
                .map(|v| Vector2::new(v.position.x, v.position.y))
                .collect();
            Ok(Some(Self::create_or_update_buffers(
                dev,
                ibuf,
                vbuf,
                &buffers.indices,
                &verts,
            )?))
        } else {
            Ok(None)
        }
    }

    fn stroke_points(
        stroke_weight: &StrokeWeight,
        stroke_width: f32,
        shape_verts: &[Vector2<f32>],
    ) -> Vec<TessPoint> {
        let mut total_length = 0.;
        let mut last: Option<Vector2<f32>> = None;
        let mut line_verts = Vec::new();
        for (vertex, i) in shape_verts.iter().zip(0..) {
            // calculate normals
            let last_dir = if let Some(last) = last {
                let diff = vertex - last;
                // also add to total length
                total_length += diff.magnitude();
                Some(nan_to_zero_vec2(diff.normalize()))
            } else {
                None
            };
            let next_dir = shape_verts
                .get(i + 1)
                .map(|next| nan_to_zero_vec2((next - vertex).normalize()));

            let normal = match (last_dir, next_dir) {
                (Some(last), Some(next)) => {
                    let tangent = nan_to_zero_vec2((last + next).normalize());
                    let normal = Vector2::new(-tangent.y, tangent.x);
                    let miter_len = 1. / normal.perp_dot(-nan_to_zero_vec2(last.normalize()));
                    // TODO: proper handling
                    normal * miter_len.min(MITER_LIMIT)
                }
                (Some(last), None) => Vector2::new(-last.y, last.x),
                (None, Some(next)) => Vector2::new(-next.y, next.x),
                (None, None) => Vector2::zero(),
            };

            last = Some(*vertex);
            line_verts.push((total_length, *vertex, normal));
        }

        if line_verts.is_empty() {
            return Vec::new();
        }

        let weight_verts = stroke_weight.flatten_to_verts();

        if weight_verts.is_empty() {
            return Vec::new();
        }

        let mut points = Vec::new();

        let mut prev_vertex = None;

        let mut last_a = None;
        let mut last_b = None;
        Interleaved::new(
            line_verts.iter(),
            weight_verts.iter(),
            |v| v.0,
            |v| v.x * total_length,
        )
        .map(|item| match item {
            InterleavedItem::A((len, vert, normal), index) => {
                let i = last_b.unwrap_or(0);
                let j = if let Some(last_b) = last_b {
                    (last_b + 1).min(weight_verts.len() - 1)
                } else {
                    0
                };
                let weight_i = weight_verts[i];
                let weight_j = weight_verts[j];
                let p = nan_to_zero((len / total_length - weight_i.x) / (weight_j.x - weight_i.x));
                let weight = nan_to_zero_vec3(weight_i.lerp(weight_j, p));
                last_a = Some(index);
                (*vert, *normal, weight)
            }
            InterleavedItem::B(weight, index) => {
                let i = last_a.unwrap_or(0);
                let j = if let Some(last_a) = last_a {
                    (last_a + 1).min(line_verts.len() - 1)
                } else {
                    0
                };
                let line_i = line_verts[i];
                let line_j = line_verts[j];
                let p = nan_to_zero((weight.x * total_length - line_i.0) / (line_j.0 - line_i.0));
                let vert = nan_to_zero_vec2(line_i.1.lerp(line_j.1, p));
                let normal = nan_to_zero_vec2(line_i.2.lerp(line_j.2, p));
                last_b = Some(index);
                (vert, normal, *weight)
            }
        })
        .filter(|(vertex, _, _)| {
            // remove duplicates
            // FIXME: this is probably a symptom of a different issue
            // (see also in StrokeWeight::flatten_to_verts)
            if Some(*vertex) == prev_vertex {
                return false;
            }
            prev_vertex = Some(*vertex);
            true
        })
        .for_each(|(vertex, normal, weight)| {
            points.push(TessPoint {
                pos: vertex + normal * weight.z * stroke_width,
                radius: weight.y * stroke_width / 2.,
            })
        });

        points
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct MatrixCacheKey([[i64; 4]; 4]);
impl MatrixCacheKey {
    fn float_to_fixed(f: f32) -> i64 {
        (f as i64) << 32 | ((f.fract() * 4_294_967_296.) as i64)
    }
    fn vector_to_fixed(v: [f32; 4]) -> [i64; 4] {
        [
            Self::float_to_fixed(v[0]),
            Self::float_to_fixed(v[1]),
            Self::float_to_fixed(v[2]),
            Self::float_to_fixed(v[3]),
        ]
    }
    fn matrix_to_fixed(m: [[f32; 4]; 4]) -> [[i64; 4]; 4] {
        [
            Self::vector_to_fixed(m[0]),
            Self::vector_to_fixed(m[1]),
            Self::vector_to_fixed(m[2]),
            Self::vector_to_fixed(m[3]),
        ]
    }
}

impl From<ShapeUniforms> for MatrixCacheKey {
    fn from(s: ShapeUniforms) -> MatrixCacheKey {
        MatrixCacheKey(Self::matrix_to_fixed(s.model))
    }
}

impl From<Globals> for MatrixCacheKey {
    fn from(s: Globals) -> MatrixCacheKey {
        MatrixCacheKey(Self::matrix_to_fixed(s.camera.into()))
    }
}

type ShapePipeline = Arc<
    GraphicsPipeline<
        SingleBufferDefinition<ShapeVertex>,
        Box<dyn PipelineLayoutAbstract + Send + Sync>,
        Arc<dyn RenderPassAbstract + Send + Sync>,
    >,
>;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
struct Globals {
    camera: Matrix4<f32>,
}

struct Cached {
    cached: Shape,
    stroke: Option<(
        Arc<CpuAccessibleBuffer<[u16]>>,
        Arc<CpuAccessibleBuffer<[ShapeVertex]>>,
    )>,
    fill: Option<(
        Arc<CpuAccessibleBuffer<[u16]>>,
        Arc<CpuAccessibleBuffer<[ShapeVertex]>>,
    )>,
    desc_set: Arc<dyn DescriptorSet + Send + Sync>,
    camera: Matrix4<f32>,
}

pub trait GraphicsPipelineConfig {
    fn config<A, B, C, D, E, F, G, H, I, J, K, L>(
        builder: GraphicsPipelineBuilder<A, B, C, D, E, F, G, H, I, J, K, L>,
    ) -> GraphicsPipelineBuilder<A, B, C, D, E, F, G, H, I, J, K, L>;
}

impl GraphicsPipelineConfig for () {
    fn config<A, B, C, D, E, F, G, H, I, J, K, L>(
        builder: GraphicsPipelineBuilder<A, B, C, D, E, F, G, H, I, J, K, L>,
    ) -> GraphicsPipelineBuilder<A, B, C, D, E, F, G, H, I, J, K, L> {
        builder
    }
}

/// Cached tessellator and renderer of [`Shape`]s.
pub struct ShapeRasterizer<ID: Copy + Hash + Eq> {
    device: Arc<Device>,
    cache: FnvHashMap<ID, Cached>,
    global_cache:
        HashMap<MatrixCacheKey, Weak<CpuBufferPoolSubbuffer<Globals, Arc<StdMemoryPool>>>>,
    global_pool: CpuBufferPool<Globals>,
    shape_uniform_pool: CpuBufferPool<ShapeUniforms>,
    shape_uniform_cache:
        HashMap<MatrixCacheKey, Weak<CpuBufferPoolSubbuffer<ShapeUniforms, Arc<StdMemoryPool>>>>,
    shape_pipeline: ShapePipeline,
    shape_ds_pool: FixedSizeDescriptorSetsPool<ShapePipeline>,
    shape_ds_cache:
        HashMap<(MatrixCacheKey, MatrixCacheKey), Weak<dyn DescriptorSet + Send + Sync>>,
    used_ids: FnvHashSet<ID>,
}

impl<ID: Copy + Hash + Eq> ShapeRasterizer<ID> {
    /// Creates a new shape rasterizer that will be bound to the given subpass in the given render
    /// pass.
    pub fn new(
        device: Arc<Device>,
        render_pass: &Arc<RenderPassAbstract + Send + Sync>,
        subpass: u32,
    ) -> Result<ShapeRasterizer<ID>, Error> {
        Self::new_with_pipeline_config::<()>(device, render_pass, subpass)
    }

    pub fn new_with_pipeline_config<F: GraphicsPipelineConfig>(
        device: Arc<Device>,
        render_pass: &Arc<RenderPassAbstract + Send + Sync>,
        subpass: u32,
    ) -> Result<ShapeRasterizer<ID>, Error> {
        let shape_vs = shape_vert::Shader::load(Arc::clone(&device))?;
        let shape_fs = shape_frag::Shader::load(Arc::clone(&device))?;

        let shape_pipeline = Arc::new(
            F::config(
                GraphicsPipeline::start()
                    .vertex_input_single_buffer::<ShapeVertex>()
                    .vertex_shader(shape_vs.main_entry_point(), ())
                    .viewports_scissors_dynamic(1)
                    .fragment_shader(shape_fs.main_entry_point(), ())
                    .blend_alpha_blending()
                    .depth_write(true)
                    .render_pass(
                        Subpass::from(Arc::clone(render_pass), subpass)
                            .expect("Subpass given to Rasterizer does not exist"),
                    ),
            )
            .build(Arc::clone(&device))?,
        );

        Ok(ShapeRasterizer {
            cache: FnvHashMap::default(),
            global_cache: HashMap::new(),
            global_pool: CpuBufferPool::uniform_buffer(Arc::clone(&device)),
            shape_uniform_pool: CpuBufferPool::uniform_buffer(Arc::clone(&device)),
            shape_uniform_cache: HashMap::new(),
            shape_ds_pool: FixedSizeDescriptorSetsPool::new(Arc::clone(&shape_pipeline), 0),
            shape_ds_cache: HashMap::new(),
            shape_pipeline,
            device,
            used_ids: FnvHashSet::default(),
        })
    }

    // TODO: deduplicate code
    fn global_buffer(
        &mut self,
        globals: Globals,
    ) -> Result<Arc<CpuBufferPoolSubbuffer<Globals, Arc<StdMemoryPool>>>, DeviceMemoryAllocError>
    {
        let cache_key: MatrixCacheKey = globals.into();
        if let Some(buf) = self
            .global_cache
            .get(&cache_key)
            .map_or(None, |weak| Weak::upgrade(&weak))
        {
            return Ok(Arc::clone(&buf));
        }
        let buf = Arc::new(self.global_pool.next(globals)?);
        self.global_cache.insert(cache_key, Arc::downgrade(&buf));
        Ok(buf)
    }

    fn uniform_buffer(
        &mut self,
        uniforms: ShapeUniforms,
    ) -> Result<
        Arc<CpuBufferPoolSubbuffer<ShapeUniforms, Arc<StdMemoryPool>>>,
        DeviceMemoryAllocError,
    > {
        let cache_key: MatrixCacheKey = uniforms.into();
        if let Some(buf) = self
            .shape_uniform_cache
            .get(&cache_key)
            .map_or(None, |weak| Weak::upgrade(&weak))
        {
            return Ok(Arc::clone(&buf));
        }
        let buf = Arc::new(self.shape_uniform_pool.next(uniforms)?);
        self.shape_uniform_cache
            .insert(cache_key, Arc::downgrade(&buf));
        Ok(buf)
    }

    fn desc_set(
        &mut self,
        globals: Globals,
        uniforms: ShapeUniforms,
    ) -> Result<Arc<dyn DescriptorSet + Send + Sync>, Error> {
        let cache_key: (MatrixCacheKey, MatrixCacheKey) = (globals.into(), uniforms.into());
        if let Some(desc_set) = self
            .shape_ds_cache
            .get(&cache_key)
            .map_or(None, |weak| Weak::upgrade(&weak))
        {
            return Ok(Arc::clone(&desc_set));
        }

        let globals = self.global_buffer(globals)?;
        let uniforms = self.uniform_buffer(uniforms)?;
        let desc_set: Arc<dyn DescriptorSet + Send + Sync> = Arc::new(
            self.shape_ds_pool
                .next()
                .add_buffer(globals)?
                .add_buffer(uniforms)?
                .build()?,
        );

        self.shape_ds_cache
            .insert(cache_key, Arc::downgrade(&desc_set));
        Ok(desc_set)
    }

    fn update(&mut self, id: ID, shape: &Shape, camera: Matrix4<f32>) -> Result<(), Error> {
        if !self.cache.contains_key(&id) {
            let desc_set = self.desc_set(
                Globals { camera },
                ShapeUniforms {
                    model: shape.transform.unwrap_or(Matrix4::identity()).into(),
                },
            )?;

            let stroke = shape.tess_stroke(&self.device, None, None)?;
            let fill = shape.tess_fill(&self.device, None, None)?;

            self.cache.insert(
                id,
                Cached {
                    cached: shape.clone(), // TODO: decide if this is a good idea
                    fill,
                    stroke,
                    desc_set,
                    camera,
                },
            );
        } else {
            // temporarily move out
            let mut cached = {
                let cached_ref = self.cache.get_mut(&id).unwrap();
                mem::replace(cached_ref, unsafe { mem::uninitialized() })
            };

            {
                let Cached {
                    cached,
                    stroke,
                    fill,
                    desc_set,
                    camera: cached_camera,
                } = &mut cached;

                let mut fill_tess = false;
                let mut stroke_tess = false;

                if shape.path != cached.path {
                    fill_tess = true;
                    stroke_tess = true;
                    cached.path = shape.path.clone();
                }

                if shape.fill.is_some() != cached.fill.is_some() {
                    fill_tess = true;
                }

                if shape.fill != cached.fill {
                    cached.fill = shape.fill.clone();
                }

                if shape.stroke.is_some() != cached.stroke.is_some()
                    || shape.stroke.as_ref().map(|(w, d, _)| (w, d))
                        != cached.stroke.as_ref().map(|(w, d, _)| (w, d))
                {
                    stroke_tess = true;
                }

                if shape.stroke != cached.stroke {
                    cached.stroke = shape.stroke.clone();
                }

                if fill_tess {
                    // temporarily move out
                    let mut ifill = mem::replace(fill, unsafe { mem::uninitialized() });
                    let (ibuf, vbuf) = ifill.map_or((None, None), |(x, y)| (Some(x), Some(y)));
                    ifill = shape.tess_fill(&self.device, ibuf, vbuf)?;
                    mem::forget(mem::replace(fill, ifill));
                }

                if stroke_tess {
                    // temporarily move out
                    let mut istroke = mem::replace(stroke, unsafe { mem::uninitialized() });
                    let (ibuf, vbuf) = istroke.map_or((None, None), |(x, y)| (Some(x), Some(y)));
                    istroke = shape.tess_stroke(&self.device, ibuf, vbuf)?;
                    mem::forget(mem::replace(stroke, istroke));
                }

                if shape.transform != cached.transform || camera != *cached_camera {
                    *desc_set = self.desc_set(
                        Globals { camera },
                        ShapeUniforms {
                            model: shape.transform.unwrap_or(Matrix4::identity()).into(),
                        },
                    )?;
                    cached.transform = shape.transform.clone();
                    *cached_camera = camera;
                }
            }
            let cached_ref = self.cache.get_mut(&id).unwrap();
            mem::forget(mem::replace(cached_ref, cached));
        }

        Ok(())
    }

    fn draw_shape(
        &self,
        id: ID,
        mut cmd_buffer: AutoCommandBufferBuilder,
        dyn_state: &DynamicState,
    ) -> Result<AutoCommandBufferBuilder, Error> {
        if let Some(cached) = self.cache.get(&id) {
            if let Some((indices, verts)) = &cached.fill {
                cmd_buffer = cmd_buffer.draw_indexed(
                    Arc::clone(&self.shape_pipeline),
                    dyn_state,
                    Arc::clone(verts),
                    Arc::clone(indices),
                    Arc::clone(&cached.desc_set),
                    ShapePushConstants {
                        color: cached.cached.fill.unwrap().into(),
                    },
                )?;
            }

            if let Some((indices, verts)) = &cached.stroke {
                cmd_buffer = cmd_buffer.draw_indexed(
                    Arc::clone(&self.shape_pipeline),
                    dyn_state,
                    Arc::clone(verts),
                    Arc::clone(indices),
                    Arc::clone(&cached.desc_set),
                    ShapePushConstants {
                        color: cached.cached.stroke.as_ref().unwrap().2.into(),
                    },
                )?;
            }

            Ok(cmd_buffer)
        } else {
            #[derive(Debug, Fail)]
            #[fail(display = "shape does not exist in cache")]
            struct NoShapeError;
            Err(NoShapeError.into())
        }
    }

    /// Draws a shape using the given command buffer.
    ///
    /// Note that this will add the shape to the cache (with the given ID).
    ///
    /// Also note that the current render pass must be the one this shape rasterizer was
    /// constructed with.
    pub fn draw(
        &mut self,
        cmd_buffer: AutoCommandBufferBuilder,
        id: ID,
        shape: &Shape,
        dyn_state: &DynamicState,
        camera: Matrix4<f32>,
    ) -> Result<AutoCommandBufferBuilder, Error> {
        self.used_ids.insert(id);
        self.update(id, shape, camera)?;
        self.draw_shape(id, cmd_buffer, dyn_state)
    }

    /// Frees all resources that weren’t used since the last call to `drop_unused`.
    pub fn drop_unused(&mut self) {
        for id in self
            .cache
            .keys()
            .filter(|id| !self.used_ids.contains(id))
            .map(|id| *id)
            .collect::<Vec<_>>()
        {
            self.cache.remove(&id);
        }

        self.used_ids.clear();

        for key in self
            .shape_uniform_cache
            .iter()
            .filter(|(_, v)| Weak::upgrade(&v).is_some())
            .map(|(k, _)| *k)
            .collect::<Vec<_>>()
        {
            self.shape_uniform_cache.remove(&key);
        }

        for key in self
            .shape_ds_cache
            .iter()
            .filter(|(_, v)| Weak::upgrade(&v).is_some())
            .map(|(k, _)| *k)
            .collect::<Vec<_>>()
        {
            self.shape_ds_cache.remove(&key);
        }
    }
}
