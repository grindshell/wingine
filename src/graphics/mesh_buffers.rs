//! The buffers meshes keep their corners and triangles in, which they share, so that meshes
//! drawn one after another bind nothing between them, and can be drawn in one draw call.

use std::{
    mem::size_of,
    ops::Range,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use super::mesh::GpuVertex;

/// How many corners and triangles' indices a new arena has room for at first. It grows as
/// meshes need, to [`ARENA_BYTES`].
const FIRST_VERTICES: u32 = 4096;
const FIRST_INDICES: u32 = 3 * FIRST_VERTICES;
/// How large an arena's buffers grow, at most, in bytes, unless the device takes less: past
/// that, meshes go in another, and a mesh larger than that has one of its own.
const ARENA_BYTES: u64 = 64 << 20;

/// The buffers meshes keep their corners and triangles in. Each mesh's are in one arena, a
/// vertex buffer and an index buffer, at offsets of its own; an arena grows as meshes need
/// room, copying what it holds, and a mesh dropped gives its room back. Cloning is cheap: the
/// renderer and every mesh hold the same.
#[derive(Clone)]
pub(crate) struct MeshBuffers(Arc<Mutex<Arenas>>);

struct Arenas {
    device: wgpu::Device,
    queue: wgpu::Queue,
    arenas: Vec<Arena>,
    /// How many corners and indices an arena may grow to.
    most: [u32; 2],
}

/// A vertex buffer and an index buffer, and the room left in each.
struct Arena {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    vertex_room: Room,
    index_room: Room,
}

/// Where a mesh's corners and triangles are in the buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    /// Which arena they're in.
    pub arena: u32,
    pub first_vertex: u32,
    pub first_index: u32,
}

impl MeshBuffers {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let most_bytes = ARENA_BYTES.min(device.limits().max_buffer_size);
        let most = [size_of::<GpuVertex>(), size_of::<u32>()]
            .map(|size| u32::try_from(most_bytes / size as u64).unwrap_or(u32::MAX));
        Self::with_most(device, queue, most)
    }

    /// Buffers whose arenas grow to `most` corners and indices.
    fn with_most(device: &wgpu::Device, queue: &wgpu::Queue, most: [u32; 2]) -> Self {
        Self(Arc::new(Mutex::new(Arenas {
            device: device.clone(),
            queue: queue.clone(),
            arenas: Vec::new(),
            most,
        })))
    }

    /// Finds room for `vertices` and `indices`, and writes them there, growing an arena, or
    /// making one, if none has room.
    pub fn place(&self, vertices: &[GpuVertex], indices: &[u32]) -> Place {
        let mut arenas = self.arenas();
        let counts = [vertices.len(), indices.len()].map(|n| n as u32);
        let place = arenas.room_for(counts);
        let arena = &arenas.arenas[place.arena as usize];
        let at = |first: u32, size: usize| u64::from(first) * size as u64;
        let vertex_at = at(place.first_vertex, size_of::<GpuVertex>());
        let index_at = at(place.first_index, size_of::<u32>());
        let queue = &arenas.queue;
        if !vertices.is_empty() {
            queue.write_buffer(&arena.vertices, vertex_at, bytemuck::cast_slice(vertices));
        }
        if !indices.is_empty() {
            queue.write_buffer(&arena.indices, index_at, bytemuck::cast_slice(indices));
        }
        place
    }

    /// Writes `vertices` over those at `place`.
    pub fn write_vertices(&self, place: Place, vertices: &[GpuVertex]) {
        let arenas = self.arenas();
        let buffer = &arenas.arenas[place.arena as usize].vertices;
        let at = u64::from(place.first_vertex) * size_of::<GpuVertex>() as u64;
        arenas
            .queue
            .write_buffer(buffer, at, bytemuck::cast_slice(vertices));
    }

    /// Gives back the room of a mesh of `counts` corners and indices at `place`.
    pub fn free(&self, place: Place, [vertices, indices]: [u32; 2]) {
        let mut arenas = self.arenas();
        let arena = &mut arenas.arenas[place.arena as usize];
        arena
            .vertex_room
            .free(place.first_vertex..place.first_vertex + vertices);
        arena
            .index_room
            .free(place.first_index..place.first_index + indices);
    }

    /// Each arena's vertex buffer and index buffer, in `into`, which is cleared first.
    pub fn buffers(&self, into: &mut Vec<[wgpu::Buffer; 2]>) {
        into.clear();
        let arenas = self.arenas();
        let each = arenas.arenas.iter();
        into.extend(each.map(|arena| [arena.vertices.clone(), arena.indices.clone()]));
    }

    /// How many arenas there are, and how many corners and indices each has room for in all.
    #[cfg(test)]
    pub fn sizes(&self) -> Vec<[u32; 2]> {
        let arenas = self.arenas();
        let each = arenas.arenas.iter();
        each.map(|arena| [arena.vertex_room.size, arena.index_room.size])
            .collect()
    }

    /// The arenas. A panic while they were held left them whole enough: room is taken or given
    /// back only once it's found.
    fn arenas(&self) -> MutexGuard<'_, Arenas> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Arenas {
    /// Room for `counts` corners and indices: in the first arena with room, or that can grow to
    /// have it; or in a new one.
    fn room_for(&mut self, counts: [u32; 2]) -> Place {
        for (index, arena) in self.arenas.iter_mut().enumerate() {
            if let Some([first_vertex, first_index]) = arena.take(counts) {
                return Place {
                    arena: index as u32,
                    first_vertex,
                    first_index,
                };
            }
        }
        for index in 0..self.arenas.len() {
            let arena = &self.arenas[index];
            let rooms = [&arena.vertex_room, &arena.index_room];
            let [vertices, indices] =
                [0, 1].map(|i| grown_size(rooms[i].size, rooms[i].needs(counts[i]), self.most[i]));
            if let (Some(vertices), Some(indices)) = (vertices, indices) {
                self.grow(index, [vertices, indices]);
                let [first_vertex, first_index] = self.arenas[index]
                    .take(counts)
                    .expect("an arena grown to fit has room");
                return Place {
                    arena: index as u32,
                    first_vertex,
                    first_index,
                };
            }
        }
        // A new arena, as large as a first one, or as the mesh, if it's larger.
        let first = [FIRST_VERTICES, FIRST_INDICES];
        let size = [0, 1].map(|i| first[i].min(self.most[i]).max(counts[i]).max(1));
        let arena = Arena {
            vertices: vertex_buffer(&self.device, size[0]),
            indices: index_buffer(&self.device, size[1]),
            vertex_room: Room::new(size[0]),
            index_room: Room::new(size[1]),
        };
        self.arenas.push(arena);
        let index = self.arenas.len() - 1;
        let [first_vertex, first_index] = self.arenas[index]
            .take(counts)
            .expect("a new arena has room");
        Place {
            arena: index as u32,
            first_vertex,
            first_index,
        }
    }

    /// Grows the arena `index` to `size` corners and indices, copying what it holds into new
    /// buffers. What was written to the old ones before is in them before they're copied, since
    /// a queue's writes come before what it's given next.
    fn grow(&mut self, index: usize, size: [u32; 2]) {
        let arena = &mut self.arenas[index];
        if size == [arena.vertex_room.size, arena.index_room.size] {
            return;
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("growing mesh buffers"),
            });
        let mut grow = |buffer: &mut wgpu::Buffer, room: &mut Room, size, made: wgpu::Buffer| {
            if size != room.size {
                encoder.copy_buffer_to_buffer(buffer, 0, &made, 0, buffer.size());
                *buffer = made;
                room.grow(size);
            }
        };
        if size[0] != arena.vertex_room.size {
            let made = vertex_buffer(&self.device, size[0]);
            grow(&mut arena.vertices, &mut arena.vertex_room, size[0], made);
        }
        if size[1] != arena.index_room.size {
            let made = index_buffer(&self.device, size[1]);
            grow(&mut arena.indices, &mut arena.index_room, size[1], made);
        }
        self.queue.submit([encoder.finish()]);
    }
}

/// How large a buffer of `size` grows to have `needs` more at its end, if it can grow to have
/// them, to `most` at most: twice as large, or as large as that needs, if more. It doesn't grow
/// if it needs none.
fn grown_size(size: u32, needs: u32, most: u32) -> Option<u32> {
    if needs == 0 {
        return Some(size);
    }
    let least = size.checked_add(needs).filter(|&least| least <= most)?;
    Some(size.saturating_mul(2).min(most).max(least))
}

impl Arena {
    /// Takes room for `counts` corners and indices, if it has it for both.
    fn take(&mut self, [vertices, indices]: [u32; 2]) -> Option<[u32; 2]> {
        let first_vertex = self.vertex_room.take(vertices)?;
        let Some(first_index) = self.index_room.take(indices) else {
            self.vertex_room.free(first_vertex..first_vertex + vertices);
            return None;
        };
        Some([first_vertex, first_index])
    }
}

fn vertex_buffer(device: &wgpu::Device, vertices: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mesh vertices"),
        size: u64::from(vertices) * size_of::<GpuVertex>() as u64,
        usage: wgpu::BufferUsages::VERTEX
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

fn index_buffer(device: &wgpu::Device, indices: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mesh indices"),
        size: u64::from(indices) * size_of::<u32>() as u64,
        usage: wgpu::BufferUsages::INDEX
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

/// The room left in a buffer of `size` items: the ranges of them free, in order, none touching.
#[derive(Debug)]
struct Room {
    size: u32,
    free: Vec<Range<u32>>,
}

impl Room {
    fn new(size: u32) -> Self {
        let free = std::iter::once(0..size).collect();
        Self { size, free }
    }

    /// Takes `n` items from the first range free with as many, and gives the first of them.
    fn take(&mut self, n: u32) -> Option<u32> {
        if n == 0 {
            return Some(0);
        }
        let index = self
            .free
            .iter()
            .position(|range| range.len() >= n as usize)?;
        let range = &mut self.free[index];
        let first = range.start;
        range.start += n;
        if range.start == range.end {
            self.free.remove(index);
        }
        Some(first)
    }

    /// How many items the buffer would need at its end to give `n`: as many as the range free at
    /// its end lacks.
    fn needs(&self, n: u32) -> u32 {
        let at_end = self.free.last().filter(|range| range.end == self.size);
        n - at_end.map_or(0, |range| range.len() as u32).min(n)
    }

    /// Gives back `range`, joining the ranges free either side of it.
    fn free(&mut self, range: Range<u32>) {
        if range.start >= range.end {
            return;
        }
        let index = self.free.partition_point(|free| free.end <= range.start);
        let joins_before = index > 0 && self.free[index - 1].end == range.start;
        let joins_after = self
            .free
            .get(index)
            .is_some_and(|after| after.start == range.end);
        match (joins_before, joins_after) {
            (true, true) => {
                self.free[index - 1].end = self.free[index].end;
                self.free.remove(index);
            }
            (true, false) => self.free[index - 1].end = range.end,
            (false, true) => self.free[index].start = range.start,
            (false, false) => self.free.insert(index, range),
        }
    }

    /// Makes the buffer `size` items, the room past its old end free.
    fn grow(&mut self, size: u32) {
        let old = std::mem::replace(&mut self.size, size);
        self.free(old..size);
    }
}

#[cfg(test)]
mod tests {
    use bytemuck::Zeroable;

    use super::*;
    use crate::headless::Headless;

    #[test]
    fn meshes_take_room_in_an_arena_that_grows_and_give_it_back() {
        let mut headless = Headless::new().expect("a GPU adapter is available");
        let gpu = &headless.context().gpu;
        let buffers = MeshBuffers::with_most(&gpu.device, &gpu.queue, [1000, 3000]);
        let place = |vertices: usize| {
            let corners = vec![GpuVertex::zeroed(); vertices];
            let indices = vec![0; 3 * vertices];
            let Place {
                arena,
                first_vertex,
                first_index,
            } = buffers.place(&corners, &indices);
            [arena, first_vertex, first_index]
        };
        assert_eq!(place(10), [0, 0, 0]);
        assert_eq!(
            buffers.sizes(),
            [[1000, 3000]],
            "as large as an arena grows"
        );
        assert_eq!(place(900), [0, 10, 30]);
        // Too many for what's left, and it can't grow: in an arena of its own.
        assert_eq!(place(100), [1, 0, 0]);
        let freed = Place {
            arena: 0,
            first_vertex: 0,
            first_index: 0,
        };
        buffers.free(freed, [10, 30]);
        assert_eq!(place(10), [0, 0, 0], "the room given back");
        // Larger than an arena grows: one of its own, as large.
        assert_eq!(place(1500), [2, 0, 0]);
        assert_eq!(buffers.sizes(), [[1000, 3000], [1000, 3000], [1500, 4500]]);

        // Arenas begin smaller than they may grow, and grow twice as large.
        let gpu = &headless.context().gpu;
        let most = [4 * FIRST_VERTICES, 4 * FIRST_INDICES];
        let buffers = MeshBuffers::with_most(&gpu.device, &gpu.queue, most);
        let corners = vec![GpuVertex::zeroed(); FIRST_VERTICES as usize];
        buffers.place(&corners, &[]);
        assert_eq!(buffers.sizes(), [[FIRST_VERTICES, FIRST_INDICES]]);
        let place = buffers.place(&corners[..1], &[0, 0, 0]);
        assert_eq!((place.arena, place.first_vertex), (0, FIRST_VERTICES));
        assert_eq!(buffers.sizes(), [[2 * FIRST_VERTICES, FIRST_INDICES]]);
    }

    #[test]
    fn room_is_taken_first_fit_and_given_back_joined() {
        // Where each range free begins and ends.
        let free = |room: &Room| -> Vec<[u32; 2]> {
            room.free
                .iter()
                .map(|range| [range.start, range.end])
                .collect()
        };
        let mut room = Room::new(10);
        assert_eq!(room.take(4), Some(0));
        assert_eq!(room.take(4), Some(4));
        assert_eq!(room.take(4), None);
        assert_eq!(room.needs(4), 2, "2 are free at its end");
        assert_eq!(room.take(0), Some(0), "nothing takes no room");
        room.free(0..4);
        assert_eq!(free(&room), [[0, 4], [8, 10]]);
        assert_eq!(room.take(3), Some(0));
        room.free(4..8);
        assert_eq!(free(&room), [[3, 10]], "joined either side");
        room.free(0..3);
        assert_eq!(free(&room), [[0, 10]]);
        room.grow(16);
        assert_eq!((room.size, free(&room)), (16, vec![[0, 16]]));
        assert_eq!(room.take(16), Some(0));
        assert_eq!(room.needs(5), 5, "nothing free at its end");
        room.grow(20);
        assert_eq!(free(&room), [[16, 20]]);
    }
}
