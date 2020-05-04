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
    type_id: TypeId,
    /// Component layout.
    layout: Layout,
    /// Type name.
    name: &'static str,
}

impl ComponentInfo {
    pub fn of<T: Component>() -> Self {
        ComponentInfo {
            type_id: TypeId::of::<T>(),
            layout: Layout::new::<T>(),
            name: std::any::type_name::<T>(),
        }
    }

    pub fn id(&self) -> ComponentId {
        ComponentId {
            type_id: self.type_id,
            #[cfg(debug_assertions)]
            name: self.name,
        }
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub fn name(&self) -> &'static str {
        self.name
    }
}

/// Absolute ordering between types.
/// It takes align into account and orders types with larger alignent first.
/// Which is crutial for storing components in archetype's chunks in the same order.
impl Ord for ComponentInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(&self.type_id, &other.type_id)
    }
}

impl PartialOrd for ComponentInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Ord::cmp(self, other).into()
    }
}

/// Contains minimal info required for binsearching component in array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(not(debug_assertions), repr(transparent))]
pub struct ComponentId {
    type_id: TypeId,
    #[cfg(debug_assertions)]
    name: &'static str,
}

impl ComponentId {
    pub fn of<T: Component>() -> Self {
        ComponentId {
            type_id: TypeId::of::<T>(),
            #[cfg(debug_assertions)]
            name: T::type_name(),
        }
    }
}
