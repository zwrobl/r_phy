use std::{convert::Infallible, marker::PhantomData};

use ash::vk;
use type_kit::{Create, CreateResult, Destroy, DestroyResult, FromGuard};

use crate::{
    resources::{
        error::ResourceError,
        image::{ImageInfo, MipInfo},
    },
    Context,
};

pub trait FilterType: 'static {
    const FILTER: vk::Filter;
    const MIP_FILTER: vk::SamplerMipmapMode;
}

#[derive(Debug, Clone, Copy)]
pub struct Linear;

impl FilterType for Linear {
    const FILTER: vk::Filter = vk::Filter::LINEAR;
    const MIP_FILTER: vk::SamplerMipmapMode = vk::SamplerMipmapMode::LINEAR;
}

#[derive(Debug, Clone, Copy)]
pub struct Nearest;

impl FilterType for Nearest {
    const FILTER: vk::Filter = vk::Filter::NEAREST;
    const MIP_FILTER: vk::SamplerMipmapMode = vk::SamplerMipmapMode::NEAREST;
}

pub trait AddressMode: 'static {
    const MODE: vk::SamplerAddressMode;
}

#[derive(Debug, Clone, Copy)]
pub struct EdgeClamp;

impl AddressMode for EdgeClamp {
    const MODE: vk::SamplerAddressMode = vk::SamplerAddressMode::CLAMP_TO_EDGE;
}

#[derive(Debug, Clone, Copy)]
pub struct BorderClamp;

impl AddressMode for BorderClamp {
    const MODE: vk::SamplerAddressMode = vk::SamplerAddressMode::CLAMP_TO_BORDER;
}

#[derive(Debug, Clone, Copy)]
pub struct Repeat;

impl AddressMode for Repeat {
    const MODE: vk::SamplerAddressMode = vk::SamplerAddressMode::REPEAT;
}

#[derive(Debug, Clone, Copy)]
pub struct MirrorRepeat;

impl AddressMode for MirrorRepeat {
    const MODE: vk::SamplerAddressMode = vk::SamplerAddressMode::MIRRORED_REPEAT;
}

#[derive(Debug, Clone, Copy)]
pub enum BorderColor {
    Transparent,
    White,
    Black,
}

impl Default for BorderColor {
    #[inline]
    fn default() -> Self {
        BorderColor::Black
    }
}

#[derive(Debug)]
enum FormatRepr {
    Float,
    Integer,
}

#[derive(Debug)]
enum FormatTransparency {
    Opaque,
    Transparent,
}

impl BorderColor {
    fn get_vk_border_color(&self, format: vk::Format) -> vk::BorderColor {
        let (repr, alpha) = match format {
            vk::Format::R8_UINT
            | vk::Format::R8G8_UINT
            | vk::Format::R8G8B8_UINT
            | vk::Format::B8G8R8_UINT => (FormatRepr::Integer, FormatTransparency::Opaque),
            vk::Format::R8G8B8A8_UINT | vk::Format::B8G8R8A8_UINT => {
                (FormatRepr::Integer, FormatTransparency::Transparent)
            }
            vk::Format::R8_UNORM
            | vk::Format::R8G8_UNORM
            | vk::Format::R8G8B8_UNORM
            | vk::Format::B8G8R8_UNORM
            | vk::Format::R8_SRGB
            | vk::Format::R8G8_SRGB
            | vk::Format::R8G8B8_SRGB
            | vk::Format::B8G8R8_SRGB => (FormatRepr::Float, FormatTransparency::Opaque),
            vk::Format::R8G8B8A8_UNORM
            | vk::Format::R8G8B8A8_SRGB
            | vk::Format::B8G8R8A8_UNORM
            | vk::Format::B8G8R8A8_SRGB => (FormatRepr::Float, FormatTransparency::Transparent),
            _ => unimplemented!(),
        };
        match *self {
            BorderColor::Black => match repr {
                FormatRepr::Float => vk::BorderColor::FLOAT_OPAQUE_BLACK,
                FormatRepr::Integer => vk::BorderColor::INT_OPAQUE_BLACK,
            },
            BorderColor::White => match repr {
                FormatRepr::Float => vk::BorderColor::FLOAT_OPAQUE_WHITE,
                FormatRepr::Integer => vk::BorderColor::INT_OPAQUE_WHITE,
            },
            BorderColor::Transparent => match (repr, alpha) {
                (FormatRepr::Float, FormatTransparency::Transparent) => {
                    vk::BorderColor::FLOAT_TRANSPARENT_BLACK
                }
                (FormatRepr::Integer, FormatTransparency::Transparent) => {
                    vk::BorderColor::INT_TRANSPARENT_BLACK
                }
                _ => panic!(
                    "Invalid image format {:?} for border color {:?}",
                    format, self
                ),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct MipBias {
    min_lod: f32,
    max_lod: f32,
}

impl From<MipInfo> for MipBias {
    #[inline]
    fn from(value: MipInfo) -> Self {
        Self {
            min_lod: value.base_mip_level as f32,
            max_lod: (value.base_mip_level + value.level_count) as f32,
        }
    }
}

#[derive(Debug)]
pub struct SamplerCreateInfo<F: FilterType, A: AddressMode> {
    format: vk::Format,
    border: BorderColor,
    anisotropy: Option<f32>,
    mip_bias: Option<MipBias>,
    _phantom: PhantomData<(F, A)>,
}

impl<F: FilterType, A: AddressMode> SamplerCreateInfo<F, A> {
    #[inline]
    pub fn new(image_info: &ImageInfo) -> Self {
        Self {
            format: image_info.format,
            border: BorderColor::Black,
            mip_bias: image_info.mip_info.map(|mip_info| mip_info.into()),
            anisotropy: None,
            _phantom: PhantomData,
        }
    }

    #[inline]
    pub fn with_border_color(self, border: BorderColor) -> Self {
        Self { border, ..self }
    }

    #[inline]
    pub fn with_anisotropy(self, max_anisotropy: f32) -> Self {
        Self {
            anisotropy: Some(max_anisotropy),
            ..self
        }
    }

    fn get_vk_create_info(&self) -> vk::SamplerCreateInfo {
        let mip_bias = self.mip_bias.unwrap_or_default();
        vk::SamplerCreateInfo {
            mag_filter: F::FILTER,
            min_filter: F::FILTER,
            mipmap_mode: F::MIP_FILTER,
            address_mode_u: A::MODE,
            address_mode_v: A::MODE,
            address_mode_w: A::MODE,
            anisotropy_enable: self.anisotropy.is_some() as vk::Bool32,
            max_anisotropy: self.anisotropy.unwrap_or_default(),
            min_lod: mip_bias.min_lod,
            max_lod: mip_bias.max_lod,
            border_color: self.border.get_vk_border_color(self.format),
            ..Default::default()
        }
    }
}

#[derive(Debug)]
pub struct Sampler<F: FilterType, A: AddressMode> {
    sampler: vk::Sampler,
    _phantom: PhantomData<(F, A)>,
}

impl<F: FilterType, A: AddressMode> Sampler<F, A> {
    #[inline]
    pub fn get_vk_sampler(&self) -> vk::Sampler {
        self.sampler
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SamplerRaw {
    sampler: vk::Sampler,
}

impl<F: FilterType, A: AddressMode> Create for Sampler<F, A> {
    type Config<'a> = SamplerCreateInfo<F, A>;

    type CreateError = ResourceError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let sampler = unsafe { context.create_sampler(&config.get_vk_create_info(), None)? };
        Ok(Self {
            sampler,
            _phantom: PhantomData,
        })
    }
}

impl<F: FilterType, A: AddressMode> Destroy for Sampler<F, A> {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_sampler(self.sampler, None);
        }
        Ok(())
    }
}

impl Destroy for SamplerRaw {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        unsafe {
            context.destroy_sampler(self.sampler, None);
        }
        Ok(())
    }
}

impl<F: FilterType, A: AddressMode> FromGuard for Sampler<F, A> {
    type Inner = SamplerRaw;

    #[inline]
    fn into_inner(self) -> Self::Inner {
        Self::Inner {
            sampler: self.sampler,
        }
    }

    #[inline]
    unsafe fn from_inner(inner: Self::Inner) -> Self {
        Self {
            sampler: inner.sampler,
            _phantom: PhantomData,
        }
    }
}
