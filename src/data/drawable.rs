use crate::data::Shape;
use crate::node::NodeRef;

#[derive(Debug, Clone, PartialEq)]
pub struct Drawable {
    pub id: (NodeRef, u64),
    pub shape: Shape,
}
