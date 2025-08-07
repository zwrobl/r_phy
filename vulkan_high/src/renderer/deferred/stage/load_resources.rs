use std::convert::Infallible;

use type_kit::{list_type, unpack_list, Cons, Destroy, ListMutType, Nil, Task, TypedNil};
use vulkan_low::{
    resources::{
        error::ResourceError,
        framebuffer::{ClearColor, ClearDeptStencil, ClearNone, ClearValueBuilder},
        render_pass::RenderPass,
    },
    Context,
};

use crate::renderer::{
    deferred::{
        presets::{AttachmentsGBuffer, DeferedRenderPass},
        DeferredFrameData,
    },
    frame::FrameCell,
    storage::DrawStorage,
    DestroyTerminator, ExternalResources,
};
pub struct LoadResources;

impl Destroy for LoadResources {
    type Context<'a> = &'a Context;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, _context: Self::Context<'a>) -> type_kit::DestroyResult<Self> {
        Ok(())
    }
}

unsafe impl Task for LoadResources {
    type Dependencies = Nil;
    type ResourceSet = list_type![
        ExternalResources,
        FrameCell<DeferedRenderPass<AttachmentsGBuffer>>,
        DrawStorage,
        TypedNil<DestroyTerminator>
    ];
    type InitializerList = DeferredFrameData;
    type TaskError = ResourceError;
    type TaskResult = ();

    fn execute<'a>(
        &'a mut self,
        unpack_list![context, frame]: ListMutType<'a, Self::ResourceSet>,
    ) -> Result<Self::TaskResult, Self::TaskError> {
        let render_pass = context
            .get_unique_resource::<RenderPass<DeferedRenderPass<AttachmentsGBuffer>>, _>()?;
        let clear_values = ClearValueBuilder::new()
            .push(ClearNone)
            .push(ClearDeptStencil::new(1.0, 0))
            .push(ClearColor::new([0.0, 0.0, 0.0, 1.0]))
            .push(ClearColor::new([0.0, 0.0, 0.0, 1.0]))
            .push(ClearColor::new([0.0, 0.0, 0.0, 1.0]))
            .push(ClearColor::new([0.0, 0.0, 0.0, 1.0]));
        frame.primary_command = Some(
            context
                .start_recording(frame.primary_command.take().unwrap())
                .push(&render_pass.begin(&frame.swapchain_frame, &clear_values))
                .stop_recording(),
        );
        Ok(())
    }
}
