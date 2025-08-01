use std::{any::TypeId, convert::Infallible, error::Error, marker::PhantomData};

use ash::vk;
use type_kit::{unpack_list, Cons, Create, Destroy, DestroyResult, FromGuard};

use vulkan_low::{
    device::raw::{
        allocator::{AllocatorBuilder, AllocatorIndex},
        resources::{
            buffer::{UniformBuffer, UniformBufferInfoBuilder, UniformBufferPartial},
            command::operation::Graphics,
            descriptor::{Descriptor, DescriptorPool, DescriptorPoolRef, DescriptorSetWriter},
            image::{Image2D, Image2DReader, ImageReader, Texture, TexturePartial},
            layout::presets::{FragmentStage, PodUniform},
            ResourceIndex, ResourceIndexListBuilder,
        },
        Partial,
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
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
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
    textures: Option<Vec<ResourceIndex<Texture<Image2D>>>>,
    uniforms: Option<ResourceIndex<UniformBuffer<PodUniform<M::Uniform, FragmentStage>, Graphics>>>,
    descriptors: ResourceIndex<DescriptorPool<M::DescriptorLayout>>,
}

pub struct MaterialPackPartial<'a, M: Material, R: ImageReader<Type = Image2D>> {
    textures: Option<Vec<TexturePartial<Image2D, R>>>,
    uniforms: Option<MaterialUniformPartial<'a, M>>,
    num_materials: usize,
}

impl<'a, M: Material, R: ImageReader<Type = Image2D>> Partial for MaterialPackPartial<'a, M, R> {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
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

pub struct MaterialPackRef<M: Material> {
    pub descriptors: ResourceIndex<DescriptorPool<M::DescriptorLayout>>,
    _phantom: PhantomData<M>,
}

impl<'a, M: Material, T: Material> TryFrom<&'a MaterialPack<M>> for MaterialPackRef<T> {
    type Error = &'static str;

    fn try_from(value: &'a MaterialPack<M>) -> Result<Self, Self::Error> {
        if TypeId::of::<M>() == TypeId::of::<T>() {
            Ok(Self {
                descriptors: unsafe {
                    ResourceIndex::from_inner(value.data.descriptors.into_inner())
                },
                _phantom: PhantomData,
            })
        } else {
            Err("Invalid Material type")
        }
    }
}

// impl<'a, M: Material> MaterialPackRef<'a, M> {
//     pub fn get_descriptor(&self, index: usize) -> Descriptor<M::DescriptorLayout> {
//         self.descriptors.get(index)
//     }
// }

fn prepare_material_pack_textures<'a, M: Material>(
    context: &Context,
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
                    .map(|image| TexturePartial::create(Image2DReader::new(image)?, context))
                    .collect::<Vec<_>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(textures))
    } else {
        Ok(None)
    }
}

#[inline]
fn allocate_material_pack_textures_memory<'a>(
    context: &Context,
    textures: Vec<TexturePartial<Image2D, Image2DReader>>,
    allocator: AllocatorIndex,
) -> ResourceResult<Vec<ResourceIndex<Texture<Image2D>>>> {
    textures
        .into_iter()
        .map(|texture| context.create_resource((texture, allocator)))
        .collect()
}

fn prepare_material_pack_uniforms<'a, M: Material>(
    context: &Context,
    materials: &'a [M],
) -> Result<Option<MaterialUniformPartial<'a, M>>, Box<dyn Error>> {
    let data = materials
        .iter()
        .filter_map(|material| material.uniform())
        .collect::<Vec<_>>();
    if !data.is_empty() {
        let uniform = UniformBufferPartial::create(
            UniformBufferInfoBuilder::new().with_len(materials.len()),
            context,
        )?;
        Ok(Some(MaterialUniformPartial { uniform, data }))
    } else {
        Ok(None)
    }
}

fn allocate_material_pack_uniforms_memory<'a, M: Material>(
    context: &Context,
    partial: MaterialUniformPartial<'a, M>,
    allocator: AllocatorIndex,
) -> Result<
    ResourceIndex<UniformBuffer<PodUniform<M::Uniform, FragmentStage>, Graphics>>,
    Box<dyn Error>,
> {
    let MaterialUniformPartial { uniform, data } = partial;
    let uniform_buffer = context.create_resource::<UniformBuffer<_, _>, _>((uniform, allocator))?;
    let index_list = ResourceIndexListBuilder::new().push(uniform_buffer).build();
    context
        .opperate_mut(index_list, |unpack_list![uniform_buffer, _rest]| {
            for (index, uniform) in data.into_iter().enumerate() {
                *uniform_buffer[index].as_inner_mut() = *uniform;
            }
            Result::<_, Infallible>::Ok(())
        })
        .unwrap();
    Ok(uniform_buffer)
}

pub fn prepare_material_pack<'a, M: Material>(
    context: &Context,
    materials: &'a [M],
) -> Result<MaterialPackPartial<'a, M, Image2DReader<'a>>, Box<dyn Error>> {
    let textures = prepare_material_pack_textures(context, materials)?;
    let uniforms = prepare_material_pack_uniforms(context, materials)?;
    Ok(MaterialPackPartial {
        textures,
        uniforms,
        num_materials: materials.len(),
    })
}

pub fn allocate_material_pack_memory<'a, M: Material>(
    context: &Context,
    partial: MaterialPackPartial<'a, M, Image2DReader<'a>>,
    allocator: AllocatorIndex,
) -> Result<MaterialPack<M>, Box<dyn Error>> {
    let MaterialPackPartial {
        textures,
        uniforms,
        num_materials,
    } = partial;
    let textures = if let Some(textures) = textures {
        Some(allocate_material_pack_textures_memory(
            context, textures, allocator,
        )?)
    } else {
        None
    };
    let uniforms = if let Some(uniforms) = uniforms {
        Some(allocate_material_pack_uniforms_memory(
            context, uniforms, allocator,
        )?)
    } else {
        None
    };
    let writer = DescriptorSetWriter::<M::DescriptorLayout>::new(num_materials);
    let writer = if let Some(textures) = &textures {
        let image_infos = textures
            .iter()
            .map(|&texture| {
                let index_list = ResourceIndexListBuilder::new().push(texture).build();
                context
                    .opperate_ref(index_list, |unpack_list![texture, _allocator]| {
                        let image_info: vk::DescriptorImageInfo = (&***texture).into();
                        Result::<_, Infallible>::Ok(image_info)
                    })
                    .unwrap()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        writer.write_images::<TextureSamplers<M>>(&image_infos)
    } else {
        writer
    };
    let writer = if let Some(uniforms) = &uniforms {
        let index_list = ResourceIndexListBuilder::new().push(*uniforms).build();
        context
            .opperate_ref(index_list, |unpack_list![uniforms, _allocator]| {
                Result::<_, Infallible>::Ok(writer.write_buffer(uniforms))
            })
            .unwrap()
            .unwrap()
    } else {
        writer
    };
    let descriptors = context.create_resource(writer)?;
    let data = MaterialPackData {
        textures,
        uniforms,
        descriptors,
    };
    Ok(MaterialPack { data })
}

pub fn load_material_pack<M: Material>(
    context: &Context,
    materials: &[M],
    allocator: AllocatorIndex,
) -> Result<MaterialPack<M>, Box<dyn Error>> {
    let pack = prepare_material_pack(context, materials)?;
    let pack = allocate_material_pack_memory(context, pack, allocator)?;
    Ok(pack)
}

impl<M: Material> Destroy for MaterialPack<M> {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        if let Some(textures) = self.data.textures.as_mut() {
            let _ = textures
                .iter_mut()
                .try_for_each(|texture| context.destroy_resource(*texture));
        }
        if let Some(uniforms) = self.data.uniforms.as_mut() {
            let _ = context.destroy_resource(*uniforms);
        }
        let _ = context.destroy_resource(self.data.descriptors);
        Ok(())
    }
}
