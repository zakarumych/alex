use core::{
    alloc::Layout,
    any::{type_name, TypeId},
    cmp::{Eq, Ord, Ordering, PartialEq, PartialOrd},
    fmt::{self, Display},
    hash::{Hash, Hasher},
    ptr::{drop_in_place, NonNull},
};

#[derive(Clone, Copy, Debug)]
pub struct ComponentInfo {
    id: TypeId,
    layout: Layout,
    name: &'static str,
    drop_in_place: unsafe fn(NonNull<u8>),
}

impl ComponentInfo {
    pub fn new<T: 'static>() -> Self {
        ComponentInfo {
            id: TypeId::of::<T>(),
            layout: Layout::new::<T>(),
            name: type_name::<T>(),
            drop_in_place: erased_drop_in_place::<T>,
        }
    }

    pub fn is<T: 'static>(&self) -> bool {
        self.id == TypeId::of::<T>()
    }

    pub fn id(&self) -> TypeId {
        self.id
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub(crate) fn drop_in_place(&self) -> unsafe fn(NonNull<u8>) {
        self.drop_in_place
    }
}

impl Display for ComponentInfo {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "`{}`", self.name)
    }
}

impl Hash for ComponentInfo {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.id.hash(state)
    }
}

impl PartialEq for ComponentInfo {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ComponentInfo {
    fn assert_receiver_is_total_eq(&self) {
        fn test<T: Eq>(_: &T) {}
        test(&self.id);
    }
}

impl PartialOrd for ComponentInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(Ord::cmp(&self.id, &other.id))
    }
}

impl Ord for ComponentInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(&self.id, &other.id)
    }
}

unsafe fn erased_drop_in_place<T>(ptr: NonNull<u8>) {
    drop_in_place(ptr.as_ptr() as *mut T)
}
