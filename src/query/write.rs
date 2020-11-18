use core::marker::PhantomData;

pub struct Write<T> {
    marker: PhantomData<fn() -> T>,
}

pub fn write<T>() -> Write<T> {
    Write {
        marker: PhantomData,
    }
}
