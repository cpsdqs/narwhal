use crate::data::Value;
use crate::util::BSMap;
use std::any::Any;
use std::sync::Arc;

/// A node.
#[derive(Debug, Clone)]
pub struct Node {
    /// Enabled flag.
    pub enabled: bool,

    /// Node type name.
    pub node_type: String,

    /// Property data.
    pub(crate) props: BSMap<usize, Value>,
}

impl Node {
    /// Creates a new empty node of the given node type.
    pub fn empty(node_type: String) -> Node {
        Node {
            enabled: true,
            node_type,
            props: BSMap::new(),
        }
    }

    /// Returns true if there are no properties on this node.
    pub fn is_empty(&self) -> bool {
        self.props.is_empty()
    }

    /// Returns the number of property fields.
    pub fn property_count(&self) -> usize {
        self.props.len()
    }

    /// Returns a property value.
    pub fn get(&self, property: usize) -> Option<&Value> {
        self.props.get(&property)
    }

    /// Returns a mutable reference to a property value.
    pub fn get_mut(&mut self, property: usize) -> Option<&mut Value> {
        self.props.get_mut(&property)
    }

    /// Returns a property value, attempting to downcast it.
    pub fn get_any<T: Any + Send + Sync>(&self, property: usize) -> Option<&T> {
        self.get(property).map_or(None, |val| match val {
            Value::Any(any) => any.downcast_ref::<T>(),
            _ => None,
        })
    }

    /// Sets a property value.
    pub fn set<T: Into<Value>>(&mut self, property: usize, value: T) {
        self.props.insert(property, value.into());
    }

    /// Sets a property value with an Any value.
    pub fn set_any<T: Any + Send + Sync>(&mut self, property: usize, value: T) {
        let value: Arc<Any + Send + Sync> = Arc::new(value);
        self.set(property, value);
    }

    /// Returns true if this node contains dynamically typed values.
    pub fn has_dynamic_values(&self) -> bool {
        for (_, v) in self.props.iter() {
            match v {
                Value::Any(_) => return true,
                _ => (),
            }
        }

        false
    }

    /// Iterates over dynamic values.
    pub fn iter_mut_dynamic_values(&mut self) -> impl Iterator<Item = (usize, &mut Value)> {
        self.props
            .iter_mut()
            .filter(|(_, v)| match v {
                Value::Any(_) => true,
                _ => false,
            })
            .map(|(k, v)| (*k, v))
    }
}
