use std::any::TypeId;

use bytemuck::{AnyBitPattern, Pod};

use ash::vk;

use crate::resources::layout::{Layout, PushConstant, PushConstantList};

use super::{GraphicsPipeline, GraphicsPipelineConfig};

pub struct PushConstantRangeMapper {
    layout: vk::PipelineLayout,
    ranges: Vec<(TypeId, vk::PushConstantRange)>,
}

impl PushConstantRangeMapper {
    pub fn new<C: GraphicsPipelineConfig>(pipeline: &GraphicsPipeline<C>) -> Self {
        let layout = pipeline.layout().into();
        let ranges = Self::get_ranges::<<C::Layout as Layout>::PushConstants>();
        Self { layout, ranges }
    }

    pub fn map_ref<'a, P: PushConstant + Pod>(
        &self,
        data: impl Into<&'a P>,
    ) -> PushConstantDataRef<'a, P> {
        let data = data.into();
        self.ranges
            .iter()
            .find_map(|(type_id, range)| {
                if *type_id == TypeId::of::<P>() {
                    Some(PushConstantDataRef {
                        layout: self.layout,
                        range: *range,
                        data,
                    })
                } else {
                    None
                }
            })
            .expect(&format!(
                "Failed to map push constant of type {:?}",
                TypeId::of::<P>()
            ))
    }

    pub fn map<P: PushConstant + Pod>(&self, data: impl Into<P>) -> PushConstantData<P> {
        let data = data.into();
        self.ranges
            .iter()
            .find_map(|(type_id, range)| {
                if *type_id == TypeId::of::<P>() {
                    Some(PushConstantData {
                        layout: self.layout,
                        range: *range,
                        data,
                    })
                } else {
                    None
                }
            })
            .expect(&format!(
                "Failed to map push constant of type {:?}",
                TypeId::of::<P>()
            ))
    }

    fn get_next_range<L: PushConstantList>(
        mut ranges: Vec<(TypeId, vk::PushConstantRange)>,
        offset: u32,
    ) -> Vec<(TypeId, vk::PushConstantRange)> {
        if !L::exhausted() {
            let range = L::Item::range(offset);
            ranges.push((TypeId::of::<L::Item>(), range));
            Self::get_next_range::<L::Next>(ranges, range.offset + range.size)
        } else {
            ranges
        }
    }

    fn get_ranges<L: PushConstantList>() -> Vec<(TypeId, vk::PushConstantRange)> {
        Self::get_next_range::<L>(Vec::with_capacity(L::len()), 0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PushConstantDataRef<'a, T: PushConstant + AnyBitPattern> {
    pub layout: vk::PipelineLayout,
    pub range: vk::PushConstantRange,
    pub data: &'a T,
}

#[derive(Debug, Clone, Copy)]
pub struct PushConstantData<T: PushConstant + AnyBitPattern> {
    pub layout: vk::PipelineLayout,
    pub range: vk::PushConstantRange,
    pub data: T,
}

impl<T: PushConstant + AnyBitPattern> PushConstantData<T> {
    pub fn with_data(mut self, data: T) -> Self {
        self.data = data;
        self
    }
}

impl<'a, T: PushConstant + AnyBitPattern, N: PushConstant + AnyBitPattern + From<&'a T>>
    From<PushConstantDataRef<'a, T>> for PushConstantData<N>
{
    fn from(value: PushConstantDataRef<'a, T>) -> Self {
        let data = value.data;
        Self {
            layout: value.layout,
            range: value.range,
            data: N::from(data),
        }
    }
}

impl<'a, T: PushConstant + AnyBitPattern> From<&'a PushConstantData<T>>
    for PushConstantDataRef<'a, T>
{
    fn from(value: &'a PushConstantData<T>) -> Self {
        Self {
            layout: value.layout,
            range: value.range,
            data: &value.data,
        }
    }
}
