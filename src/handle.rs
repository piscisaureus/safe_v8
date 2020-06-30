use std::marker::PhantomData;
use std::mem::transmute;
use std::ops::Deref;
use std::ptr::null;
use std::ptr::NonNull;

use crate::Data;
use crate::HandleScope;
use crate::Isolate;
use crate::IsolateHandle;

extern "C" {
  fn v8__Local__New(isolate: *mut Isolate, other: *const Data) -> *const Data;

  fn v8__Global__New(isolate: *mut Isolate, data: *const Data) -> *const Data;
  fn v8__Global__Reset(data: *const Data);
}

/// An object reference managed by the v8 garbage collector.
///
/// All objects returned from v8 have to be tracked by the garbage
/// collector so that it knows that the objects are still alive.  Also,
/// because the garbage collector may move objects, it is unsafe to
/// point directly to an object.  Instead, all objects are stored in
/// handles which are known by the garbage collector and updated
/// whenever an object moves.  Handles should always be passed by value
/// (except in cases like out-parameters) and they should never be
/// allocated on the heap.
///
/// There are two types of handles: local and persistent handles.
///
/// Local handles are light-weight and transient and typically used in
/// local operations.  They are managed by HandleScopes. That means that a
/// HandleScope must exist on the stack when they are created and that they are
/// only valid inside of the HandleScope active during their creation.
/// For passing a local handle to an outer HandleScope, an EscapableHandleScope
/// and its Escape() method must be used.
///
/// Persistent handles can be used when storing objects across several
/// independent operations and have to be explicitly deallocated when they're no
/// longer used.
///
/// It is safe to extract the object stored in the handle by
/// dereferencing the handle (for instance, to extract the *Object from
/// a Local<Object>); the value will still be governed by a handle
/// behind the scenes and the same rules apply to these values as to
/// their handles.
///
/// Note: Local handles in Rusty V8 differ from the V8 C++ API in that they are
/// never empty. In situations where empty handles are needed, use
/// Option<Local>.
#[repr(C)]
pub struct Local<'s, T>(NonNull<T>, PhantomData<&'s ()>);

impl<'s, T> Copy for Local<'s, T> {}

impl<'s, T> Clone for Local<'s, T> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<'s, T> Local<'s, T> {
  /// Construct a new Local from an existing Handle.
  pub fn new(
    scope: &mut HandleScope<'s, ()>,
    handle: impl Handle<Data = T>,
  ) -> Self {
    let (data, host_isolate) = handle.get_raw_info();
    assert!(
      HostIsolate::new(scope).match_isolate(host_isolate.apply_scope(scope))
    );
    unsafe {
      scope.cast_local(|sd| {
        v8__Local__New(sd.get_isolate_ptr(), data as *const T as *const _)
          as *const T
      })
    }
    .unwrap()
  }

  /// Create a local handle by downcasting from one of its super types.
  /// This function is unsafe because the cast is unchecked.
  pub unsafe fn cast<A>(other: Local<'s, A>) -> Self
  where
    Local<'s, A>: From<Self>,
  {
    transmute(other)
  }

  pub(crate) unsafe fn from_raw(ptr: *const T) -> Option<Self> {
    NonNull::new(ptr as *mut _).map(|nn| Self::from_non_null(nn))
  }

  pub(crate) unsafe fn from_non_null(nn: NonNull<T>) -> Self {
    Self(nn, PhantomData)
  }

  pub(crate) fn as_non_null(self) -> NonNull<T> {
    self.0
  }

  pub(crate) fn slice_into_raw(slice: &[Self]) -> &[*const T] {
    unsafe { &*(slice as *const [Self] as *const [*const T]) }
  }
}

impl<'s, T> Deref for Local<'s, T> {
  type Target = T;
  fn deref(&self) -> &T {
    unsafe { self.0.as_ref() }
  }
}

/// An object reference that is independent of any handle scope. Where
/// a Local handle only lives as long as the HandleScope in which it was
/// allocated, a global handle remains valid until it is explicitly
/// disposed using reset().
///
/// A global handle contains a reference to a storage cell within
/// the V8 engine which holds an object value and which is updated by
/// the garbage collector whenever the object is moved. A new storage
/// cell can be created using the constructor or Global::set and
/// existing handles can be disposed using Global::reset.
#[repr(C)]
pub struct Global<T> {
  data: NonNull<T>,
  isolate_handle: IsolateHandle,
}

impl<T> Global<T> {
  /// Construct a new Global from an existing Handle.
  pub fn new(scope: &mut Isolate, handle: impl Handle<Data = T>) -> Self {
    let (data, host_isolate) = handle.get_raw_info();
    let host_isolate = host_isolate.apply_scope(scope);
    let data = unsafe {
      NonNull::new_unchecked(v8__Global__New(
        host_isolate.get_isolate_ptr(),
        data as *const T as *const Data,
      ) as *const T as *mut T)
    };
    Self {
      data,
      isolate_handle: host_isolate.get_isolate_handle(),
    }
  }

  pub fn get<'a>(&'a self, isolate: &mut Isolate) -> &'a T {
    let (data, host_isolate) = self.get_raw_info();
    assert!(host_isolate.match_isolate(HostIsolate::new(isolate)));
    unsafe { &*data }
  }
}

impl<T> Clone for Global<T> {
  fn clone(&self) -> Self {
    let (data, host_isolate) = self.get_raw_info();
    let data = unsafe {
      NonNull::new_unchecked(v8__Global__New(
        host_isolate.get_isolate_ptr(),
        data as *const T as *const Data,
      ) as *const T as *mut T)
    };
    Self {
      data,
      isolate_handle: host_isolate.get_isolate_handle(),
    }
  }
}

impl<T> Drop for Global<T> {
  fn drop(&mut self) {
    unsafe {
      if self.isolate_handle.get_isolate_ptr().is_null() {
        // This global handle is associated with an Isolate that has already
        // been disposed.
      } else {
        // Destroy the storage cell that contains the contents of this Global.
        v8__Global__Reset(self.data.as_ptr() as *mut Data)
      }
    }
  }
}

pub trait Handle {
  type Data;
  fn get_raw_info(&self) -> (*const Self::Data, HostIsolate);
}

impl<'s, T> Handle for Local<'s, T> {
  type Data = T;
  fn get_raw_info(&self) -> (*const Self::Data, HostIsolate) {
    (&**self, HostIsolate::Scope)
  }
}

impl<'a, 's: 'a, T> Handle for &'a Local<'s, T> {
  type Data = T;
  fn get_raw_info(&self) -> (*const Self::Data, HostIsolate) {
    (&***self, HostIsolate::Scope)
  }
}

impl<T> Handle for Global<T> {
  type Data = T;
  fn get_raw_info(&self) -> (*const Self::Data, HostIsolate) {
    unsafe {
      match self.isolate_handle.get_isolate_ptr() {
        p if p.is_null() => (null(), HostIsolate::Disposed),
        p => (self.data.as_ptr(), HostIsolate::Ptr(p)),
      }
    }
  }
}

impl<'a, T> Handle for &'a Global<T> {
  type Data = T;
  fn get_raw_info(&self) -> (*const Self::Data, HostIsolate) {
    unsafe {
      match self.isolate_handle.get_isolate_ptr() {
        p if p.is_null() => (null(), HostIsolate::Disposed),
        p => (self.data.as_ptr(), HostIsolate::Ptr(p)),
      }
    }
  }
}

impl<'s, T, Rhs: Handle> PartialEq<Rhs> for Local<'s, T>
where
  T: PartialEq<Rhs::Data>,
{
  fn eq(&self, other: &Rhs) -> bool {
    let (d1, i1) = self.get_raw_info();
    let (d2, i2) = other.get_raw_info();
    i1.match_isolate(i2) && unsafe { (*d1).eq(&*d2) }
  }
}

impl<'s, T, Rhs: Handle> PartialEq<Rhs> for Global<T>
where
  T: PartialEq<Rhs::Data>,
{
  fn eq(&self, other: &Rhs) -> bool {
    let (d1, i1) = self.get_raw_info();
    let (d2, i2) = other.get_raw_info();
    i1.match_isolate(i2) && unsafe { (*d1).eq(&*d2) }
  }
}

#[derive(Copy, Clone)]
pub enum HostIsolate {
  Scope,
  Ptr(*mut Isolate),
  Disposed,
}

impl HostIsolate {
  fn new(isolate: &mut Isolate) -> Self {
    Self::Ptr(isolate)
  }

  fn apply_scope(self, scope: &mut Isolate) -> Self {
    match self {
      Self::Scope => Self::Ptr(scope as *mut _),
      _ => self,
    }
  }

  fn match_isolate(self, other: Self) -> bool {
    match (self, other) {
      (Self::Scope, Self::Scope) => true,
      (Self::Ptr(p1), Self::Ptr(p2)) => p1 == p2,
      (Self::Disposed, _) | (_, Self::Disposed) => false,
      // TODO(pisciaureus): currently there's no way to check whether some
      // isolate is also the one activated in the current scope, so we just
      // pretend it's alright. This eventually has to be tightened up.
      _ => true,
    }
  }

  fn get_isolate_ptr(self) -> *mut Isolate {
    match self {
      Self::Ptr(p) if !p.is_null() => p,
      _ => panic!("host Isolate for Handle not available"),
    }
  }

  fn get_isolate_handle(self) -> IsolateHandle {
    unsafe { (*self.get_isolate_ptr()).thread_safe_handle() }
  }
}
