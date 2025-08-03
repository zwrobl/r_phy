use crate::resources::framebuffer::{References, Transitions};
use type_kit::Nil;

use super::{Subpass, TransitionList};

pub struct EmptyRenderPassTransitions {}

impl TransitionList<Nil> for EmptyRenderPassTransitions {
    fn transitions() -> Transitions<Nil> {
        unreachable!()
    }
}

pub struct EmptySubpass {}

impl Subpass<Nil> for EmptySubpass {
    fn references() -> References<Nil> {
        unreachable!()
    }
}
