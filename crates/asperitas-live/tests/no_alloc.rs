//! The render path allocates nothing: a counting allocator wraps a render call.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

use asperitas_dsp::asperitas::Controls;
use asperitas_live::{ControlSlot, Engine, LoopSource, Source};

struct Counting;

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static A: Counting = Counting;

#[test]
fn render_does_not_allocate() {
    let slot = Arc::new(ControlSlot::new(&Controls::default()));
    let clip: Vec<[f32; 2]> = (0..300).map(|i| [i as f32 * 1e-3; 2]).collect();
    let mut looped = Engine::new(Source::Loop(LoopSource::new(clip)), slot.clone());
    let mut live = Engine::new(Source::External, slot.clone());
    let input = vec![[0.1f32; 2]; 513];
    let mut out = vec![[0.0f32; 2]; 513];

    let before = ALLOCS.with(|c| c.get());
    for _ in 0..50 {
        slot.write(&Controls { knobs: [0.3, 0.6] });
        looped.render(&[], &mut out);
        live.render(&input, &mut out);
    }
    assert_eq!(ALLOCS.with(|c| c.get()), before);
}
