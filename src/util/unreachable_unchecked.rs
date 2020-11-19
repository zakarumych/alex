#[cfg(debug_assertions)]
#[track_caller]
pub fn unreachable_unchecked() -> ! {
    unreachable!()
}

#[cfg(not(debug_assertions))]
pub fn unreachable_unchecked() -> ! {
    core::hint::unreachable_unchecked()
}
