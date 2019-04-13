use crate::data::{Camera, Drawable, Value};
use crate::eval::*;
use crate::node::{Graph, NodeRef, OrderError};
use crate::render::{
    Context, ShapeRasterizer, TexturePool, TextureRef, COLOR_FORMAT, DEPTH_FORMAT,
};
use failure::Error;
use fnv::{FnvHashMap, FnvHashSet};
use std::collections::HashMap;
use std::sync::Arc;
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::device::{Device, Queue};
use vulkano::framebuffer::{Framebuffer, RenderPassAbstract};
use vulkano::pipeline::viewport::{Scissor, Viewport};
use vulkano::OomError;

const CAMERA_SCENE_INPUT_PROP: usize = 0;
const CAMERA_DATA_OUTPUT_PROP: usize = 0;

/// A node type instance.
pub enum NodeType {
    Data(Box<dyn SharedDataType>),
    Graphics(Box<dyn SharedGraphicsType>),
}

impl NodeType {
    fn name(&self) -> String {
        match self {
            NodeType::Data(data) => data.name(),
            NodeType::Graphics(graphics) => graphics.name(),
        }
    }

    fn clear_caches(&mut self) {
        match self {
            NodeType::Data(data) => data.clear_caches(),
            NodeType::Graphics(graphics) => graphics.clear_caches(),
        }
    }
}

enum NodeInstance {
    Data(Box<dyn DataNode>),
    Graphics(Box<dyn GraphicsNode>),
}

impl NodeInstance {
    fn wants_rasterized(&self) -> bool {
        match self {
            NodeInstance::Graphics(_) => true,
            NodeInstance::Data(_) => false,
        }
    }

    fn is_contextful(&self) -> bool {
        match self {
            NodeInstance::Graphics(_) => true,
            NodeInstance::Data(_) => false,
        }
    }

    fn is_impure(&self) -> bool {
        match self {
            NodeInstance::Graphics(_) => false,
            NodeInstance::Data(data) => !data.is_pure(),
        }
    }

    fn clear_caches(&mut self) {
        match self {
            NodeInstance::Graphics(graphics) => graphics.clear_caches(),
            NodeInstance::Data(data) => data.clear_caches(),
        }
    }
}

fn value_is_rasterizable(value: &Value) -> bool {
    match value {
        Value::Drawables(_) => true,
        _ => false,
    }
}

/// Internal renderer errors that occur when something is very wrong.
#[derive(Debug, Fail)]
pub enum InternalRendererError {
    /// A node is supposed to exist but doesn’t.
    #[fail(display = "missing node: {:?}", _0)]
    MissingNode(NodeRef),

    /// A node wasn’t assigned any context data even though it should’ve been.
    #[fail(display = "missing context for node {:?}", _0)]
    NoContext(NodeRef),

    /// Some other internal error.
    #[fail(display = "{}", _0)]
    Other(Arc<Error>),
}

/// Rendering errors.
#[derive(Debug, Fail)]
pub enum RenderError {
    /// A node failed to evaluate its outputs.
    #[fail(display = "eval error on node {:?}: {}", _0, _1)]
    Eval(NodeRef, EvalError),

    /// A node type is missing.
    #[fail(display = "missing node type: {}", _0)]
    MissingNodeType(String),

    /// An internal error occured.
    #[fail(display = "internal renderer error: {}", _0)]
    Internal(InternalRendererError),

    /// A node that is not of the Data type is connected to the camera node’s transform inputs, but
    /// non-data nodes can’t be used before the camera transform is determined.
    #[fail(display = "camera input {:?} is not a data node", _0)]
    NonDataCameraInput(NodeRef),

    /// The graph isn’t acyclic.
    #[fail(display = "failed to topologically sort the graph: {}", _0)]
    OrderError(OrderError),

    /// The camera node did not output a [`Camera`] struct at output port 0.
    #[fail(display = "no camera data")]
    NoCameraData,

    /// The camera node did not receive a [`TextureRef`] value at input port 0.
    #[fail(display = "no scene")]
    NoScene,
}

impl From<OrderError> for RenderError {
    fn from(err: OrderError) -> RenderError {
        RenderError::OrderError(err)
    }
}

impl From<InternalRendererError> for RenderError {
    fn from(err: InternalRendererError) -> RenderError {
        RenderError::Internal(err)
    }
}

impl From<Error> for RenderError {
    fn from(err: Error) -> RenderError {
        RenderError::Internal(InternalRendererError::Other(Arc::new(err)))
    }
}

/// Number of cycles until garbage is collected
const CYCLES_UNTIL_GC: u8 = 128;

/// Graph renderer.
pub struct Renderer {
    graph: Graph,
    shape_rasterizer: ShapeRasterizer<(NodeRef, u64)>,
    shape_render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
    texture_pool: TexturePool,
    ctx_cache: FnvHashMap<NodeRef, Context>,
    cache: FnvHashMap<NodeRef, FnvHashMap<usize, Arc<Value>>>,
    node_types: HashMap<String, NodeType>,
    nodes: FnvHashMap<NodeRef, NodeInstance>,
    resolution: f32,
    cycle: u8,
    device: Arc<Device>,
    queue: Arc<Queue>,
}

impl Renderer {
    /// Creates a new renderer.
    pub fn new(graph: Graph, device: Arc<Device>, queue: Arc<Queue>) -> Result<Renderer, Error> {
        let shape_render_pass: Arc<dyn RenderPassAbstract + Send + Sync> =
            Arc::new(single_pass_renderpass! {
                Arc::clone(&device),
                attachments: {
                    color: {
                        load: Clear,
                        store: Store,
                        format: COLOR_FORMAT,
                        samples: 1,
                    },
                    depth: {
                        load: Clear,
                        store: Store,
                        format: DEPTH_FORMAT,
                        samples: 1,
                    }
                },
                pass: {
                    color: [color],
                    depth_stencil: {depth}
                }
            }?);

        Ok(Renderer {
            graph,
            shape_rasterizer: ShapeRasterizer::new(Arc::clone(&device), &shape_render_pass, 0)?,
            shape_render_pass,
            cache: FnvHashMap::default(),
            ctx_cache: FnvHashMap::default(),
            node_types: HashMap::new(),
            nodes: FnvHashMap::default(),
            texture_pool: TexturePool::new(Arc::clone(&device), Arc::clone(&queue)),
            resolution: 1.,
            cycle: 0,
            device,
            queue,
        })
    }

    /// Adds a node type.
    pub fn add_node_type(&mut self, type_def: NodeTypeDef) -> Result<(), Error> {
        let node_type = match type_def {
            NodeTypeDef::Data(new) => NodeType::Data(new()),
            NodeTypeDef::Graphics(new) => NodeType::Graphics(new(&self.device, &self.queue)?),
        };
        self.node_types.insert(node_type.name(), node_type);
        Ok(())
    }

    /// Adds a node type with a closure.
    pub fn add_node_type_with<F: FnOnce(&Device, &Queue) -> Result<NodeType, Error>>(
        &mut self,
        gen: F,
    ) -> Result<(), Error> {
        let node_type = gen(&self.device, &self.queue)?;
        self.node_types.insert(node_type.name(), node_type);
        Ok(())
    }

    /// Returns a mutable reference to the loaded node types.
    pub fn node_types_mut(&mut self) -> &mut HashMap<String, NodeType> {
        &mut self.node_types
    }

    /// Returns a reference to the graph.
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// Returns a mutable reference to the graph.
    pub fn graph_mut(&mut self) -> &mut Graph {
        &mut self.graph
    }

    /// Returns the rendering resolution.
    pub fn resolution(&self) -> f32 {
        self.resolution
    }

    /// Sets the rendering resolution.
    pub fn set_resolution(&mut self, value: f32) {
        self.resolution = value;
    }

    /// Propagates cache invalidation through the graph starting from the given node’s outputs.
    /// This should be called if a node’s outputs have changed and all subsequent nodes
    /// must thus be re-evaluated.
    fn propagate_cache_invalidation(&mut self, node: NodeRef) {
        for (node, ..) in self.graph.node_outputs(node).collect::<Vec<_>>() {
            self.cache.remove(&node);
            self.propagate_cache_invalidation(node);
        }
    }

    /// Updates the output cache for the given node. If the outputs are different from the previous
    /// values, all subsequent nodes’ caches will be invalidated.
    fn set_cache(&mut self, node: NodeRef, outputs: FnvHashMap<usize, Arc<Value>>) {
        if let Some(cached) = self.cache.get(&node) {
            // FIXME: textures may have been updated without their ID changing
            if cached != &outputs {
                return;
            }
        }

        self.cache.insert(node, outputs);
        self.propagate_cache_invalidation(node);
    }

    /// Returns all node inputs.
    fn node_inputs(
        &self,
        node: NodeRef,
        ignore_prop: Option<usize>,
    ) -> Result<FnvHashMap<usize, Vec<Arc<Value>>>, EvalError> {
        let mut inputs = FnvHashMap::default();

        // collect inputs from connected nodes
        for (input, out_prop, in_prop) in self.graph.node_inputs(node) {
            if Some(in_prop) == ignore_prop {
                continue;
            }

            if let Some(values) = self.cache.get(&input) {
                if let Some(value) = values.get(&out_prop) {
                    inputs
                        .entry(in_prop)
                        .or_insert_with(|| Vec::new())
                        .push(Arc::clone(&value));
                } else {
                    return Err(EvalError::MissingInput(in_prop));
                }
            } else {
                return Err(EvalError::MissingInput(in_prop));
            }
        }

        // collect fallback property values
        if let Some(node) = self.graph.node(&node) {
            for (k, v) in node.props.iter() {
                inputs
                    .entry(*k)
                    .or_insert_with(|| vec![Arc::new(v.clone())]);
            }
        }

        Ok(inputs)
    }

    /// Recursively propagates node contexts.
    fn propagate_contexts(
        &mut self,
        node_ref: NodeRef,
        mut context: Context,
    ) -> Result<(), RenderError> {
        self.ensure_node_instance(node_ref)?;

        match self.nodes.get(&node_ref).unwrap() {
            NodeInstance::Data(_) => (),
            NodeInstance::Graphics(node) => {
                self.ctx_cache
                    .entry(node_ref)
                    .and_modify(|context| context.merge(*context))
                    .or_insert(context);
                node.map_context(&mut context);
            }
        }

        let mut input_nodes = FnvHashSet::default();
        for (node, ..) in self.graph.node_inputs(node_ref) {
            input_nodes.insert(node);
        }
        for node in input_nodes {
            self.propagate_contexts(node, context)?;
        }

        Ok(())
    }

    /// Ensures the existence of a node instance for the given node.
    fn ensure_node_instance(&mut self, node_ref: NodeRef) -> Result<(), RenderError> {
        if self.nodes.contains_key(&node_ref) {
            return Ok(());
        }

        if let Some(node) = self.graph.node(&node_ref) {
            if let Some(node_type) = self.node_types.get_mut(&node.node_type) {
                let instance = match node_type {
                    NodeType::Data(node_type) => NodeInstance::Data(node_type.create()),
                    NodeType::Graphics(node_type) => NodeInstance::Graphics(node_type.create()),
                };
                self.nodes.insert(node_ref, instance);

                Ok(())
            } else {
                Err(RenderError::MissingNodeType(node.node_type.clone()))
            }
        } else {
            Err(InternalRendererError::MissingNode(node_ref).into())
        }
    }

    /// Evaluates a single data node for the camera inputs and caches its outputs.
    fn eval_one_camera(&mut self, node_ref: NodeRef, is_camera: bool) -> Result<(), RenderError> {
        let inputs = Input {
            values: self
                .node_inputs(
                    node_ref,
                    if is_camera {
                        Some(CAMERA_SCENE_INPUT_PROP)
                    } else {
                        None
                    },
                )
                .map_err(|e| RenderError::Eval(node_ref, e))?,
            node: node_ref,
        };

        let mut outputs = Output {
            values: FnvHashMap::default(),
        };

        self.ensure_node_instance(node_ref)?;

        match self.nodes.get_mut(&node_ref).unwrap() {
            NodeInstance::Data(node) => node
                .eval(inputs, &mut outputs)
                .map_err(|e| RenderError::Eval(node_ref, e))?,
            NodeInstance::Graphics(_) => {
                return Err(RenderError::NonDataCameraInput(node_ref));
            }
        }

        self.set_cache(node_ref, outputs.values);
        Ok(())
    }

    /// Evaluates the camera node and its data inputs, recursively.
    fn eval_camera(&mut self, node_ref: NodeRef, is_camera: bool) -> Result<(), RenderError> {
        let mut input_nodes = FnvHashSet::default();
        for (input, _, in_prop) in self.graph.node_inputs(node_ref) {
            if is_camera && in_prop == CAMERA_SCENE_INPUT_PROP {
                continue;
            }
            input_nodes.insert(input);
        }

        for node in input_nodes {
            self.eval_camera(node, false)?;
        }

        self.eval_one_camera(node_ref, is_camera)
    }

    /// Evaluates a single node and caches its outputs.
    fn eval_one(
        &mut self,
        node_ref: NodeRef,
        mut cmd_buffer: AutoCommandBufferBuilder,
    ) -> Result<AutoCommandBufferBuilder, RenderError> {
        let inputs = Input {
            values: self
                .node_inputs(node_ref, None)
                .map_err(|e| RenderError::Eval(node_ref, e))?,
            node: node_ref,
        };
        let mut outputs = Output {
            values: FnvHashMap::default(),
        };

        self.ensure_node_instance(node_ref)?;

        match self.nodes.get_mut(&node_ref).unwrap() {
            NodeInstance::Data(node) => node
                .eval(inputs, &mut outputs)
                .map_err(|e| RenderError::Eval(node_ref, e))?,
            NodeInstance::Graphics(node) => {
                let context = match self.ctx_cache.get(&node_ref) {
                    Some(context) => *context,
                    None => return Err(InternalRendererError::NoContext(node_ref).into()),
                };

                let node_context = NodeContext {
                    context,
                    tex_pool: &mut self.texture_pool,
                };

                cmd_buffer = node
                    .eval(inputs, node_context, &mut outputs, cmd_buffer)
                    .map_err(|e| RenderError::Eval(node_ref, e))?;
            }
        }

        let mut ports_to_rasterize = FnvHashMap::default();
        for (node, out_prop, _) in self.graph.node_outputs(node_ref) {
            if self
                .nodes
                .get(&node)
                .map_or(false, |node| node.wants_rasterized())
            {
                if outputs
                    .values
                    .get(&out_prop)
                    .map_or(false, |value| value_is_rasterizable(value))
                {
                    if let Some(context) = self.ctx_cache.get(&node_ref) {
                        ports_to_rasterize.insert(out_prop, *context);
                    }
                }
            }
        }

        for (port, context) in ports_to_rasterize {
            let value = outputs.values.get_mut(&port).unwrap();

            match &**value {
                Value::Drawables(drawables) => {
                    let (c, tex) = self.rasterize_drawables(drawables, context, cmd_buffer)?;
                    cmd_buffer = c;
                    *value = Arc::new(Value::Texture(tex));
                }
                v => panic!("don’t know how to rasterize {:?}", v.value_type()),
            }
        }

        self.set_cache(node_ref, outputs.values);
        Ok(cmd_buffer)
    }

    /// Renders the entire scene.
    pub fn render(
        &mut self,
        mut cmd_buffer: AutoCommandBufferBuilder,
    ) -> Result<(AutoCommandBufferBuilder, TextureRef), RenderError> {
        let camera_ref = self.graph.output();
        self.eval_camera(camera_ref, true)?;

        if !self.graph.has_order() {
            self.graph.update_order()?;
        }

        let camera = self
            .cache
            .get(&camera_ref)
            .map_or(None, |values| values.get(&CAMERA_DATA_OUTPUT_PROP))
            .map_or(None, |value| match &**value {
                Value::Any(any) => any.downcast_ref::<Camera>(),
                _ => None,
            })
            .map(|camera| *camera);

        let camera = match camera {
            Some(camera) => camera,
            None => return Err(RenderError::NoCameraData),
        };

        let context = Context {
            camera,
            resolution: self.resolution,
        };

        let order: Vec<_> = self
            .graph
            .order()
            .unwrap()
            .into_iter()
            .map(|x| *x)
            .collect(); // clone :/

        self.ctx_cache.clear();
        for i in 0..order.len() {
            let node_ref = order[order.len() - i - 1];
            self.propagate_contexts(node_ref, context)?;
        }

        let camera_is_dirty = self.graph.is_dirty(&camera_ref);
        self.graph.mark_clean(&camera_ref);

        for node_ref in &order {
            let is_dirty = self.graph.is_dirty(node_ref)
                || (camera_is_dirty
                    && self
                        .nodes
                        .get(node_ref)
                        .map_or(false, |node| node.is_contextful()))
                || self
                    .nodes
                    .get(node_ref)
                    .map_or(false, |node| node.is_impure());

            if is_dirty {
                self.graph.mark_clean(node_ref);
                self.cache.remove(node_ref);
                self.propagate_cache_invalidation(*node_ref);
            }
        }

        for node_ref in &order {
            if self.cache.contains_key(node_ref) {
                continue;
            }
            cmd_buffer = self.eval_one(*node_ref, cmd_buffer)?;
        }

        self.cycle += 1;
        if self.cycle >= CYCLES_UNTIL_GC {
            self.cycle = 0;
            self.shape_rasterizer.drop_unused();
            self.texture_pool.drop_unused();

            let mut unused_nodes: FnvHashSet<_> =
                self.cache.keys().chain(self.nodes.keys()).map(|k| *k).collect();

            for node in order {
                unused_nodes.remove(&node);
            }

            for node in unused_nodes {
                self.cache.remove(&node);
                self.nodes.remove(&node);
            }
        }

        let inputs = self
            .node_inputs(camera_ref, None)
            .map_err(|e| RenderError::Eval(camera_ref, e))?;
        match inputs
            .get(&CAMERA_SCENE_INPUT_PROP)
            .map_or(None, |values| values.get(0))
        {
            Some(value) => match &**value {
                Value::Texture(tex) => Ok((cmd_buffer, tex.clone())),
                _ => Err(RenderError::NoScene),
            },
            None => Err(RenderError::NoScene),
        }
    }

    /// Rasterizes the given drawables into a new texture.
    fn rasterize_drawables(
        &mut self,
        drawables: &[Drawable],
        context: Context,
        mut cmd_buffer: AutoCommandBufferBuilder,
    ) -> Result<(AutoCommandBufferBuilder, TextureRef), RenderError> {
        let width = context.camera.width.max(1.);
        let height = context.camera.height.max(1.);
        let resolution = context.resolution.min(4096. / width).min(4096. / height);

        let px_width = width * resolution;
        let px_height = height * resolution;

        let texture = self.texture_pool.attachment(width, height, resolution)?;

        if !drawables.is_empty() {
            let framebuffer = Arc::new(
                Framebuffer::start(self.shape_render_pass.clone())
                    .add(texture.color().clone())
                    .map_err(|e| Error::from(e))?
                    .add(
                        texture
                            .depth()
                            .expect("Texture has no depth attachment?")
                            .clone(),
                    )
                    .map_err(|e| Error::from(e))?
                    .build()
                    .map_err(|e| Error::from(e))?,
            );

            cmd_buffer = cmd_buffer
                .begin_render_pass(
                    framebuffer,
                    false,
                    vec![[0., 0., 0., 0.].into(), 0.0.into()],
                )
                .map_err(|e| Error::from(e))?;

            let camera = context.camera.matrix();

            let scissor = Scissor {
                origin: [0, 0],
                dimensions: [px_width as u32, px_height as u32],
            };
            let viewport = Viewport {
                origin: [0., 0.],
                dimensions: [px_width, px_height],
                depth_range: 0.0..1.0,
            };

            let dyn_state = DynamicState {
                line_width: None,
                scissors: Some(vec![scissor]),
                viewports: Some(vec![viewport]),
            };

            for drawable in drawables {
                cmd_buffer = self.shape_rasterizer.draw(
                    cmd_buffer,
                    drawable.id,
                    &drawable.shape,
                    &dyn_state,
                    camera,
                )?;
            }

            cmd_buffer = cmd_buffer.end_render_pass().map_err(|e| Error::from(e))?;
        }

        Ok((cmd_buffer, texture))
    }

    /// Creates a new command buffer using the current device
    pub fn new_cmd_buffer(&self) -> Result<AutoCommandBufferBuilder, OomError> {
        AutoCommandBufferBuilder::primary_one_time_submit(
            Arc::clone(&self.device),
            self.queue.family(),
        )
    }

    /// Drops all caches or other ‘inessential data’ such as buffers and textures.
    pub fn clear_caches(&mut self) {
        self.shape_rasterizer.clear_caches();
        self.texture_pool.clear_caches();
        self.ctx_cache.clear();
        self.cache.clear();

        for (_, node_type) in &mut self.node_types {
            node_type.clear_caches();
        }

        for (_, node_instance) in &mut self.nodes {
            node_instance.clear_caches();
        }
    }
}
