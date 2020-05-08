use alex::{bumpalo::Bump, *};

#[cfg(all(feature = "rayon", feature = "parallel"))]
use rayon::ThreadPoolBuilder;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Foo(u32);
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Bar(u32);

fn main() {
    init_logger();
    // Creating new `World` instance.
    let mut world = World::new();

    // Insert new entity with `Foo` component.
    let foo_entity: Entity = world.insert(Some((Foo(42),))).next().unwrap();

    // Insert new entity with `Bar` component.
    let bar_entity: Entity = world.insert(Some((Bar(23),))).next().unwrap();

    // Insert new entity with both `Foo` and `Bar` components.
    let foobar_entity: Entity = world.insert(Some((Bar(3), Foo(11)))).next().unwrap();

    // Created entities are different.
    assert_ne!(foo_entity, bar_entity);
    assert_ne!(foo_entity, foobar_entity);
    assert_ne!(bar_entity, foobar_entity);

    // Check components for each entity.
    assert_eq!(world.get_component::<Foo>(&foo_entity), Some(&Foo(42)));
    assert_eq!(world.get_component::<Bar>(&bar_entity), Some(&Bar(23)));
    assert_eq!(world.get_component::<Foo>(&foobar_entity), Some(&Foo(11)));
    assert_eq!(world.get_component::<Bar>(&foobar_entity), Some(&Bar(3)));

    // Create new systems execution schedule.
    let mut schedule = Schedule::new()
        // Add system that overwrites `Foo`s.
        .with_system(write::<Foo>(), move |mut world: WorldAccess<'_>| {
            // Iterate over all entities with `Foo` component and yield reference to the component.
            // Note how view is not strictly tied to the access used to schedule this system.
            // Even though for ease of use `Write<Foo>` implements both `Accessor` and `View` and can be used in both positions.
            let mut foos: Vec<&mut Foo> = world.entities_iter(write::<Foo>()).collect();
            assert_eq!(foos, vec![&Foo(42), &Foo(11)]);
            *foos[0] = Foo(5);

            // And it is possible to read `Foo`s instead.
            let _foos: Vec<&Foo> = world.entities_iter(read::<Foo>()).collect();
        })
        // Add system that reads `Foo`s.
        .with_system(read::<Foo>(), move |mut world: WorldAccess<'_>| {
            let foos: Vec<&Foo> = world.entities_iter(read::<Foo>()).collect();
            assert_eq!(foos, vec![&Foo(5), &Foo(11)]);
        })
        // Add system that read `Bar`s.
        .with_system(read::<Bar>(), move |mut world: WorldAccess<'_>| {
            let bars: Vec<&Bar> = world.entities_iter(read::<Bar>()).collect();
            assert_eq!(bars, vec![&Bar(23), &Bar(3)]);

            // Count entities with `Foo` component.
            // Here `()` is a view into no components and `Filter::with` adds filter to only access archetypes with `Foo`.
            // No components is accessed and there is no need to have `Foo` reading access.
            let _foos_count = world.entities_iter(().with::<Foo>()).count();
        })
        // Add system that writes both `Bar` and `Foo`s.
        .with_system(
            write::<Bar>() & write::<Foo>(),
            move |mut world: WorldAccess<'_>| {
                // Let's read both components and check content;
                let foobars: Vec<Zip<(&Bar, &Foo)>> = world
                    .entities_iter((read::<Bar>(), read::<Foo>()))
                    .collect();
                assert_eq!(foobars, vec![Zip((&Bar(3), &Foo(11)))]);

                // It is also possible to do nested iterations, but that requires a few more lines of code.
                // First take `Foo` write access out of the `WorldAccess` instance.
                // `Bar` write access stays in `World` instance.
                let mut write_foo_world = world.take(write::<Foo>());

                // Write access to `Foo` and `Bar` is granted only for archetypes with both
                // so filter is necessary.
                for foo in write_foo_world.entities_iter(write::<Foo>().with::<Bar>()) {
                    log::info!("foo: {:?}", foo.0);
                    for bar in world.entities_iter(write::<Bar>().with::<Foo>()) {
                        log::info!("\tbar: {:?}", bar.0);
                    }
                }
            },
        )
        // Add system that writes `Bar` and `Foo` even for entities with only one of them.
        .with_system(
            write::<Bar>() | write::<Foo>(),
            move |mut world: WorldAccess<'_>| {
                // Iterating over `A | B` view will yield tuple of options of those views.
                for Zip((foo, bar)) in world.entities_iter(write::<Foo>() | read::<Bar>()) {
                    log::info!("foo: {:?}, bar: {:?}", foo, bar);
                }

                {
                    // It is possible to reborrow `WorldAccess` to apply destructive changes like taking access types out
                    // while leaving original `WorldAccess` instance intact.
                    let mut world = world.reborrow();

                    // Write access to `Foo` and `Bar` is granted for archetypes with either of them
                    // so filter is unnecessary.
                    // `take_entity_iter` is slightly faster shortcut for `take` followed by `entity_iter`
                    // applicable when `Accessor` and `View` are the same.
                    for foo in world.take_entities_iter(write::<Foo>()) {
                        log::info!("foo: {:?}", foo.0);
                        for bar in world.entities_iter(write::<Bar>()) {
                            log::info!("\tbar: {:?}", bar.0);
                        }
                    }
                }

                // Borrow ends, `WorldAccess` is usable again.
                for foo in world.entities_iter(write::<Foo>()) {
                    log::info!("foo: {:?}", foo.0);
                }
            },
        );

    #[cfg(all(feature = "rayon", feature = "parallel"))]
    schedule.execute_rayon(
        &ThreadPoolBuilder::new().num_threads(1).build().unwrap(),
        &mut world,
        &Bump::new(),
    );

    #[cfg(not(all(feature = "rayon", feature = "parallel")))]
    schedule.execute(&mut world, &Bump::new());
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
