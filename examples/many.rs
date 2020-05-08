use alex::{bumpalo::Bump, *};

#[cfg(all(feature = "rayon", feature = "parallel"))]
use rayon::ThreadPoolBuilder;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Foo(u32);
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Bar(u32);

fn main() {
    init_logger();
    // Creating new `World` instance.
    let mut world = World::new();

    // Insert new entities with `Foo` component.
    world
        .insert(std::iter::repeat((Foo(42),)).take(100000))
        .count();

    Schedule::new()
        .with_system(read::<Foo>(), |mut world| {
            log::info!("Counted - {}", world.entities_iter(read::<Foo>()).count());
        })
        .execute(&mut world, &Bump::new());
}

fn init_logger() {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            let _ = console_log::init();
        } else {
            let _ = env_logger::try_init();
        }
    }
}
