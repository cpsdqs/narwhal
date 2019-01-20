//! Node type definitions.

use crate::data::{Camera, TryFromValue, Value};
use crate::node::NodeRef;
use crate::render::{Context, TexturePool, TextureRef};
use failure::Error;
use fnv::FnvHashMap;
use std::any::Any;
use std::sync::Arc;
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::device::{Device, Queue};

/// An evaluation error.
#[derive(Fail, Debug, Clone)]
pub enum EvalError {
    /// Internal node error.
    #[fail(display = "internal node error: {}", _0)]
    Internal(Arc<Error>),

    /// Some node input is invalid.
    #[fail(display = "invalid input value: {}", _0)]
    Input(String),

    /// Input for a node property is missing.
    #[fail(display = "missing input for property #{}", _0)]
    MissingInput(usize),

    /// Input type mismatch, with an expected type.
    #[fail(display = "input #{} is of an incompatible type", _0)]
    InputType(usize),
}

impl From<Error> for EvalError {
    fn from(err: Error) -> EvalError {
        EvalError::Internal(Arc::new(err))
    }
}

macro_rules! conflate_errors_into_eval_error {
    ($($t:ty),+,) => {
        $(
            impl From<$t> for EvalError {
                fn from(err: $t) -> EvalError {
                    Error::from(err).into()
                }
            }
        )+
    }
}

conflate_errors_into_eval_error! {
    TexAllocError,
    vulkano::OomError,
    vulkano::framebuffer::FramebufferCreationError,
    vulkano::command_buffer::BeginRenderPassError,
    vulkano::command_buffer::AutoCommandBufferBuilderContextError,
}

pub type EvalResult<T> = Result<T, EvalError>;

/// Node input values.
pub struct Input {
    pub(crate) values: FnvHashMap<usize, Vec<Arc<Value>>>,
    pub(crate) node: NodeRef,
}

impl Input {
    /// Returns all values for the given key.
    pub fn get<K: Into<usize>>(&self, key: K) -> EvalResult<&[Arc<Value>]> {
        let key = key.into();
        match self.values.get(&key) {
            Some(values) => Ok(values),
            None => Err(EvalError::MissingInput(key)),
        }
    }

    /// Returns the first value for the key and attempts to downcast it.
    pub fn one<K: Into<usize>, V: TryFromValue>(&self, key: K) -> EvalResult<&V> {
        let key = key.into();
        match self.values.get(&key).map_or(None, |values| values.get(0)) {
            Some(value) => match V::try_from_ref(value) {
                Some(value) => Ok(value),
                None => Err(EvalError::InputType(key)),
            },
            None => Err(EvalError::MissingInput(key)),
        }
    }

    /// Returns the first value for the key and attempts to downcast it.
    pub fn one_any<K: Into<usize>, V: Any + Send + Sync>(&self, key: K) -> EvalResult<&V> {
        let key = key.into();
        match self.values.get(&key).map_or(None, |values| values.get(0)) {
            Some(value) => match &**value {
                Value::Any(value) => match value.downcast_ref::<V>() {
                    Some(value) => Ok(value),
                    None => Err(EvalError::InputType(key)),
                },
                _ => Err(EvalError::InputType(key)),
            },
            None => Err(EvalError::MissingInput(key)),
        }
    }

    /// Returns a reference to the current node.
    pub fn node(&self) -> NodeRef {
        self.node
    }
}

/// Texture allocation errors.
#[derive(Debug, Fail)]
pub enum TexAllocError {
    /// Failed to allocate the texture due to an internal error.
    #[fail(display = "internal error: {}", _0)]
    Internal(Error),
}

/// The node evaluation context.
pub struct NodeContext<'a> {
    pub(crate) context: Context,
    pub(crate) tex_pool: &'a mut TexturePool,
}

impl<'a> NodeContext<'a> {
    pub fn camera(&self) -> Camera {
        self.context.camera
    }

    pub fn resolution(&self) -> f32 {
        self.context.resolution
    }

    /// Allocates a storage texture from the texture pool.
    pub fn new_storage_texture(
        &mut self,
        width: f32,
        height: f32,
        resolution: f32,
    ) -> Result<TextureRef, TexAllocError> {
        self.tex_pool
            .storage(width, height, resolution)
            .map_err(|e| TexAllocError::Internal(e))
    }

    /// Allocates an attachment texture from the texture pool.
    pub fn new_attachment(
        &mut self,
        width: f32,
        height: f32,
        resolution: f32,
    ) -> Result<TextureRef, TexAllocError> {
        self.tex_pool
            .attachment(width, height, resolution)
            .map_err(|e| TexAllocError::Internal(e))
    }
}

/// Node outputs.
pub struct Output {
    pub(crate) values: FnvHashMap<usize, Arc<Value>>,
}

impl Output {
    /// Sets an output value.
    pub fn set<K: Into<usize>, V: Into<Value>>(&mut self, key: K, value: V) {
        self.values.insert(key.into(), Arc::new(value.into()));
    }
}

/// A data node that may only handle values directly and without context.
///
/// This is the only kind of node that may be used to compute the camera transform.
pub trait DataNode: Send + Sync {
    /// Evaluates the outputs from the given inputs.
    fn eval(&mut self, input: Input, output: &mut Output) -> EvalResult<()>;

    /// If true, this node will only be considered dirty if its inputs change.
    ///
    /// True by default.
    fn is_pure(&self) -> bool {
        true
    }
}

/// A graphics node with access to the command buffer.
pub trait GraphicsNode: Send + Sync {
    /// Evaluates the outputs from the given inputs and the context.
    fn eval(
        &mut self,
        input: Input,
        context: NodeContext,
        output: &mut Output,
        cmd_buffer: AutoCommandBufferBuilder,
    ) -> EvalResult<AutoCommandBufferBuilder>;

    /// Optionally modifies the given context for the input nodes, if, for example, only a small
    /// region of an input texture is required.
    fn map_context(&self, _context: &mut Context) {}
}

/// A shared data node type that may hold shared data and can create data nodes.
pub trait SharedDataType: Send + Sync {
    fn name(&self) -> String;
    fn create(&mut self) -> Box<dyn DataNode>;
}

/// A shared data node type that may hold shared data (such as shaders) and can create data nodes.
pub trait SharedGraphicsType: Send + Sync {
    fn name(&self) -> String;
    fn create(&mut self) -> Box<dyn GraphicsNode>;
}

/// A node type definition, holding functions that create a shared node type.
#[derive(Clone, Copy)]
pub enum NodeTypeDef {
    Data(fn() -> Box<dyn SharedDataType>),
    Graphics(fn(&Arc<Device>, &Arc<Queue>) -> Result<Box<dyn SharedGraphicsType>, Error>),
}
