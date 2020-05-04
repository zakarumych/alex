use {
    crate::{
        access::{Accessor, ArchetypeAccess, ComponentAccess},
        archetype::ArchetypeInfo,
        filter::Filter,
        util::{ChainIter, Zip},
        view::{EmptyArchetypeView, EmptyChunkView, View},
    },
    std::ops::BitAnd,
};

pub struct And<T> {
    tuple: T,
}

impl And<()> {
    pub fn new() -> And<()> {
        And { tuple: () }
    }
}

macro_rules! tuple_ands {
    () => {
        impl<'a> Accessor<'a> for And<()> {
            type AccessTypes = std::iter::Empty<ComponentAccess>;
            fn access_types(&'a self, _archetype: &ArchetypeInfo) -> Self::AccessTypes {
                std::iter::empty()
            }
        }

        impl Filter for And<()> {
            fn filter_archetype(&self, _archetype: &ArchetypeInfo) -> bool {
                true
            }
        }

        impl<'a> View<'a> for And<()> {
            type EntityView = ();
            type ChunkView = EmptyChunkView;
            type ArchetypeView = EmptyArchetypeView;
            fn view(&self, archetype: &mut ArchetypeAccess<'a>) -> Self::ArchetypeView {
                EmptyArchetypeView::from_access(archetype)
            }
        }

        impl<Add> BitAnd<Add> for And<()> {
            type Output = And<(Add,)>;
            fn bitand(self, rhs: Add) -> And<(Add,)> {
                And {
                    tuple: (rhs,)
                }
            }
        }
    };

    ($($a:ident),+) => {
        impl<'a, $($a),+> Accessor<'a> for And<($($a,)+)>
        where
            $($a: Accessor<'a>,)+
        {
            type AccessTypes = ChainIter<ComponentAccess, ($($a::AccessTypes,)+)>;
            fn access_types(&'a self, archetype: &ArchetypeInfo) -> Self::AccessTypes {
                #![allow(non_snake_case)]
                let ($($a,)+) = &self.tuple;
                ChainIter::new(($($a.access_types(archetype),)+))
            }
        }

        impl<$($a),+> Filter for And<($($a,)+)>
        where
            $($a: Filter,)+
        {
            fn filter_archetype(&self, archetype: &ArchetypeInfo) -> bool {
                #![allow(non_snake_case)]
                let ($($a,)+) = &self.tuple;
                true $(&& $a.filter_archetype(archetype))+
            }
        }

        impl<'a, $($a),+> View<'a> for And<($($a,)+)>
        where
            $($a: View<'a>,)+
        {
            type EntityView = Zip<($($a::EntityView,)+)>;
            type ChunkView = Zip<($($a::ChunkView,)+)>;
            type ArchetypeView = Zip<($($a::ArchetypeView,)+)>;
            fn view(&self, archetype: &mut ArchetypeAccess<'a>) -> Self::ArchetypeView {
                #![allow(non_snake_case)]
                let ($($a,)+) = &self.tuple;
                Zip(($($a.view(archetype),)+))
            }
        }

        impl<Add $(,$a)+> BitAnd<Add> for And<($($a,)+)> {
            type Output = And<($($a,)+ Add,)>;
            fn bitand(self, rhs: Add) -> And<($($a,)+ Add,)> {
                #![allow(non_snake_case)]
                let ($($a,)+) = self.tuple;
                And {
                    tuple: ($($a,)+ rhs,)
                }
            }
        }
    }
}

for_sequences!(tuple_ands);

#[macro_export]
macro_rules! impl_and {
    ($(<$($p:ident $(: $w:tt)?),+>)? for $type:ty) => {
        impl<Rhs $($(, $p $(:$w)?)+)?> std::ops::BitAnd<Rhs> for $type {
            type Output = $crate::And<($type, Rhs)>;
            fn bitand(self, rhs: Rhs) -> $crate::And<($type, Rhs)> {
                $crate::And::new() & self & rhs
            }
        }
    };
}
