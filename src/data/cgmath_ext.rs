//! CGMath extension traits.

use cgmath::{Matrix4, Vector2, Vector3, Vector4};

macro_rules! vec_ext {
    ($name:ident, $ty:tt; $($f:ident),+) => {
        /// Vector extensions.
        pub trait $name {
            fn into_f32(self) -> $ty<f32>;
            fn into_f64(self) -> $ty<f64>;
        }

        impl $name for $ty<f32> {
            fn into_f32(self) -> $ty<f32> {
                self
            }
            fn into_f64(self) -> $ty<f64> {
                $ty {
                    $(
                        $f: self.$f as f64,
                    )+
                }
            }
        }

        impl $name for $ty<f64> {
            fn into_f32(self) -> $ty<f32> {
                $ty {
                    $(
                        $f: self.$f as f32,
                    )+
                }
            }
            fn into_f64(self) -> $ty<f64> {
                self
            }
        }
    }
}

vec_ext!(Vector2Ext, Vector2; x, y);
vec_ext!(Vector3Ext, Vector3; x, y, z);
vec_ext!(Vector4Ext, Vector4; x, y, z, w);

/// Matrix4 extensions.
pub trait Matrix4Ext {
    fn into_f32(self) -> Matrix4<f32>;
    fn into_f64(self) -> Matrix4<f64>;
}

impl Matrix4Ext for Matrix4<f32> {
    fn into_f32(self) -> Matrix4<f32> {
        self
    }
    fn into_f64(self) -> Matrix4<f64> {
        Matrix4 {
            x: self.x.into_f64(),
            y: self.y.into_f64(),
            z: self.z.into_f64(),
            w: self.w.into_f64(),
        }
    }
}

impl Matrix4Ext for Matrix4<f64> {
    fn into_f32(self) -> Matrix4<f32> {
        Matrix4 {
            x: self.x.into_f32(),
            y: self.y.into_f32(),
            z: self.z.into_f32(),
            w: self.w.into_f32(),
        }
    }
    fn into_f64(self) -> Matrix4<f64> {
        self
    }
}
