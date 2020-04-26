use {
    alex::{
        access::*, archetype::*, component::*, entity::*, generation::*, schedule::*, tuples::*,
        view::*, world::*,
    },
    bumpalo::Bump,
    instant::{Duration, Instant},
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "rayon-parallel")]
use rayon::{ThreadPool, ThreadPoolBuilder};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Foo(u32);
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Bar(u32);

fn main() {
    init_logger();
    let mut world = World::new();
    let foo_entity = world.insert(Some((Foo(42),))).next().unwrap();
    let bar_entity = world.insert(Some((Bar(23),))).next().unwrap();
    let foobar_entity = world.insert(Some((Bar(3), Foo(11)))).next().unwrap();

    assert_ne!(foo_entity, bar_entity);

    assert_eq!(world.get_component::<Foo>(&foo_entity), Some(&Foo(42)));
    assert_eq!(world.get_component::<Bar>(&bar_entity), Some(&Bar(23)));

    assert_eq!(world.get_component::<Foo>(&foobar_entity), Some(&Foo(11)));
    assert_eq!(world.get_component::<Bar>(&foobar_entity), Some(&Bar(3)));

    let write_foos = write::<Foo>();
    let read_foos = read::<Foo>();
    let write_bars = write::<Bar>();
    let read_bars = read::<Bar>();

    let started = Instant::now();

    let schedule = Schedule::new()
        .with_query(write_foos, move |mut world: WorldAccess<'_>| {
            let mut foos = world.iter_entities(&write_foos).collect::<Vec<_>>();
            assert_eq!(foos, vec![&Foo(42), &Foo(11)]);
            *foos[0] = Foo(5);
        })
        .with_query(read_foos, move |mut world: WorldAccess<'_>| {
            let foos = world.iter_entities(&read_foos).collect::<Vec<_>>();
            assert_eq!(foos, vec![&Foo(5), &Foo(11)]);
        })
        .with_query(read_bars, move |mut world: WorldAccess<'_>| {
            let bars = world.iter_entities(&read_bars).collect::<Vec<_>>();
            assert_eq!(bars, vec![&Bar(23), &Bar(3)]);
        })
        .with_query((read_bars, read_foos), move |mut world: WorldAccess<'_>| {
            let view = (read_bars, read_foos);
            let foobars = world.iter_entities(&view).collect::<Vec<_>>();
            assert_eq!(foobars, vec![Zip((&Bar(3), &Foo(11)))]);
        });

    #[cfg(feature = "rayon-parallel")]
    schedule.execute_rayon(
        &ThreadPoolBuilder::new().build().unwrap(),
        &mut world,
        &Bump::new(),
    );

    #[cfg(not(feature = "rayon-parallel"))]
    schedule.execute(&mut world, &Bump::new());

    assert_eq!(world.archetypes().len(), 4);

    log::info!("Elapsed {:.10}s", started.elapsed().as_secs_f32());
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
