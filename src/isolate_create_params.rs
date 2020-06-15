use crate::array_buffer;
use crate::array_buffer::Allocator as ArrayBufferAllocator;
use crate::support::char;
use crate::support::int;
use crate::support::intptr_t;
use crate::support::Allocated;
use crate::support::Allocation;
use crate::support::Opaque;
use crate::support::SharedPtr;

use std::any::Any;
use std::convert::TryFrom;
use std::iter::once;
use std::mem::size_of;
use std::mem::MaybeUninit;
use std::ptr::null;

/// Initial configuration parameters for a new Isolate.
#[must_use]
#[derive(Default)]
pub struct CreateParams {
  raw: raw::CreateParams,
  allocations: CreateParamAllocations,
}

impl CreateParams {
  /// Explicitly specify a startup snapshot blob.
  pub fn snapshot_blob(mut self, data: impl Allocated<[u8]>) -> Self {
    let data = Allocation::of(data);
    let header = Allocation::of(raw::StartupData::boxed_header(&data));
    self.raw.snapshot_blob = &*header;
    self.allocations.snapshot_blob_data = Some(data);
    self.allocations.snapshot_blob_header = Some(header);
    self
  }

  /// The ArrayBuffer::ArrayBufferAllocator to use for allocating and freeing
  /// the backing store of ArrayBuffers.
  pub fn array_buffer_allocator(
    mut self,
    array_buffer_allocator: impl Into<SharedPtr<ArrayBufferAllocator>>,
  ) -> Self {
    self.raw.array_buffer_allocator_shared = array_buffer_allocator.into();
    self
  }

  /// Specifies an optional nullptr-terminated array of raw addresses in the
  /// embedder that V8 can match against during serialization and use for
  /// deserialization. This array and its content must stay valid for the
  /// entire lifetime of the isolate.
  pub fn external_references(
    mut self,
    ext_refs: impl Allocated<[intptr_t]>,
  ) -> Self {
    let last_non_null = ext_refs
      .iter()
      .cloned()
      .enumerate()
      .rev()
      .find_map(|(idx, value)| if value != 0 { Some(idx) } else { None });
    let first_null = ext_refs
      .iter()
      .cloned()
      .enumerate()
      .find_map(|(idx, value)| if value == 0 { Some(idx) } else { None });
    match (last_non_null, first_null) {
      (None, _) => {
        // Empty list.
        self.raw.external_references = null();
        self.allocations.external_references = None;
      }
      (_, None) => {
        // List does not have null terminator. Make a copy and add it.
        let ext_refs =
          ext_refs.iter().cloned().chain(once(0)).collect::<Vec<_>>();
        let ext_refs = Allocation::of(ext_refs);
        self.raw.external_references = &ext_refs[0];
        self.allocations.external_references = Some(ext_refs);
      }
      (Some(idx1), Some(idx2)) if idx1 + 1 == idx2 => {
        // List is properly null terminated, we'll use it as-is.
        let ext_refs = Allocation::of(ext_refs);
        self.raw.external_references = &ext_refs[0];
        self.allocations.external_references = Some(ext_refs);
      }
      _ => panic!("unexpected null pointer in external references list"),
    }
    self
  }

  /// Whether calling Atomics.wait (a function that may block) is allowed in
  /// this isolate. This can also be configured via SetAllowAtomicsWait.
  pub fn allow_atomics_wait(mut self, value: bool) -> Self {
    self.raw.allow_atomics_wait = value;
    self
  }

  /// Termination is postponed when there is no active SafeForTerminationScope.
  pub fn only_terminate_in_safe_scope(mut self, value: bool) -> Self {
    self.raw.only_terminate_in_safe_scope = value;
    self
  }

  /// The following parameters describe the offsets for addressing type info
  /// for wrapped API objects and are used by the fast C API
  /// (for details see v8-fast-api-calls.h).
  pub fn embedder_wrapper_type_info_offsets(
    mut self,
    embedder_wrapper_type_index: int,
    embedder_wrapper_object_index: int,
  ) -> Self {
    self.raw.embedder_wrapper_type_index = embedder_wrapper_type_index;
    self.raw.embedder_wrapper_object_index = embedder_wrapper_object_index;
    self
  }

  fn set_fallback_defaults(mut self) -> Self {
    if self.raw.array_buffer_allocator_shared.is_null() {
      self = self.array_buffer_allocator(array_buffer::new_default_allocator());
    }
    self
  }

  pub(crate) fn finalize(mut self) -> (raw::CreateParams, Box<dyn Any>) {
    self = self.set_fallback_defaults();
    let Self { raw, allocations } = self;
    (raw, Box::new(allocations))
  }
}

#[derive(Default)]
struct CreateParamAllocations {
  // Owner of the snapshot data buffer itself.
  snapshot_blob_data: Option<Allocation<[u8]>>,
  // Owns `struct StartupData` which contains just the (ptr, len) tuple in V8's
  // preferred format. We have to heap allocate this because we need to put a
  // stable pointer to it in `CreateParams`.
  snapshot_blob_header: Option<Allocation<raw::StartupData>>,
  external_references: Option<Allocation<[intptr_t]>>,
}

pub(crate) mod raw {
  use super::*;

  #[repr(C)]
  pub(crate) struct CreateParams {
    pub code_event_handler: *const Opaque, // JitCodeEventHandler
    pub constraints: ResourceConstraints,
    pub snapshot_blob: *const StartupData,
    pub counter_lookup_callback: *const Opaque, // CounterLookupCallback
    pub create_histogram_callback: *const Opaque, // CreateHistogramCallback
    pub add_histogram_sample_callback: *const Opaque, // AddHistogramSampleCallback
    pub array_buffer_allocator: *mut ArrayBufferAllocator,
    pub array_buffer_allocator_shared: SharedPtr<ArrayBufferAllocator>,
    pub external_references: *const intptr_t,
    pub allow_atomics_wait: bool,
    pub only_terminate_in_safe_scope: bool,
    pub embedder_wrapper_type_index: int,
    pub embedder_wrapper_object_index: int,
  }

  extern "C" {
    fn v8__Isolate__CreateParams__CONSTRUCT(
      buf: *mut MaybeUninit<CreateParams>,
    );
    fn v8__Isolate__CreateParams__SIZEOF() -> usize;
  }

  impl Default for CreateParams {
    fn default() -> Self {
      let size = unsafe { v8__Isolate__CreateParams__SIZEOF() };
      assert_eq!(size_of::<Self>(), size);
      let mut buf = MaybeUninit::<Self>::uninit();
      unsafe { v8__Isolate__CreateParams__CONSTRUCT(&mut buf) };
      unsafe { buf.assume_init() }
    }
  }

  #[repr(C)]
  pub(crate) struct StartupData {
    pub data: *const char,
    pub raw_size: int,
  }

  impl StartupData {
    pub(super) fn boxed_header(data: &Allocation<[u8]>) -> Box<Self> {
      Box::new(Self {
        data: &data[0] as *const _ as *const char,
        raw_size: int::try_from(data.len()).unwrap(),
      })
    }
  }

  #[repr(C)]
  pub(crate) struct ResourceConstraints {
    code_range_size_: usize,
    max_old_generation_size_: usize,
    max_young_generation_size_: usize,
    max_zone_pool_size_: usize,
    initial_old_generation_size_: usize,
    initial_young_generation_size_: usize,
    stack_limit_: *mut u32,
  }
}
