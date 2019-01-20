//! Color and color management.

use cgmath::Vector4;
use lazy_static::lazy_static;
use lcms_prime::{CIExyY, CIExyYTriple, Profile, ToneCurve};
use vulkano::format::ClearValue;

lazy_static! {
    /// The ACEScg color profile.
    pub static ref ACES_CG: Profile = Profile::new_rgb(
        CIExyY {
            x: 0.32168,
            y: 0.33767,
            Y: 1.,
        },
        CIExyYTriple {
            red: CIExyY {
                x: 0.713,
                y: 0.293,
                Y: 1.,
            },
            green: CIExyY {
                x: 0.165,
                y: 0.830,
                Y: 1.,
            },
            blue: CIExyY {
                x: 0.128,
                y: 0.044,
                Y: 1.,
            },
        },
        [
            ToneCurve::new_gamma(1.).unwrap(),
            ToneCurve::new_gamma(1.).unwrap(),
            ToneCurve::new_gamma(1.).unwrap(),
        ]
    ).unwrap();

    /// The sRGB color profile.
    pub static ref SRGB: Profile = Profile::new_srgb();
}

/// An ACEScg RGBA color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Transparent black.
    pub const CLEAR: Color = Color {
        r: 0.,
        g: 0.,
        b: 0.,
        a: 0.,
    };

    /// Opaque black.
    pub const BLACK: Color = Color {
        r: 0.,
        g: 0.,
        b: 0.,
        a: 1.,
    };

    /// Opaque white.
    pub const WHITE: Color = Color {
        r: 1.,
        g: 1.,
        b: 1.,
        a: 1.,
    };

    /// Converts straight alpha to premultiplied alpha.
    pub fn to_premultiplied_alpha(self) -> Color {
        Color {
            r: self.r * self.a,
            g: self.g * self.a,
            b: self.b * self.a,
            a: self.a,
        }
    }

    /// Converts premultiplied alpha to straight alpha.
    pub fn to_straight_alpha(self) -> Color {
        if self.a == 0. {
            Color::CLEAR
        } else {
            Color {
                r: self.r / self.a,
                g: self.g / self.a,
                b: self.b / self.a,
                a: self.a,
            }
        }
    }
}

impl Into<Vector4<f32>> for Color {
    fn into(self) -> Vector4<f32> {
        Vector4 {
            x: self.r,
            y: self.g,
            z: self.b,
            w: self.a,
        }
    }
}

impl From<(f32, f32, f32, f32)> for Color {
    fn from(i: (f32, f32, f32, f32)) -> Color {
        Color {
            r: i.0,
            g: i.1,
            b: i.2,
            a: i.3,
        }
    }
}

impl From<[f32; 4]> for Color {
    fn from(i: [f32; 4]) -> Color {
        Color {
            r: i[0],
            g: i[1],
            b: i[2],
            a: i[3],
        }
    }
}

impl Into<[f32; 4]> for Color {
    fn into(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

impl Into<ClearValue> for Color {
    fn into(self) -> ClearValue {
        let floats: [f32; 4] = self.into();
        floats.into()
    }
}
