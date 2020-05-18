# Alex - A lite entity component system.


## Description

Alex is an ECS implementation aimed to be flexible, data-driven and fast.

In this implementation entities are grouped by set of components.
Each group is called `Archetype`.

Systems declare access types to subsets of components of archetypes.
Queries in the system may access only those components that system declared.
If query tries to access something else it will trigger a panic.

Panics are considered OK in this case as it is very easy to spot what access type is missing and either add it to the system or modify query. And this doesn't depend on actual content of the `World`. So there's no way it will not panic immediatelly when mistake is made.

## Systems scheduling

For better CPU cores utilization systems may be executed in parallel on a thread-pool.
Parallel execution requires scheduling to ward off data races.
Alex schedules systems execution based on requested access types
to particular archetypes which are not tied to views the systems will fetch.
Instead views borrows data from granted access types to ensure that borrow is valid.
This allows parallel execution of systems that mutably borrow same component in disjoing sets of archetypes.
And providing ability to compose `View`s inside systems arbitrary within granted access types.

Scheduled system execution is mostly deterministic.
Conflicting systems are always executed in the same oreder as they were added.
If component uses interior mutability (like `Mutex`) then two or more systems may modify component
while borrowing it immutably and thus they can executed in any order or even in parallel.
