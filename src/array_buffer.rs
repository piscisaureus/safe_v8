use std::ffi::c_void;
use std::mem;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr;

use crate::support::long;
use crate::support::Delete;
use crate::support::Opaque;
use crate::support::Shared;
use crate::support::SharedRef;
use crate::support::ToCFn;
use crate::support::UniqueRef;
use crate::ArrayBuffer;
use crate::InIsolate;
use crate::Isolate;
use crate::Local;
use crate::ToLocal;

extern "C" {
  fn v8__ArrayBuffer__Allocator__NewDefaultAllocator() -> *mut Allocator;
  fn v8__ArrayBuffer__Allocator__DELETE(this: &'static mut Allocator);
  fn v8__ArrayBuffer__New__byte_length(
    isolate: *mut Isolate,
    byte_length: usize,
  ) -> *mut ArrayBuffer;
  fn v8__ArrayBuffer__New__backing_store(
    isolate: *mut Isolate,
    backing_store: *mut SharedRef<BackingStore>,
  ) -> *mut ArrayBuffer;
  fn v8__ArrayBuffer__ByteLength(self_: *const ArrayBuffer) -> usize;
  fn v8__ArrayBuffer__GetBackingStore(
    self_: *const ArrayBuffer,
  ) -> SharedRef<BackingStore>;
  fn v8__ArrayBuffer__NewBackingStore__allocate(
    isolate: *mut Isolate,
    byte_length: usize,
  ) -> *mut BackingStore;
  fn v8__ArrayBuffer__NewBackingStore__import(
    data: *mut std::ffi::c_void,
    byte_length: usize,
    deleter: BackingStoreDeleterCallback,
    deleter_Data: *mut std::ffi::c_void,
  ) -> SharedRef<BackingStore>;

  fn v8__BackingStore__Data(
    self_: *const BackingStore,
  ) -> *mut std::ffi::c_void;
  fn v8__BackingStore__ByteLength(self_: &BackingStore) -> usize;
  fn v8__BackingStore__IsShared(self_: &BackingStore) -> bool;
  fn v8__BackingStore__DELETE(self_: &mut BackingStore);
  fn std__shared_ptr__v8__BackingStore__get(
    ptr: *const SharedRef<BackingStore>,
  ) -> *mut BackingStore;
  fn std__shared_ptr__v8__BackingStore__reset(
    ptr: *mut SharedRef<BackingStore>,
  );
  fn std__shared_ptr__v8__BackingStore__use_count(
    ptr: *const SharedRef<BackingStore>,
  ) -> long;
}

/// A thread-safe allocator that V8 uses to allocate |ArrayBuffer|'s memory.
/// The allocator is a global V8 setting. It has to be set via
/// Isolate::CreateParams.
///
/// Memory allocated through this allocator by V8 is accounted for as external
/// memory by V8. Note that V8 keeps track of the memory for all internalized
/// |ArrayBuffer|s. Responsibility for tracking external memory (using
/// Isolate::AdjustAmountOfExternalAllocatedMemory) is handed over to the
/// embedder upon externalization and taken over upon internalization (creating
/// an internalized buffer from an existing buffer).
///
/// Note that it is unsafe to call back into V8 from any of the allocator
/// functions.
///
/// This is called v8::ArrayBuffer::Allocator in C++. Rather than use the
/// namespace array_buffer, which will contain only the Allocator we opt in Rust
/// to allow it to live in the top level: v8::Allocator
#[repr(C)]
pub struct Allocator(Opaque);

/// malloc/free based convenience allocator.
///
/// Caller takes ownership, i.e. the returned object needs to be freed using
/// |delete allocator| once it is no longer in use.
pub fn new_default_allocator() -> UniqueRef<Allocator> {
  unsafe {
    UniqueRef::from_raw(v8__ArrayBuffer__Allocator__NewDefaultAllocator())
  }
}

#[test]
fn test_default_allocator() {
  new_default_allocator();
}

impl Delete for Allocator {
  fn delete(&'static mut self) {
    unsafe { v8__ArrayBuffer__Allocator__DELETE(self) };
  }
}

pub type BackingStoreDeleterCallback = extern "C" fn(
  data: *mut std::ffi::c_void,
  byte_length: usize,
  deleter_data: *mut std::ffi::c_void,
);

/// A wrapper around the backing store (i.e. the raw memory) of an array buffer.
/// See a document linked in http://crbug.com/v8/9908 for more information.
///
/// The allocation and destruction of backing stores is generally managed by
/// V8. Clients should always use standard C++ memory ownership types (i.e.
/// std::unique_ptr and std::shared_ptr) to manage lifetimes of backing stores
/// properly, since V8 internal objects may alias backing stores.
///
/// This object does not keep the underlying |ArrayBuffer::Allocator| alive by
/// default. Use Isolate::CreateParams::array_buffer_allocator_shared when
/// creating the Isolate to make it hold a reference to the allocator itself.
#[repr(C)]
pub struct BackingStore([usize; 6]);

unsafe impl Send for BackingStore {}

impl BackingStore {
  /// Returns a [u8] slice with a lifetime equal to the lifetime of the
  /// BackingStore.
  pub fn data<'a>(&'a self) -> &'a [u8] {
    unsafe {
      std::slice::from_raw_parts::<'a, u8>(
        v8__BackingStore__Data(self) as *mut u8,
        self.byte_length(),
      )
    }
  }
  /// Returns a mutable [u8] slice with a lifetime equal to the lifetime of the
  /// BackingStore.
  pub fn data_mut<'a>(&'a mut self) -> &'a mut [u8] {
    unsafe {
      std::slice::from_raw_parts_mut::<'a, u8>(
        v8__BackingStore__Data(self) as *mut u8,
        self.byte_length(),
      )
    }
  }

  /// The length (in bytes) of this backing store.
  pub fn byte_length(&self) -> usize {
    unsafe { v8__BackingStore__ByteLength(self) }
  }

  /// Indicates whether the backing store was created for an ArrayBuffer or
  /// a SharedArrayBuffer.
  pub fn is_shared(&self) -> bool {
    unsafe { v8__BackingStore__IsShared(self) }
  }
}

impl Delete for BackingStore {
  fn delete(&mut self) {
    unsafe { v8__BackingStore__DELETE(self) };
  }
}

impl Deref for BackingStore {
  type Target = [u8];
  fn deref(&self) -> &Self::Target {
    self.data()
  }
}

impl DerefMut for BackingStore {
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.data_mut()
  }
}

impl Shared for BackingStore {
  fn deref(ptr: *const SharedRef<Self>) -> *mut Self {
    unsafe { std__shared_ptr__v8__BackingStore__get(ptr) }
  }
  fn reset(ptr: *mut SharedRef<Self>) {
    unsafe { std__shared_ptr__v8__BackingStore__reset(ptr) }
  }
  fn use_count(ptr: *const SharedRef<Self>) -> long {
    unsafe { std__shared_ptr__v8__BackingStore__use_count(ptr) }
  }
}

impl ArrayBuffer {
  /// Create a new ArrayBuffer. Allocate |byte_length| bytes.
  /// Allocated memory will be owned by a created ArrayBuffer and
  /// will be deallocated when it is garbage-collected,
  /// unless the object is externalized.
  pub fn new<'sc>(
    scope: &mut impl ToLocal<'sc>,
    byte_length: usize,
  ) -> Local<'sc, ArrayBuffer> {
    let isolate = scope.isolate();
    let ptr =
      unsafe { v8__ArrayBuffer__New__byte_length(isolate, byte_length) };
    unsafe { scope.to_local(ptr) }.unwrap()
  }

  pub fn new_with_backing_store<'sc>(
    scope: &mut impl ToLocal<'sc>,
    backing_store: &mut SharedRef<BackingStore>,
  ) -> Local<'sc, ArrayBuffer> {
    let isolate = scope.isolate();
    let ptr = unsafe {
      v8__ArrayBuffer__New__backing_store(isolate, &mut *backing_store)
    };
    unsafe { scope.to_local(ptr) }.unwrap()
  }

  /// Data length in bytes.
  pub fn byte_length(&self) -> usize {
    unsafe { v8__ArrayBuffer__ByteLength(self) }
  }

  pub fn get_backing_store(&self) -> SharedRef<BackingStore> {
    unsafe { v8__ArrayBuffer__GetBackingStore(self) }
  }

  /// Returns a new standalone BackingStore that is allocated using the array
  /// buffer allocator of the isolate. The result can be later passed to
  /// ArrayBuffer::New.
  ///
  /// If the allocator returns nullptr, then the function may cause GCs in the
  /// given isolate and re-try the allocation. If GCs do not help, then the
  /// function will crash with an out-of-memory error.
  pub fn new_backing_store(
    scope: &mut impl InIsolate,
    byte_length: usize,
  ) -> UniqueRef<BackingStore> {
    unsafe {
      UniqueRef::from_raw(v8__ArrayBuffer__NewBackingStore__allocate(
        scope.isolate(),
        byte_length,
      ))
    }
  }
}

impl<T> From<T> for SharedRef<BackingStore>
where
  T: Deref<Target = [u8]> + DerefMut + 'static,
{
  /// Returns a new standalone BackingStore that takes over the ownership of
  /// the given buffer.
  ///
  /// The destructor of the BackingStore frees owned buffer memory.
  ///
  /// The result can be later passed to ArrayBuffer::New. The raw pointer
  /// to the buffer must not be passed again to any V8 API function.
  fn from(mut buf: T) -> Self {
    let slice: &mut [u8] = &mut *buf;
    let data = slice.as_mut_ptr() as *mut c_void;
    let byte_length = slice.len();

    let deleter = |_: *mut c_void, _: usize, p: *mut c_void| unsafe {
      ptr::read(p as *mut T);
    };
    let deleter_data = &mut buf as *mut _ as *mut c_void;
    mem::forget(buf);

    unsafe {
      v8__ArrayBuffer__NewBackingStore__import(
        data,
        byte_length,
        deleter.to_c_fn(),
        deleter_data,
      )
    }
  }
}
