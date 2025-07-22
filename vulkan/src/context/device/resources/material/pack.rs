use std::{any::TypeId, convert::Infallible, error::Error, marker::PhantomData};

use type_kit::{Create, Destroy, DestroyResult, DropGuard};

use crate::context::{
    device::{
        command::operation::Graphics,
        descriptor::{Descriptor, DescriptorPool, DescriptorPoolRef, DescriptorSetWriter},
        raw::{
            allocator::AllocatorIndex,
            resources::{
                buffer::{UniformBuffer, UniformBufferInfoBuilder, UniformBufferPartial},
                image::{Image2D, Image2DReader, ImageReader, Texture, TexturePartial},
            },
            unique::layout::presets::{FragmentStage, PodUniform},
            Partial,
        },
    },
    error::{ResourceResult, VkResult},
    Context,
};

use super::{Material, TextureSamplers};

struct MaterialUniformPartial<'a, M: Material> {
    uniform: UniformBufferPartial<PodUniform<M::Uniform, FragmentStage>, Graphics>,
    data: Vec<&'a M::Uniform>,
}

impl<'a, M: Material> Partial for MaterialUniformPartial<'a, M> {
    fn register_memory_requirements<B: crate::context::device::raw::allocator::AllocatorBuilder>(
        &self,
        builder: &mut B,
    ) {
        self.uniform.register_memory_requirements(builder);
    }
}

impl<'b, M: Material> Destroy for MaterialUniformPartial<'b, M> {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.uniform.destroy(context)
    }
}

pub struct MaterialPackData<M: Material> {
    textures: Option<Vec<Texture<Image2D>>>,
    uniforms: Option<DropGuard<UniformBuffer<PodUniform<M::Uniform, FragmentStage>, Graphics>>>,
    descriptors: DropGuard<DescriptorPool<M::DescriptorLayout>>,
}

pub struct MaterialPackPartial<'a, M: Material, R: ImageReader<Type = Image2D>> {
    textures: Option<Vec<TexturePartial<Image2D, R>>>,
    uniforms: Option<MaterialUniformPartial<'a, M>>,
    num_materials: usize,
}

impl<'a, M: Material, R: ImageReader<Type = Image2D>> Partial for MaterialPackPartial<'a, M, R> {
    #[inline]
    fn register_memory_requirements<B: crate::context::device::raw::allocator::AllocatorBuilder>(
        &self,
        builder: &mut B,
    ) {
        self.uniforms.register_memory_requirements(builder);
        self.textures.register_memory_requirements(builder);
    }
}

impl<'b, M: Material, R: ImageReader<Type = Image2D>> Destroy for MaterialPackPartial<'b, M, R> {
    type Context<'a> = &'a Context;

    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        let _ = self.uniforms.destroy(context);
        let _ = self.textures.destroy(context);
        Ok(())
    }
}

pub struct MaterialPack<M: Material> {
    data: MaterialPackData<M>,
}

impl<'a, M: Material> From<&'a MaterialPack<M>> for &'a MaterialPackData<M> {
    fn from(pack: &'a MaterialPack<M>) -> Self {
        &pack.data
    }
}

impl<'a, M: Material> From<&'a mut MaterialPack<M>> for &'a mut MaterialPackData<M> {
    fn from(pack: &'a mut MaterialPack<M>) -> Self {
        &mut pack.data
    }
}

pub struct MaterialPackRef<'a, M: Material> {
    descriptors: DescriptorPoolRef<'a, M::DescriptorLayout>,
    _phantom: PhantomData<M>,
}

impl<'a, M: Material, T: Material> TryFrom<&'a MaterialPack<M>> for MaterialPackRef<'a, T> {
    type Error = &'static str;

    fn try_from(value: &'a MaterialPack<M>) -> Result<Self, Self::Error> {
        if TypeId::of::<M>() == TypeId::of::<T>() {
            Ok(Self {
                descriptors: (&*value.data.descriptors).try_into().unwrap(),
                _phantom: PhantomData,
            })
        } else {
            Err("Invalid Material type")
        }
    }
}

impl<'a, M: Material> MaterialPackRef<'a, M> {
    pub fn get_descriptor(&self, index: usize) -> Descriptor<M::DescriptorLayout> {
        self.descriptors.get(index)
    }
}

impl Context {
    fn prepare_material_pack_textures<'a, M: Material>(
        &self,
        materials: &'a [M],
    ) -> VkResult<Option<Vec<TexturePartial<Image2D, Image2DReader<'a>>>>> {
        if M::NUM_IMAGES > 0 {
            let textures = materials
                .iter()
                .flat_map(|material| {
                    // TODO: It would be better to create vector of iterators and flatten them
                    // Currently unable to do this because of the lifetime of the iterator
                    material
                        .images()
                        .unwrap()
                        .map(|image| TexturePartial::create(Image2DReader::new(image)?, self))
                        .collect::<Vec<_>>()
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Some(textures))
        } else {
            Ok(None)
        }
    }

    fn allocate_material_pack_textures_memory<'a>(
        &self,
        textures: Vec<TexturePartial<Image2D, Image2DReader>>,
        allocator: AllocatorIndex,
    ) -> ResourceResult<Vec<Texture<Image2D>>> {
        textures
            .into_iter()
            .map(|texture| Texture::<Image2D>::create((texture, allocator), self))
            .collect()
    }

    fn prepare_material_pack_uniforms<'a, M: Material>(
        &self,
        materials: &'a [M],
    ) -> Result<Option<MaterialUniformPartial<'a, M>>, Box<dyn Error>> {
        let data = materials
            .iter()
            .filter_map(|material| material.uniform())
            .collect::<Vec<_>>();
        if !data.is_empty() {
            let uniform = UniformBufferPartial::create(
                UniformBufferInfoBuilder::new().with_len(materials.len()),
                self,
            )?;
            Ok(Some(MaterialUniformPartial { uniform, data }))
        } else {
            Ok(None)
        }
    }

    fn allocate_material_pack_uniforms_memory<'a, M: Material>(
        &self,
        partial: MaterialUniformPartial<'a, M>,
        allocator: AllocatorIndex,
    ) -> Result<UniformBuffer<PodUniform<M::Uniform, FragmentStage>, Graphics>, Box<dyn Error>>
    {
        let MaterialUniformPartial { uniform, data } = partial;
        let mut uniform_buffer = UniformBuffer::create((uniform, allocator), self)?;
        for (index, uniform) in data.into_iter().enumerate() {
            *uniform_buffer[index].as_inner_mut() = *uniform;
        }
        Ok(uniform_buffer)
    }

    pub fn prepare_material_pack<'a, M: Material>(
        &self,
        materials: &'a [M],
    ) -> Result<MaterialPackPartial<'a, M, Image2DReader<'a>>, Box<dyn Error>> {
        let textures = self.prepare_material_pack_textures(materials)?;
        let uniforms = self.prepare_material_pack_uniforms(materials)?;
        Ok(MaterialPackPartial {
            textures,
            uniforms,
            num_materials: materials.len(),
        })
    }

    pub fn allocate_material_pack_memory<'a, M: Material>(
        &self,
        partial: MaterialPackPartial<'a, M, Image2DReader<'a>>,
        allocator: AllocatorIndex,
    ) -> Result<MaterialPack<M>, Box<dyn Error>> {
        let MaterialPackPartial {
            textures,
            uniforms,
            num_materials,
        } = partial;
        let textures = if let Some(textures) = textures {
            Some(self.allocate_material_pack_textures_memory(textures, allocator)?)
        } else {
            None
        };
        let uniforms = if let Some(uniforms) = uniforms {
            Some(DropGuard::new(
                self.allocate_material_pack_uniforms_memory(uniforms, allocator)?,
            ))
        } else {
            None
        };
        let writer = DescriptorSetWriter::<M::DescriptorLayout>::new(num_materials);
        let writer = if let Some(textures) = &textures {
            writer.write_images::<TextureSamplers<M>, _>(textures)
        } else {
            writer
        };
        let writer = if let Some(uniforms) = &uniforms {
            writer.write_buffer(uniforms)
        } else {
            writer
        };
        let descriptors = DescriptorPool::create(writer, self)?;
        let data = MaterialPackData {
            textures,
            uniforms,
            descriptors: DropGuard::new(descriptors),
        };
        Ok(MaterialPack { data })
    }

    pub fn load_material_pack<M: Material>(
        &self,
        materials: &[M],
        allocator: AllocatorIndex,
    ) -> Result<MaterialPack<M>, Box<dyn Error>> {
        let pack = self.prepare_material_pack(materials)?;
        let pack = self.allocate_material_pack_memory(pack, allocator)?;
        Ok(pack)
    }
}

impl<M: Material> Destroy for MaterialPack<M> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        if let Some(textures) = self.data.textures.as_mut() {
            let _ = textures
                .iter_mut()
                .try_for_each(|texture| texture.destroy(context));
        }
        if let Some(uniforms) = self.data.uniforms.as_mut() {
            let _ = uniforms.destroy(context);
        }
        let _ = self.data.descriptors.destroy(context);
        Ok(())
    }
}
