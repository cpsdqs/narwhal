use crate::data::{Color, Drawable, Path2D, StrokeWeight};
use crate::render::TextureRef;
use cgmath::{Matrix4, Vector2, Vector3, Vector4};
use std::any::Any;
use std::fmt;
use std::sync::Arc;

/// A value.
#[derive(Debug, Clone)]
pub enum Value {
    /// A 64-bit float number.
    Float(f64),

    /// An arbitrary string.
    String(String),

    /// Two 64-bit floats.
    Vec2(Vector2<f64>),

    /// Three 64-bit floats.
    Vec3(Vector3<f64>),

    /// Four 64-bit floats.
    Vec4(Vector4<f64>),

    /// A 3D transformation matrix.
    Mat4(Matrix4<f64>),

    /// A color.
    Color(Color),

    /// A path.
    Path2D(Path2D),

    /// A stroke weight profile.
    StrokeWeight(StrokeWeight),

    /// A list of drawables.
    Drawables(Vec<Drawable>),

    /// A 2D bitmap texture.
    Texture(TextureRef),

    /// Unknown or undefined type with raw bytes.
    Raw(Vec<u8>),

    /// Any intermediate value representation.
    Any(Arc<Any + Send + Sync>),
}

/// Value types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueType {
    Float,
    String,
    Vec2,
    Vec3,
    Vec4,
    Mat4,
    Color,
    Path2D,
    StrokeWeight,
    Drawables,
    Texture,
    Raw,
    Any,
}

impl fmt::Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Value {
    /// Returns the type name (i.e. the discriminant).
    pub fn value_type(&self) -> ValueType {
        match self {
            Value::Float(..) => ValueType::Float,
            Value::String(..) => ValueType::String,
            Value::Vec2(..) => ValueType::Vec2,
            Value::Vec3(..) => ValueType::Vec3,
            Value::Vec4(..) => ValueType::Vec4,
            Value::Mat4(..) => ValueType::Mat4,
            Value::Color(..) => ValueType::Color,
            Value::Path2D(..) => ValueType::Path2D,
            Value::StrokeWeight(..) => ValueType::StrokeWeight,
            Value::Drawables(..) => ValueType::Drawables,
            Value::Texture(..) => ValueType::Texture,
            Value::Raw(..) => ValueType::Raw,
            Value::Any(..) => ValueType::Any,
        }
    }
}

macro_rules! impl_value_from {
    ($ty:ty => $dis:ident) => {
        impl From<$ty> for Value {
            fn from(i: $ty) -> Value {
                Value::$dis(i.into())
            }
        }
    };
}

impl_value_from!(f64 => Float);
impl<'a> From<&'a str> for Value {
    fn from(i: &'a str) -> Value {
        Value::String(i.into())
    }
}
impl_value_from!(String => String);
impl_value_from!(Vector2<f64> => Vec2);
impl_value_from!(Vector3<f64> => Vec3);
impl_value_from!(Vector4<f64> => Vec4);
impl_value_from!(Matrix4<f64> => Mat4);
impl_value_from!(Color => Color);
impl_value_from!(Path2D => Path2D);
impl_value_from!(StrokeWeight => StrokeWeight);
impl_value_from!(Vec<Drawable> => Drawables);
impl From<Drawable> for Value {
    fn from(i: Drawable) -> Value {
        Value::Drawables(vec![i])
    }
}
impl_value_from!(TextureRef => Texture);
impl_value_from!(Vec<u8> => Raw);
impl_value_from!(Arc<Any + Send + Sync> => Any);

/// Trait for types that may be converted to a value.
pub trait TryFromValue
where
    Self: Sized,
{
    /// Attempts to extract this type from a value and returns an error if it fails.
    fn try_from(value: Value) -> Option<Self>;
    /// Attempts to extract this type from a reference to a value and returns an error if it fails.
    fn try_from_ref(value: &Value) -> Option<&Self>;
}

impl TryFromValue for Value {
    fn try_from(value: Value) -> Option<Value> {
        Some(value)
    }
    fn try_from_ref(value: &Value) -> Option<&Value> {
        Some(value)
    }
}

macro_rules! impl_try_from_value {
    ($dis:ident => $ty:ty) => {
        impl TryFromValue for $ty {
            fn try_from(value: Value) -> Option<$ty> {
                match value {
                    Value::$dis(v) => Some(v),
                    _ => None,
                }
            }
            fn try_from_ref(value: &Value) -> Option<&$ty> {
                match value {
                    Value::$dis(ref v) => Some(v),
                    _ => None,
                }
            }
        }
    };
}

impl_try_from_value!(Float => f64);
impl_try_from_value!(String => String);
impl_try_from_value!(Vec2 => Vector2<f64>);
impl_try_from_value!(Vec3 => Vector3<f64>);
impl_try_from_value!(Vec4 => Vector4<f64>);
impl_try_from_value!(Mat4 => Matrix4<f64>);
impl_try_from_value!(Color => Color);
impl_try_from_value!(Path2D => Path2D);
impl_try_from_value!(StrokeWeight => StrokeWeight);
impl_try_from_value!(Drawables => Vec<Drawable>);
impl_try_from_value!(Texture => TextureRef);
impl_try_from_value!(Raw => Vec<u8>);
impl_try_from_value!(Any => Arc<Any + Send + Sync>);

impl PartialEq for Value {
    fn eq(&self, rhs: &Value) -> bool {
        match (self, rhs) {
            (Value::Float(a), Value::Float(b)) => a.eq(b),
            (Value::String(a), Value::String(b)) => a.eq(b),
            (Value::Vec2(a), Value::Vec2(b)) => a.eq(b),
            (Value::Vec3(a), Value::Vec3(b)) => a.eq(b),
            (Value::Vec4(a), Value::Vec4(b)) => a.eq(b),
            (Value::Mat4(a), Value::Mat4(b)) => a.eq(b),
            (Value::Color(a), Value::Color(b)) => a.eq(b),
            (Value::Path2D(a), Value::Path2D(b)) => a.eq(b),
            (Value::StrokeWeight(a), Value::StrokeWeight(b)) => a.eq(b),
            (Value::Drawables(a), Value::Drawables(b)) => a.eq(b),
            (Value::Texture(a), Value::Texture(b)) => a.eq(b),
            (Value::Raw(a), Value::Raw(b)) => a.eq(b),
            (Value::Any(_), Value::Any(_)) => false,
            _ => false,
        }
    }
}
