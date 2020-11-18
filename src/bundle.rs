use {
    crate::{archetype::UninitComponents, component::ComponentInfo},
    core::{
        any::{type_name, TypeId},
        mem::align_of,
    },
};

/// Allows inserting bundles of components into ECS.
/// This trait is implemented for tuples and `DynamicEntity`
/// which is enough for most use-cases.
///
/// Can be safely implemented manually and derived if `"derive"` feature is enabled.
pub trait Bundle {
    /// Calls closure with slice of component type ids.
    /// Components must not be repeated.
    /// Slice must be sorted by component alignment descended and then type id.
    fn with_ids<T>(&self, f: impl FnOnce(&[TypeId]) -> T) -> T;

    /// Calls closure with slice of component infos.
    /// Components must not be repeated.
    /// Slice should be sorted by component alignment descended and then type id.
    fn with_components<T>(&self, f: impl FnOnce(&[ComponentInfo]) -> T) -> T;

    /// Calls closure with slice of component type names.
    /// Components must not be repeated.
    /// Slice should be sorted by component alignment descended and then type id.
    fn with_type_names<T>(&self, f: impl FnOnce(&[&'static str]) -> T) -> T;

    /// Initialize components.
    /// Provided `UninitComponents` expects same set of components that `Self::with_ids` provides into closure.
    fn init_components(self, uninit: UninitComponents<'_>);
}

macro_rules! const_tree_for_token {
    ($a:tt, $($output:tt)*) => { $($output)* }
}

macro_rules! impl_component_source_for_tuple {
    () => {
        impl Bundle for () {
            fn with_ids<T>(&self, f: impl FnOnce(&[TypeId]) -> T) -> T {
                f(&[])
            }

            fn with_type_names<T>(&self, f: impl FnOnce(&[&'static str]) -> T) -> T {
                f(&[])
            }

            fn with_components<T>(&self, f: impl FnOnce(&[ComponentInfo]) -> T) -> T {
                f(&[])
            }

            fn init_components(self, _: UninitComponents<'_>) {}
        }
    };
    ($($a:ident),+) => {
        impl<$($a),+> Bundle for ($($a,)+)
        where
            $($a: 'static,)+
        {
            fn with_ids<T>(&self, f: impl FnOnce(&[TypeId]) -> T) -> T {
                let mut array = [$(
                    (!0 - align_of::<$a>(), TypeId::of::<$a>()),
                )+];
                array.sort_unstable();

                let empty: TypeId = TypeId::of::<()>();

                let mut type_ids = [$(
                    const_tree_for_token!($a, empty),
                )+];

                for (t, &(_, r)) in Iterator::zip(type_ids.iter_mut(), array.iter()) {
                    *t = r;
                }

                f(&type_ids)
            }

            fn with_components<T>(&self, f: impl FnOnce(&[ComponentInfo]) -> T) -> T {
                let mut array = [$(
                    ComponentInfo::new::<$a>(),
                )+];
                array.sort_unstable();
                f(&array)
            }

            fn with_type_names<T>(&self, f: impl FnOnce(&[&'static str]) -> T) -> T {
                let mut array = [$(
                    (!0 - align_of::<$a>(), TypeId::of::<$a>(), type_name::<$a>()),
                )+];
                array.sort_unstable();

                let mut type_names = [$(
                    const_tree_for_token!($a, ""),
                )+];

                for (t, &(_, _, r)) in Iterator::zip(type_names.iter_mut(), array.iter()) {
                    *t = r;
                }

                f(&type_names)
            }

            fn init_components(self, mut uninit: UninitComponents<'_>) {
                let ($($a,)+) = self;

                $(
                    uninit.init_some($a);
                )+
            }
        }
    };
}

impl_component_source_for_tuple!();
impl_component_source_for_tuple!(A);
impl_component_source_for_tuple!(A, B);
impl_component_source_for_tuple!(A, B, C);
impl_component_source_for_tuple!(A, B, C, D);

// pub struct
