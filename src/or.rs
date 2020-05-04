use {
    crate::{
        access::{Accessor, ArchetypeAccess, ComponentAccess},
        archetype::{ArchetypeInfo, TryArchetypeIter},
        filter::Filter,
        util::{ChainIter, TryIter, Zip},
        view::View,
    },
    std::ops::BitOr,
};

pub struct Or<T> {
    tuple: T,
}

impl<T> Or<(T,)> {
    pub fn new(value: T) -> Self {
        Or { tuple: (value,) }
    }
}

macro_rules! tuple_ors {
    () => {};

    ($($a:ident),+) => {
        impl<'a, $($a),+> Accessor<'a> for Or<($($a,)+)>
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

        impl<$($a),+> Filter for Or<($($a,)+)>
        where
            $($a: Filter,)+
        {
            fn filter_archetype(&self, archetype: &ArchetypeInfo) -> bool {
                #![allow(non_snake_case)]
                let ($($a,)+) = &self.tuple;
                false $(|| $a.filter_archetype(archetype))+
            }
        }

        impl<'a, $($a),+> View<'a> for Or<($($a,)+)>
        where
            $($a: View<'a>,)+
        {
            type EntityView = Zip<($(Option<$a::EntityView>,)+)>;
            type ChunkView = Zip<($(TryIter<$a::ChunkView>,)+)>;
            type ArchetypeView = Zip<($(TryArchetypeIter<$a::ArchetypeView>,)+)>;
            fn view(&self, archetype: &mut ArchetypeAccess<'a>) -> Self::ArchetypeView {
                #![allow(non_snake_case)]
                let ($($a,)+) = &self.tuple;
                Zip(($(
                    if $a.filter_archetype(archetype.info()) {
                        TryArchetypeIter::Just($a.view(archetype))
                    } else {
                        TryArchetypeIter::Nothing(archetype.chunk_sizes())
                    },
                )+))
            }
        }

        impl<Add $(,$a)+> BitOr<Add> for Or<($($a,)+)> {
            type Output = Or<($($a,)+ Add,)>;
            fn bitor(self, rhs: Add) -> Or<($($a,)+ Add,)> {
                #![allow(non_snake_case)]
                let ($($a,)+) = self.tuple;
                Or {
                    tuple: ($($a,)+ rhs,)
                }
            }
        }
    }
}

for_sequences!(tuple_ors);

#[macro_export]
macro_rules! impl_or {
    ($(<$($p:ident $(: $w:tt)?),+>)? for $type:ty) => {
        impl<Rhs $($(, $p $(:$w)?)+)?> std::ops::BitOr<Rhs> for $type {
            type Output = $crate::Or<($type, Rhs)>;
            fn bitor(self, rhs: Rhs) -> $crate::Or<($type, Rhs)> {
                $crate::Or::new(self) | rhs
            }
        }
    };
}
