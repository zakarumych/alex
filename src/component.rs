use std::{
    alloc::Layout,
    any::TypeId,
    cmp::{Ord, Ordering, PartialOrd},
};

/// Component is basic block of data in ECS
pub trait Component: Sized + Send + Sync + 'static {
    fn component_id() -> ComponentId {
        ComponentId::of::<Self>()
    }

    fn component_info() -> ComponentInfo {
        ComponentInfo::of::<Self>()
    }

    fn type_name() -> &'static str {
        std::any::type_name::<Self>()
    }
}

impl<T> Component for T where T: Send + Sync + 'static {}

/// Component storage information.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentInfo {
    /// Type id of the component.
    /// `TypeId::of::<T>()` for component `T`.
    pub id: ComponentId,
    /// Component layout.
    pub layout: Layout,
    /// Type name.
    pub name: &'static str,
}

impl ComponentInfo {
    pub fn of<T: Component>() -> Self {
        ComponentInfo {
            id: T::component_id(),
            layout: Layout::new::<T>(),
            name: std::any::type_name::<T>(),
        }
    }
}

/// Absolute ordering between types.
/// It takes align into account and orders types with larger alignent first.
/// Which is crutial for storing components in archetype's chunks in the same order.
impl Ord for ComponentInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(&self.id, &other.id)
    }
}

impl PartialOrd for ComponentInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Ord::cmp(self, other).into()
    }
}

/// Contains minimal info required for binsearching component in array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ComponentId {
    pub type_id: TypeId,
}

impl ComponentId {
    pub fn of<T: Component>() -> Self {
        ComponentId {
            type_id: TypeId::of::<T>(),
        }
    }
}
