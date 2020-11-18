mod access;
mod read;
mod view;
mod write;

use core::cmp::min;

pub use self::{
    access::{Access, AccessComponent, AccessKind, AccessRef, ArchetypeAccess},
    read::{read, Read},
    view::{ArchetypeView, ChunkView, View},
    write::{write, Write},
};

pub struct EntityIter<'a, A, V: View<'a>> {
    next: usize,
    next_chunk: usize,
    chunk_capacity: usize,
    len: usize,

    chunk: Option<V::ChunkView>,
    archerype: Option<(ArchetypeAccess<'a>, V::ArchetypeView)>,

    view: V,
    archetypes: A,
}

pub fn iter_entities<'a, A, V: View<'a>>(view: V, archetypes: A) -> EntityIter<'a, A, V>
where
    A: Iterator<Item = ArchetypeAccess<'a>>,
{
    EntityIter {
        next: 0,
        next_chunk: 0,
        chunk_capacity: 0,
        len: 0,
        chunk: None,
        archerype: None,

        view,
        archetypes,
    }
}

impl<'a, A, V> Iterator for EntityIter<'a, A, V>
where
    A: Iterator<Item = ArchetypeAccess<'a>>,
    V: View<'a>,
{
    type Item = V::EntityView;

    fn next(&mut self) -> Option<V::EntityView> {
        if self.chunk.is_some() && self.next == self.chunk_capacity {
            self.chunk.take();
            self.next = 0;
            self.len -= self.chunk_capacity;
            self.next_chunk += 1;
            self.chunk_capacity = min(self.chunk_capacity, self.len);
        }

        let chunk = match &mut self.chunk {
            Some(chunk) => chunk,
            slot => {
                if self.archerype.is_some() && self.len == 0 {
                    self.archerype.take();
                }

                let chunk = match &mut self.archerype {
                    Some((archetype, view)) => unsafe { view.chunk(self.next_chunk, *archetype) },
                    slot => {
                        let mut archetype = self.archetypes.next()?;
                        self.len = archetype.len();

                        while self.len == 0 {
                            archetype = self.archetypes.next()?;
                            self.len = archetype.len();
                        }

                        self.next_chunk = 1;
                        self.next = 0;
                        self.chunk_capacity = min(archetype.chunk_capacity(), self.len);

                        let (_, archetype_view) =
                            slot.get_or_insert((archetype, self.view.view(archetype)));

                        unsafe { archetype_view.chunk(0, archetype) }
                    }
                };

                slot.get_or_insert(chunk)
            }
        };

        todo!()
    }
}
