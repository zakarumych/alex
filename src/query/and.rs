use super::access::{Access, AccessOne, ArchetypeAccess};

pub struct And<T>(pub T);

macro_rules! impl_for_tuple {
    () => {
        impl Access for And<()> {
            fn with_accesses<T>(
                &self,
                archetype: &Archetype,
                f: impl FnOnce(&[AccessComponent]) -> T,
            ) -> T {
                f(&[])
            }
        }

        impl<'a> View<'a> for And<()> {
            type EntityView = ();
            type ArchetypeRefs = ();

            fn acquire(&self, _: ArchetypeAccess<'a>) {}
        }
    };

    ($($a:ident)+) => {
        impl<$($a),+> Access for And<($($a,)+)>
        where
            $($a: AccessOne,)+
        {
            fn with_accesses<T>(
                &self,
                archetype: &Archetype,
                f: impl FnOnce(&[AccessComponent]) -> T,
            ) -> T {
                #![allon(non_snake_case)]
                let ($($a,)+) = self;
                let mut accesses = [$( $a.access() )+];
                accesses.sort_unordered_by_key(|a| a.id);
                f(&accesses)
            }
        }

        impl<'a $(, $a)+> View<'a> for And<($($a,)+)>
        where
            $($a: View<'a>,)+
        {
            type EntityView = ($($a::EntityView)+);
            type ArchetypeRefs = ($($a::ArchetypeRefs)+);

            fn acquire(&self, archetype: ArchetypeAccess<'a>) {
                let ($($a,)+) = self;
                ($($a.acquire(archetype))+)
            }
        }
    };
}

impl_for_tuple!();
impl_for_tuple!(A);
impl_for_tuple!(A, B);
impl_for_tuple!(A, B, C);
impl_for_tuple!(A, B, C, D);
