use crate::support::SharedRef;
use crate::BackingStore;
use crate::InIsolate;
use crate::Isolate;
use crate::Local;
use crate::SharedArrayBuffer;
use crate::ToLocal;
use crate::UniqueRef;

extern "C" {
  fn v8__SharedArrayBuffer__New(
    isolate: *mut Isolate,
    byte_length: usize,
  ) -> *mut SharedArrayBuffer;
  fn v8__SharedArrayBuffer__New__backing_store(
    isolate: *mut Isolate,
    backing_store: *mut SharedRef<BackingStore>,
  ) -> *mut SharedArrayBuffer;
  fn v8__SharedArrayBuffer__New__DEPRECATED(
    isolate: *mut Isolate,
    data_ptr: *mut std::ffi::c_void,
    data_length: usize,
  ) -> *mut SharedArrayBuffer;
  fn v8__SharedArrayBuffer__ByteLength(
    self_: *const SharedArrayBuffer,
  ) -> usize;
  fn v8__SharedArrayBuffer__GetBackingStore(
    self_: *const SharedArrayBuffer,
  ) -> SharedRef<BackingStore>;
}

impl SharedArrayBuffer {
  /// Create a new SharedArrayBuffer. Allocate |byte_length| bytes.
  /// Allocated memory will be owned by a created SharedArrayBuffer and
  /// will be deallocated when it is garbage-collected,
  /// unless the object is externalized.
  pub fn new<'sc>(
    scope: &mut impl ToLocal<'sc>,
    byte_length: usize,
  ) -> Option<Local<'sc, SharedArrayBuffer>> {
    unsafe {
      Local::from_raw(v8__SharedArrayBuffer__New(scope.isolate(), byte_length))
    }
  }

  pub fn new_with_backing_store<'sc>(
    scope: &mut impl ToLocal<'sc>,
    backing_store: &mut SharedRef<BackingStore>,
  ) -> Local<'sc, SharedArrayBuffer> {
    let isolate = scope.isolate();
    let ptr = unsafe {
      v8__SharedArrayBuffer__New__backing_store(isolate, &mut *backing_store)
    };
    unsafe { scope.to_local(ptr) }.unwrap()
  }

  /// Data length in bytes.
  pub fn byte_length(&self) -> usize {
    unsafe { v8__SharedArrayBuffer__ByteLength(self) }
  }

  /// Get a shared pointer to the backing store of this array buffer. This
  /// pointer coordinates the lifetime management of the internal storage
  /// with any live ArrayBuffers on the heap, even across isolates. The embedder
  /// should not attempt to manage lifetime of the storage through other means.
  pub fn get_backing_store(&self) -> SharedRef<BackingStore> {
    unsafe { v8__SharedArrayBuffer__GetBackingStore(self) }
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
    crate::ArrayBuffer::new_backing_store(scope, byte_length)
  }
}
