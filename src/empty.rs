use crate::{access::ArchetypeAccess, archetype::ChunkSizes, filter::Filter};

pub struct EmptyView;

impl Filter for () {
    fn filter_archetype(&self, _: &ArchetypeInfo) -> bool {
        true
    }
}
