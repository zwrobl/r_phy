use std::{any::TypeId, convert::Infallible, error::Error, marker::PhantomData};

use type_kit::{
    unpack_list, Cons, Create, Destroy, DestroyResult, DropGuard, FromGuard, GenCollectionResult,
};

use vulkan_low::{
    error::{ResourceResult, VkResult},
    index_list,
    memory::allocator::{AllocatorBuilder, AllocatorIndex},
    resources::{
        buffer::{UniformBuffer, UniformBufferInfoBuilder, UniformBufferPartial},
        command::operation::Graphics,
        descriptor::{Descriptor, DescriptorBindingData, DescriptorPool, DescriptorSetWriter},
        image::{DescriptorImageInfo, Image2D, Image2DReader, Texture, TexturePartial},
        layout::presets::{FragmentStage, PodUniform},
        pipeline::{GraphicsPipeline, GraphicsPipelineConfig},
        storage::ResourceIndexListBuilder,
        Partial, ResourceIndex,
    },
    Context,
};

use super::{Material, TextureSamplers};

struct MaterialUniformPartial<'a, M: Material> {
    uniform: DropGuard<UniformBufferPartial<PodUniform<M::Uniform, FragmentStage>, Graphics>>,
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
        let _ = self.uniform.destroy(context);
        Ok(())
    }
}

type PackTexturesPartial<'a> = Vec<DropGuard<TexturePartial<Image2D, Image2DReader<'a>>>>;
type PackTextures = Vec<ResourceIndex<Texture<Image2D>>>;

pub type PackUniform<M> =
    UniformBuffer<PodUniform<<M as graphics::model::Material>::Uniform, FragmentStage>, Graphics>;

pub struct MaterialPackData<M: Material> {
    textures: Option<PackTextures>,
    uniforms: Option<ResourceIndex<PackUniform<M>>>,
    descriptors: ResourceIndex<DescriptorPool<M::DescriptorLayout>>,
}

pub struct MaterialPackPartial<'a, M: Material> {
    textures: Option<PackTexturesPartial<'a>>,
    uniforms: Option<MaterialUniformPartial<'a, M>>,
    num_materials: usize,
}

impl<'a, M: Material> Partial for MaterialPackPartial<'a, M> {
    #[inline]
    fn register_memory_requirements<B: AllocatorBuilder>(&self, builder: &mut B) {
        self.uniforms.register_memory_requirements(builder);
        self.textures.register_memory_requirements(builder);
    }
}

impl<'b, M: Material> Destroy for MaterialPackPartial<'b, M> {
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
    descriptors: ResourceIndex<DescriptorPool<M::DescriptorLayout>>,
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

impl<M: Material> MaterialPackRef<M> {
    #[inline]
    pub fn get_descriptor_binding_data<P: GraphicsPipelineConfig>(
        &self,
        context: &Context,
        descriptor_index: u32,
        pipeline_index: ResourceIndex<GraphicsPipeline<P>>,
    ) -> GenCollectionResult<DescriptorBindingData> {
        context
            .operate_ref(
                index_list![self.descriptors, pipeline_index],
                |unpack_list![pipeline, material_descriptor]| {
                    let descriptor = material_descriptor.get(descriptor_index as usize);
                    let binding = descriptor.get_binding_data(pipeline)?;
                    Result::<_, Box<dyn Error>>::Ok(binding)
                },
            )
            .map(|result| result.unwrap())
    }

    #[inline]
    pub fn get_descriptor(
        &self,
        context: &Context,
        index: u32,
    ) -> GenCollectionResult<Descriptor<M::DescriptorLayout>> {
        context
            .operate_ref(
                index_list![self.descriptors],
                |unpack_list![material_descriptor]| {
                    let descriptor = material_descriptor.get(index as usize);
                    Result::<_, Infallible>::Ok(descriptor)
                },
            )
            .map(|result| result.unwrap())
    }
}

fn prepare_material_pack_textures<'a, M: Material>(
    context: &Context,
    materials: &'a [M],
) -> VkResult<Option<PackTexturesPartial<'a>>> {
    if M::NUM_IMAGES > 0 {
        let textures = materials
            .iter()
            .flat_map(|material| {
                // TODO: It would be better to create vector of iterators and flatten them
                // Currently unable to do this because of the lifetime of the iterator
                material
                    .images()
                    .unwrap()
                    .map(|image| {
                        TexturePartial::create(Image2DReader::new(image)?, context)
                            .map(DropGuard::new)
                    })
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
    textures: Vec<DropGuard<TexturePartial<Image2D, Image2DReader<'a>>>>,
    allocator: Option<AllocatorIndex>,
) -> ResourceResult<PackTextures> {
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
        Ok(Some(MaterialUniformPartial {
            uniform: DropGuard::new(uniform),
            data,
        }))
    } else {
        Ok(None)
    }
}

fn allocate_material_pack_uniforms_memory<'a, M: Material>(
    context: &Context,
    partial: MaterialUniformPartial<'a, M>,
    allocator: Option<AllocatorIndex>,
) -> Result<ResourceIndex<PackUniform<M>>, Box<dyn Error>> {
    let MaterialUniformPartial { uniform, data } = partial;
    let uniform_buffer = context.create_resource::<PackUniform<M>, _>((uniform, allocator))?;
    context
        .operate_mut(
            index_list![uniform_buffer],
            |unpack_list![uniform_buffer]| {
                for (index, uniform) in data.into_iter().enumerate() {
                    *uniform_buffer[index].as_inner_mut() = *uniform;
                }
                Result::<_, Infallible>::Ok(())
            },
        )?
        .unwrap();
    Ok(uniform_buffer)
}

pub fn prepare_material_pack<'a, M: Material>(
    context: &Context,
    materials: &'a [M],
) -> Result<MaterialPackPartial<'a, M>, Box<dyn Error>> {
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
    partial: MaterialPackPartial<'a, M>,
    allocator: Option<AllocatorIndex>,
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
                context
                    .operate_ref(index_list![texture], |unpack_list![texture]| {
                        let image_info: DescriptorImageInfo = texture.into();
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
        context
            .operate_ref(index_list![*uniforms], |unpack_list![uniforms]| {
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
    allocator: Option<AllocatorIndex>,
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
