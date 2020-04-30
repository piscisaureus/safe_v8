use std::borrow::Borrow;
use std::convert::TryFrom;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr::null;

use crate::external_references::ExternalReferences;
use crate::support::char;
use crate::support::int;
use crate::support::intptr_t;
use crate::Context;
use crate::Global;
use crate::HandleScope;
use crate::Isolate;
use crate::Scope;

pub use raw::FunctionCodeHandling;
pub use raw::StartupData;

/// Helper class to create a snapshot data blob.
pub struct SnapshotCreator {
  raw: raw::SnapshotCreator,
  root_scope: Scope,
}

impl SnapshotCreator {
  /// Create and enter an isolate, and set it up for serialization.
  /// The isolate is created from scratch.
  pub fn new(external_references: Option<&'static ExternalReferences>) -> Self {
    let external_references_ptr = external_references
      .map(|er| er.as_ptr())
      .unwrap_or_else(null);
    let mut raw = unsafe {
      let mut buf = MaybeUninit::<raw::SnapshotCreator>::uninit();
      raw::v8__SnapshotCreator__CONSTRUCT(&mut buf, external_references_ptr);
      buf.assume_init()
    };
    let isolate =
      unsafe { &mut *raw::v8__SnapshotCreator__GetIsolate(&mut raw) };
    let root_scope = isolate.init(Box::new(()));
    Self { raw, root_scope }
  }
}

impl SnapshotCreator {
  /// Set the default context to be included in the snapshot blob.
  /// The snapshot will not contain the global proxy, and we expect one or a
  /// global object template to create one, to be provided upon deserialization.
  pub fn set_default_context(&mut self, context: Global<Context>) {
    let raw = &mut self.raw as *mut _;
    let scope = &mut HandleScope::new(self);
    let context = context.get(scope).unwrap();
    unsafe { raw::v8__SnapshotCreator__SetDefaultContext(raw, &*context) };
  }

  /// Creates a snapshot data blob.
  /// This must not be called from within a handle scope.
  pub fn create_blob(
    &mut self,
    function_code_handling: FunctionCodeHandling,
  ) -> Option<StartupData> {
    let blob = unsafe {
      raw::v8__SnapshotCreator__CreateBlob(
        &mut self.raw,
        function_code_handling,
      )
    };
    if blob.data.is_null() {
      debug_assert!(blob.raw_size == 0);
      None
    } else {
      debug_assert!(blob.raw_size > 0);
      Some(blob)
    }
  }
}

impl Drop for SnapshotCreator {
  fn drop(&mut self) {
    self.root_scope.drop_root();
    unsafe { raw::v8__SnapshotCreator__DESTRUCT(&mut self.raw) };
  }
}

impl Deref for SnapshotCreator {
  type Target = Scope;
  fn deref(&self) -> &Self::Target {
    &self.root_scope
  }
}

impl DerefMut for SnapshotCreator {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.root_scope
  }
}

impl Deref for StartupData {
  type Target = [u8];
  fn deref(&self) -> &Self::Target {
    let data = self.data as *const u8;
    let len = usize::try_from(self.raw_size).unwrap();
    unsafe { std::slice::from_raw_parts(data, len) }
  }
}

impl AsRef<[u8]> for StartupData {
  fn as_ref(&self) -> &[u8] {
    &**self
  }
}

impl Borrow<[u8]> for StartupData {
  fn borrow(&self) -> &[u8] {
    &**self
  }
}

impl Drop for StartupData {
  fn drop(&mut self) {
    unsafe { raw::v8__StartupData__DESTRUCT(self) }
  }
}

mod raw {
  use super::*;

  #[repr(C)]
  pub enum FunctionCodeHandling {
    Clear,
    Keep,
  }
  #[repr(C)]
  pub struct SnapshotCreator([usize; 1]);

  #[repr(C)]
  pub struct StartupData {
    pub(super) data: *const char,
    pub(super) raw_size: int,
  }

  extern "C" {
    pub fn v8__SnapshotCreator__CONSTRUCT(
      buf: *mut MaybeUninit<SnapshotCreator>,
      external_references: *const intptr_t,
    );
    pub fn v8__SnapshotCreator__DESTRUCT(this: *mut SnapshotCreator);
    pub fn v8__SnapshotCreator__GetIsolate(
      this: *mut SnapshotCreator,
    ) -> *mut Isolate;
    pub fn v8__SnapshotCreator__CreateBlob(
      this: *mut SnapshotCreator,
      function_code_handling: FunctionCodeHandling,
    ) -> StartupData;
    pub fn v8__SnapshotCreator__SetDefaultContext(
      this: *mut SnapshotCreator,
      context: *const Context,
    );
    pub fn v8__StartupData__DESTRUCT(this: *mut StartupData);
  }
}
