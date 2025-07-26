use ash::vk;

use crate::context::device::{AttachmentProperties, PhysicalDeviceProperties};
use graphics::model::CommonVertex;
use type_kit::{Cons, Nil};

use super::{
    Blend, ColorBlendBuilder, DepthStencil, Multisample, PipelineStatesBuilder, Rasterization,
    VertexAssembly, VertexBindingBuilder, Viewport, ViewportInfo,
};

pub struct TriangleList {}

impl VertexAssembly for TriangleList {
    fn get_input_assembly() -> vk::PipelineInputAssemblyStateCreateInfo {
        vk::PipelineInputAssemblyStateCreateInfo {
            topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            primitive_restart_enable: vk::FALSE,
            ..Default::default()
        }
    }
}

pub struct DepthTestEnabled {}

impl DepthStencil for DepthTestEnabled {
    fn get_state() -> vk::PipelineDepthStencilStateCreateInfo {
        vk::PipelineDepthStencilStateCreateInfo {
            depth_test_enable: vk::TRUE,
            depth_write_enable: vk::TRUE,
            depth_compare_op: vk::CompareOp::LESS_OR_EQUAL,
            ..Default::default()
        }
    }
}

pub struct DepthWriteDisabled {}

impl DepthStencil for DepthWriteDisabled {
    fn get_state() -> vk::PipelineDepthStencilStateCreateInfo {
        vk::PipelineDepthStencilStateCreateInfo {
            depth_test_enable: vk::TRUE,
            depth_write_enable: vk::FALSE,
            depth_compare_op: vk::CompareOp::LESS_OR_EQUAL,
            ..Default::default()
        }
    }
}

pub struct DepthTestDisabled {}

impl DepthStencil for DepthTestDisabled {
    fn get_state() -> vk::PipelineDepthStencilStateCreateInfo {
        vk::PipelineDepthStencilStateCreateInfo {
            depth_test_enable: vk::FALSE,
            depth_write_enable: vk::FALSE,
            ..Default::default()
        }
    }
}

pub struct CullBack {}

impl Rasterization for CullBack {
    fn get_state() -> vk::PipelineRasterizationStateCreateInfo {
        vk::PipelineRasterizationStateCreateInfo {
            polygon_mode: vk::PolygonMode::FILL,
            cull_mode: vk::CullModeFlags::BACK,
            front_face: vk::FrontFace::COUNTER_CLOCKWISE,
            line_width: 1.0,
            ..Default::default()
        }
    }
}

pub struct CullFront {}

impl Rasterization for CullFront {
    fn get_state() -> vk::PipelineRasterizationStateCreateInfo {
        vk::PipelineRasterizationStateCreateInfo {
            polygon_mode: vk::PolygonMode::FILL,
            cull_mode: vk::CullModeFlags::FRONT,
            front_face: vk::FrontFace::COUNTER_CLOCKWISE,
            line_width: 1.0,
            ..Default::default()
        }
    }
}

pub struct ViewportDefault {}

impl Viewport for ViewportDefault {
    fn get_state(image_extent: vk::Extent2D) -> ViewportInfo {
        let viewports = vec![vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: image_extent.width as f32,
            height: image_extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }];
        let scissors = vec![vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: image_extent,
        }];
        let create_info = vk::PipelineViewportStateCreateInfo {
            viewport_count: viewports.len() as u32,
            p_viewports: viewports.as_ptr(),
            scissor_count: scissors.len() as u32,
            p_scissors: scissors.as_ptr(),
            ..Default::default()
        };
        ViewportInfo {
            _viewports: viewports,
            _scissors: scissors,
            create_info,
        }
    }
}

pub struct ViewportHalfLeft {}

impl Viewport for ViewportHalfLeft {
    fn get_state(image_extent: vk::Extent2D) -> ViewportInfo {
        let width_half = image_extent.width / 2;
        let viewports = vec![vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: image_extent.width as f32,
            height: image_extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }];
        let scissors = vec![vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: width_half,
                height: image_extent.height,
            },
        }];
        let create_info = vk::PipelineViewportStateCreateInfo {
            viewport_count: viewports.len() as u32,
            p_viewports: viewports.as_ptr(),
            scissor_count: scissors.len() as u32,
            p_scissors: scissors.as_ptr(),
            ..Default::default()
        };
        ViewportInfo {
            _viewports: viewports,
            _scissors: scissors,
            create_info,
        }
    }
}

pub struct ViewportHalfRight {}

impl Viewport for ViewportHalfRight {
    fn get_state(image_extent: vk::Extent2D) -> ViewportInfo {
        let width_half = image_extent.width / 2;
        let viewports = vec![vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: image_extent.width as f32,
            height: image_extent.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }];
        let scissors = vec![vk::Rect2D {
            offset: vk::Offset2D {
                x: width_half as i32,
                y: 0,
            },
            extent: vk::Extent2D {
                width: width_half,
                height: image_extent.height,
            },
        }];
        let create_info = vk::PipelineViewportStateCreateInfo {
            viewport_count: viewports.len() as u32,
            p_viewports: viewports.as_ptr(),
            scissor_count: scissors.len() as u32,
            p_scissors: scissors.as_ptr(),
            ..Default::default()
        };
        ViewportInfo {
            _viewports: viewports,
            _scissors: scissors,
            create_info,
        }
    }
}

pub struct AttachmentAlphaBlend {}

impl Blend for AttachmentAlphaBlend {
    const BLEND: vk::PipelineColorBlendAttachmentState = vk::PipelineColorBlendAttachmentState {
        blend_enable: vk::TRUE,
        src_color_blend_factor: vk::BlendFactor::SRC_ALPHA,
        dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        color_blend_op: vk::BlendOp::ADD,
        src_alpha_blend_factor: vk::BlendFactor::ONE,
        dst_alpha_blend_factor: vk::BlendFactor::ZERO,
        alpha_blend_op: vk::BlendOp::ADD,
        color_write_mask: vk::ColorComponentFlags::RGBA,
    };
}

pub type AlphaBlend = ColorBlendBuilder<AttachmentAlphaBlend>;

pub struct Multisampled {}

impl Multisample for Multisampled {
    fn get_state(
        device: &PhysicalDeviceProperties,
        attachments: &AttachmentProperties,
    ) -> vk::PipelineMultisampleStateCreateInfo {
        vk::PipelineMultisampleStateCreateInfo {
            rasterization_samples: attachments.msaa_samples,
            sample_shading_enable: device.enabled_features.sample_rate_shading,
            min_sample_shading: 0.2f32,
            ..Default::default()
        }
    }
}

pub type MeshVertexInput<V> = VertexBindingBuilder<Cons<V, Nil>>;

pub type StatesSkybox = PipelineStatesBuilder<
    MeshVertexInput<CommonVertex>,
    TriangleList,
    DepthWriteDisabled,
    CullFront,
    ViewportDefault,
    AlphaBlend,
    Multisampled,
>;

pub type StatesDepthWriteDisabled<V> = PipelineStatesBuilder<
    MeshVertexInput<V>,
    TriangleList,
    DepthWriteDisabled,
    CullBack,
    ViewportDefault,
    AlphaBlend,
    Multisampled,
>;

pub type StatesDepthTestEnabled<V> = PipelineStatesBuilder<
    MeshVertexInput<V>,
    TriangleList,
    DepthTestEnabled,
    CullBack,
    ViewportDefault,
    AlphaBlend,
    Multisampled,
>;
