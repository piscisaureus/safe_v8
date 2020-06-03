// Copyright 2019-2020 the Deno authors. All rights reserved. MIT license.

//! This module's public API exports a number of 'scope' types.
//!
//! These types carry information about the state of the V8 Isolate, as well as
//! lifetimes for certain (return) values. More specialized scopes typically
//! deref to more generic scopes, and ultimately they all deref to `Isolate`.
//!
//! The scope types in the public API are all pointer wrappers, and they all
//! point at a heap-allocated struct `data::ScopeData`. `ScopeData` allocations
//! are never shared between scopes; each Handle/Context/CallbackScope gets
//! its own instance.
//!
//! Notes about the available scope types:
//! See also the tests at the end of this file.
//!
//! - `HandleScope<'s, ()>`
//!   - 's = lifetime of local handles created in this scope, and of the scope
//!     itself.
//!   - This type is returned when a HandleScope is constructed from an isolate
//!     (`&mut Isolate` or `&mut OwnedIsolate`) reference.
//!   - A `Context` is _not_ available. Only certain types JavaScript values can
//!     be created: primitive values, templates, and instances of `Context`.
//!   - Derefs to `Isolate`.
//!
//! - `ContextScope<'s>`
//!   - 's = lifetime of the scope itself.
//!   - This type is never constructed; it only exists in the deref chain.
//!   - A `Context` is available; any type of value can be created.
//!   - Derefs to `HandleScope<'s, ()>`.
//!
//! - `HandleScope<'s>`
//!   - 's = lifetime of local handles created in this scope, and of the scope
//!     itself.
//!   - A `Context` is available; any type of value can be created.
//!   - Derefs to `ContextScope<'s>`
//!
//! - ContextScope<'s, P: Scope>
//!   - 's = lifetime of the scope itself.
//!   - A `Context` is available; any type of value can be created.
//!   - Derefs to `P`.
//!   - When a constructed as the child of a `HandleScope<'a, ()>`, the returned
//!     type is `ContextScope<'s, HandleScope<'p>>`. In other words, the parent
//!     HandleScope gets an upgrade to indicate the availability of a `Context`.
//!   - When a new scope is constructed inside this type of scope, the
//!     `ContextScope` wrapper around `P` is erased first, which means that the
//!     child scope is set up as if it had been created with `P` as its parent.
//!
//! - `EscapableHandleScope<'s, 'e>`
//!   - 's = lifetime of local handles created in this scope, and of the scope
//!     itself.
//!   - 'e = lifetime of the HandleScope that will receive the local handle that
//!     is created by `EscapableHandleScope::escape()`.
//!   - A `Context` is available; any type of value can be created.
//!   - Derefs to `HandleScope<'s>`.
//!
//! - `CallbackScope<'s>`
//!   - 's = lifetime of local handles created in this scope, and the value
//!     returned from the callback, and of the scope itself.
//!   - A `Context` is available; any type of value can be created.
//!   - Derefs to `HandleScope<'s>`.
//!   - This scope type is only to be constructed inside embedder defined
//!     callbacks when these are called by V8.
//!   - When a scope is created inside, type is erased to `HandleScope<'s>`.

use std::alloc::alloc;
use std::alloc::Layout;
use std::any::type_name;
use std::cell::Cell;
use std::marker::PhantomData;
use std::mem::size_of;
use std::mem::take;
use std::mem::MaybeUninit;
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr;
use std::ptr::NonNull;

use crate::function::FunctionCallbackInfo;
use crate::function::PropertyCallbackInfo;
use crate::Context;
use crate::Data;
use crate::Isolate;
use crate::Local;
use crate::Message;
use crate::Object;
use crate::OwnedIsolate;
use crate::Primitive;
use crate::PromiseRejectMessage;
use crate::Value;

pub unsafe trait Scope: Sized {}

/// Stack-allocated class which sets the execution context for all operations
/// executed within a local scope. After entering a context, all code compiled
/// and run is compiled and run in this context.
pub struct ContextScope<'s, P = ()> {
  data: NonNull<data::ScopeData>,
  _phantom: PhantomData<&'s mut P>,
}

unsafe impl<'s, P> Scope for ContextScope<'s, P> {}

impl<'s, P: param::NewContextScope<'s>> ContextScope<'s, P> {
  #[allow(clippy::new_ret_no_self)]
  pub fn new(param: &'s mut P, context: Local<Context>) -> P::NewScope {
    let scope_data = param.get_scope_data_mut();
    if scope_data.get_isolate_ptr()
      != unsafe { raw::v8__Context__GetIsolate(&*context) }
    {
      panic!(
        "{} and Context do not belong to the same Isolate",
        type_name::<P>()
      )
    }
    let new_scope_data = scope_data.new_context_scope_data(context);
    new_scope_data.as_scope()
  }
}

impl<'s, P> ContextScope<'s, P> {
  /// Returns the context of the currently running JavaScript, or the context
  /// on the top of the stack if no JavaScript is running.
  pub fn get_current_context(&self) -> Local<'s, Context> {
    let context_ptr = data::ScopeData::get(self).get_current_context();
    unsafe { Local::from_raw(context_ptr) }.unwrap()
  }

  /// Returns either the last context entered through V8's C++ API, or the
  /// context of the currently running microtask while processing microtasks.
  /// If a context is entered while executing a microtask, that context is
  /// returned.
  pub fn get_entered_or_microtask_context(&self) -> Local<'s, Context> {
    let data = data::ScopeData::get(self);
    let isolate_ptr = data.get_isolate_ptr();
    let context_ptr =
      unsafe { raw::v8__Isolate__GetEnteredOrMicrotaskContext(isolate_ptr) };
    unsafe { Local::from_raw(context_ptr) }.unwrap()
  }
}

impl<'s> Deref for ContextScope<'s> {
  type Target = HandleScope<'s, ()>;

  fn deref(&self) -> &Self::Target {
    self.cast_ref()
  }
}

impl<'s> DerefMut for ContextScope<'s> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.cast_mut()
  }
}

impl<'s, P: Scope> Deref for ContextScope<'s, P> {
  type Target = P;

  fn deref(&self) -> &Self::Target {
    self.cast_ref()
  }
}

impl<'s, P: Scope> DerefMut for ContextScope<'s, P> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.cast_mut()
  }
}

impl<'s, P> Drop for ContextScope<'s, P> {
  fn drop(&mut self) {
    data::ScopeData::get_mut(self).notify_scope_dropped();
  }
}

/// A stack-allocated class that governs a number of local handles.
/// After a handle scope has been created, all local handles will be
/// allocated within that handle scope until either the handle scope is
/// deleted or another handle scope is created.  If there is already a
/// handle scope and a new one is created, all allocations will take
/// place in the new handle scope until it is deleted.  After that,
/// new handles will again be allocated in the original handle scope.
///
/// After the handle scope of a local handle has been deleted the
/// garbage collector will no longer track the object stored in the
/// handle and may deallocate it.  The behavior of accessing a handle
/// for which the handle scope has been deleted is undefined.
pub struct HandleScope<'s, C = Context> {
  data: NonNull<data::ScopeData>,
  _phantom: PhantomData<&'s mut C>,
}

unsafe impl<'s, C> Scope for HandleScope<'s, C> {}

impl<'s> HandleScope<'s> {
  #[allow(clippy::new_ret_no_self)]
  pub fn new<P: param::NewHandleScope<'s>>(param: &'s mut P) -> P::NewScope {
    param
      .get_scope_data_mut()
      .new_handle_scope_data()
      .as_scope()
  }
}

impl<'s> HandleScope<'s, ()> {
  pub fn get_isolate(&self) -> &Isolate {
    data::ScopeData::get(self).get_isolate()
  }

  pub fn get_isolate_mut(&mut self) -> &mut Isolate {
    data::ScopeData::get_mut(self).get_isolate_mut()
  }

  pub(crate) fn get_isolate_ptr(&self) -> *mut Isolate {
    data::ScopeData::get(self).get_isolate_ptr()
  }

  /// Schedules an exception to be thrown when returning to JavaScript. When
  /// an exception has been scheduled it is illegal to invoke any
  /// JavaScript operation; the caller must return immediately and only
  /// after the exception has been handled does it become legal to invoke
  /// JavaScript operations.
  ///
  /// This function always returns the `undefined` value.
  pub fn throw_exception(
    &mut self,
    exception: Local<Value>,
  ) -> Local<'s, Value> {
    unsafe {
      self.cast_local(|sd| {
        raw::v8__Isolate__ThrowException(sd.get_isolate_ptr(), &*exception)
      })
    }
    .unwrap()
  }

  pub(crate) unsafe fn cast_local<T>(
    &mut self,
    f: impl FnOnce(&mut data::ScopeData) -> *const T,
  ) -> Option<Local<'s, T>> {
    Local::from_raw(f(data::ScopeData::get_mut(self)))
  }
}

impl<'s> AsRef<Isolate> for HandleScope<'s, ()> {
  fn as_ref(&self) -> &Isolate {
    self.get_isolate()
  }
}

impl<'s> AsMut<Isolate> for HandleScope<'s, ()> {
  fn as_mut(&mut self) -> &mut Isolate {
    self.get_isolate_mut()
  }
}

impl<'s> Deref for HandleScope<'s, ()> {
  type Target = Isolate;

  fn deref(&self) -> &Self::Target {
    self.get_isolate()
  }
}

impl<'s> DerefMut for HandleScope<'s, ()> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.get_isolate_mut()
  }
}

impl<'s> Deref for HandleScope<'s> {
  type Target = ContextScope<'s>;

  fn deref(&self) -> &Self::Target {
    self.cast_ref()
  }
}

impl<'s> DerefMut for HandleScope<'s> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.cast_mut()
  }
}

impl<'s, C> Drop for HandleScope<'s, C> {
  fn drop(&mut self) {
    data::ScopeData::get_mut(self).notify_scope_dropped();
  }
}

/// A HandleScope which first allocates a handle in the current scope
/// which will be later filled with the escape value.
// TODO(piscisaureus): type parameter `C` is not very useful in practice; being
// a source of complexity and potential confusion, it is desirable to
// eventually remove it. Blocker at the time of writing is that there are some
// tests that enter an `EscapableHandleScope` without creating a `ContextScope`
// at all. These tests need to updated first.
pub struct EscapableHandleScope<'s, 'e: 's, C = Context> {
  data: NonNull<data::ScopeData>,
  _phantom:
    PhantomData<(&'s mut raw::HandleScope, &'e mut raw::EscapeSlot, &'s C)>,
}

unsafe impl<'s, 'e: 's, C> Scope for EscapableHandleScope<'s, 'e, C> {}

impl<'s, 'e: 's> EscapableHandleScope<'s, 'e> {
  #[allow(clippy::new_ret_no_self)]
  pub fn new<P: param::NewEscapableHandleScope<'s, 'e>>(
    param: &'s mut P,
  ) -> P::NewScope {
    param
      .get_scope_data_mut()
      .new_escapable_handle_scope_data()
      .as_scope()
  }
}

impl<'s, 'e: 's, C> EscapableHandleScope<'s, 'e, C> {
  /// Pushes the value into the previous scope and returns a handle to it.
  /// Cannot be called twice.
  pub fn escape<T>(&mut self, value: Local<T>) -> Local<'e, T>
  where
    for<'l> Local<'l, T>: Into<Local<'l, Data>>,
  {
    let escape_slot = data::ScopeData::get_mut(self)
      .get_escape_slot_mut()
      .expect("internal error: EscapableHandleScope has no escape slot")
      .take()
      .expect("EscapableHandleScope::escape() called twice");
    escape_slot.escape(value)
  }
}

impl<'s, 'e: 's, C> Deref for EscapableHandleScope<'s, 'e, C> {
  type Target = HandleScope<'s, C>;

  fn deref(&self) -> &Self::Target {
    self.cast_ref()
  }
}

impl<'s, 'e: 's, C> DerefMut for EscapableHandleScope<'s, 'e, C> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.cast_mut()
  }
}

impl<'s, 'e: 's, C> Drop for EscapableHandleScope<'s, 'e, C> {
  fn drop(&mut self) {
    data::ScopeData::get_mut(self).notify_scope_dropped();
  }
}

/// A `CallbackScope` can be used to bootstrap a `HandleScope` and
/// `ContextScope` inside a callback function that gets called by V8.
/// Bootstrapping a scope inside a callback is the only valid use case of this
/// type; using it in other places leads to undefined behavior, which is also
/// the reason `CallbackScope::new()` is marked as being an unsafe function.
///
/// For some callback types, rusty_v8 internally creates a scope and passes it
/// as an argument to to embedder callback. Eventually we intend to wrap all
/// callbacks in this fashion, so the embedder would never needs to construct
/// a CallbackScope.
///
/// A CallbackScope can be created from the following inputs:
///   - `Local<Context>`
///   - `Local<Message>`
///   - `Local<Object>`
///   - `Local<Promise>`
///   - `Local<SharedArrayBuffer>`
///   - `&FunctionCallbackInfo`
///   - `&PropertyCallbackInfo`
///   - `&PromiseRejectMessage`
pub struct CallbackScope<'s> {
  data: NonNull<data::ScopeData>,
  _phantom: PhantomData<&'s mut HandleScope<'s>>,
}

unsafe impl<'s> Scope for CallbackScope<'s> {}

impl<'s> CallbackScope<'s> {
  pub unsafe fn new<P: param::NewCallbackScope<'s>>(param: P) -> Self {
    data::ScopeData::get_current_mut(param.get_isolate_mut())
      .new_callback_scope_data(param.maybe_get_current_context())
      .as_scope()
  }
}

impl<'s> Deref for CallbackScope<'s> {
  type Target = HandleScope<'s>;

  fn deref(&self) -> &Self::Target {
    self.cast_ref()
  }
}

impl<'s> DerefMut for CallbackScope<'s> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.cast_mut()
  }
}

impl<'s> Drop for CallbackScope<'s> {
  fn drop(&mut self) {
    data::ScopeData::get_mut(self).notify_scope_dropped();
  }
}

trait ScopeCast: Sized {
  fn cast_ref<S: Scope>(&self) -> &S;
  fn cast_mut<S: Scope>(&mut self) -> &mut S;
}

impl<T: Scope> ScopeCast for T {
  fn cast_ref<S: Scope>(&self) -> &S {
    assert_eq!(Layout::new::<Self>(), Layout::new::<S>());
    unsafe { &*(self as *const _ as *const S) }
  }

  fn cast_mut<S: Scope>(&mut self) -> &mut S {
    assert_eq!(Layout::new::<Self>(), Layout::new::<S>());
    unsafe { &mut *(self as *mut _ as *mut S) }
  }
}

/// Scopes are typically constructed as the child of another scope. The scope
/// that is returned from `«Child»Scope::new(parent: &mut «Parent»Scope)` does
/// not necessarily have type `«Child»Scope`, but rather its type is a merger of
/// both the the parent and child scope types.
///
/// For example: a `ContextScope` created inside `HandleScope<'a, ()>` does not
/// produce a `ContextScope`, but rather a `HandleScope<'a, Context>`, which
/// describes a scope that is both a `HandleScope` _and_ a `ContextScope`.  
///  
/// The Traits in the (private) `param` module define which types can be passed
/// as a parameter to the `«Some»Scope::new()` constructor, and what the
/// actual, merged scope type will be that `new()` returns for a specific
/// parameter type.
mod param {
  use super::*;

  pub trait NewContextScope<'s>: data::GetScopeData {
    type NewScope: Scope;
  }

  impl<'s, 'p: 's, P: Scope> NewContextScope<'s> for ContextScope<'p, P> {
    type NewScope = ContextScope<'s, P>;
  }

  impl<'s, 'p: 's, C> NewContextScope<'s> for HandleScope<'p, C> {
    type NewScope = ContextScope<'s, HandleScope<'p>>;
  }

  impl<'s, 'p: 's, 'e: 'p, C> NewContextScope<'s>
    for EscapableHandleScope<'p, 'e, C>
  {
    type NewScope = ContextScope<'s, EscapableHandleScope<'p, 'e>>;
  }

  impl<'s, 'p: 's> NewContextScope<'s> for CallbackScope<'p> {
    type NewScope = ContextScope<'s, HandleScope<'p>>;
  }

  pub trait NewHandleScope<'s>: data::GetScopeData {
    type NewScope: Scope;
  }

  impl<'s> NewHandleScope<'s> for Isolate {
    type NewScope = HandleScope<'s, ()>;
  }

  impl<'s> NewHandleScope<'s> for OwnedIsolate {
    type NewScope = HandleScope<'s, ()>;
  }

  impl<'s, 'p: 's, P: NewHandleScope<'s>> NewHandleScope<'s>
    for ContextScope<'p, P>
  {
    type NewScope = <P as NewHandleScope<'s>>::NewScope;
  }

  impl<'s, 'p: 's, C> NewHandleScope<'s> for HandleScope<'p, C> {
    type NewScope = HandleScope<'s, C>;
  }

  impl<'s, 'p: 's, 'e: 'p, C> NewHandleScope<'s>
    for EscapableHandleScope<'p, 'e, C>
  {
    type NewScope = EscapableHandleScope<'s, 'e, C>;
  }

  impl<'s, 'p: 's> NewHandleScope<'s> for CallbackScope<'p> {
    type NewScope = HandleScope<'s>;
  }

  pub trait NewEscapableHandleScope<'s, 'e: 's>: data::GetScopeData {
    type NewScope: Scope;
  }

  impl<'s, 'p: 's, 'e: 'p, P: NewEscapableHandleScope<'s, 'e>>
    NewEscapableHandleScope<'s, 'e> for ContextScope<'p, P>
  {
    type NewScope = <P as NewEscapableHandleScope<'s, 'e>>::NewScope;
  }

  impl<'s, 'p: 's, C> NewEscapableHandleScope<'s, 'p> for HandleScope<'p, C> {
    type NewScope = EscapableHandleScope<'s, 'p, C>;
  }

  impl<'s, 'p: 's, 'e: 'p, C> NewEscapableHandleScope<'s, 'p>
    for EscapableHandleScope<'p, 'e, C>
  {
    type NewScope = EscapableHandleScope<'s, 'p, C>;
  }

  impl<'s, 'p: 's> NewEscapableHandleScope<'s, 'p> for CallbackScope<'p> {
    type NewScope = EscapableHandleScope<'s, 'p>;
  }

  pub trait NewCallbackScope<'s>: Copy + Sized {
    fn maybe_get_current_context(self) -> Option<Local<'s, Context>> {
      None
    }
    fn get_isolate_mut(self) -> &'s mut Isolate;
  }

  impl<'s> NewCallbackScope<'s> for Local<'s, Context> {
    fn maybe_get_current_context(self) -> Option<Local<'s, Context>> {
      Some(self)
    }

    fn get_isolate_mut(self) -> &'s mut Isolate {
      unsafe { &mut *raw::v8__Context__GetIsolate(&*self) }
    }
  }

  impl<'s> NewCallbackScope<'s> for Local<'s, Message> {
    fn get_isolate_mut(self) -> &'s mut Isolate {
      unsafe { &mut *raw::v8__Message__GetIsolate(&*self) }
    }
  }

  impl<'s, T> NewCallbackScope<'s> for T
  where
    T: Copy + Into<Local<'s, Object>>,
  {
    fn get_isolate_mut(self) -> &'s mut Isolate {
      let object: Local<Object> = self.into();
      unsafe { &mut *raw::v8__Object__GetIsolate(&*object) }
    }
  }

  impl<'s> NewCallbackScope<'s> for &'s PromiseRejectMessage<'s> {
    fn get_isolate_mut(self) -> &'s mut Isolate {
      let object: Local<Object> = self.get_promise().into();
      unsafe { &mut *raw::v8__Object__GetIsolate(&*object) }
    }
  }

  impl<'s> NewCallbackScope<'s> for &'s FunctionCallbackInfo {
    fn get_isolate_mut(self) -> &'s mut Isolate {
      unsafe { &mut *raw::v8__FunctionCallbackInfo__GetIsolate(self) }
    }
  }

  impl<'s> NewCallbackScope<'s> for &'s PropertyCallbackInfo {
    fn get_isolate_mut(self) -> &'s mut Isolate {
      unsafe { &mut *raw::v8__PropertyCallbackInfo__GetIsolate(self) }
    }
  }
}

/// All publicly exported `«Some»Scope` types are essentially wrapping a pointer
/// to a heap-allocated struct `ScopeData`. This module contains the definition
/// for `ScopeData` and its inner types, as well as related helper traits.
pub(crate) mod data {
  use super::*;

  pub struct ScopeData {
    // The first four fields are always valid - even when the `Box<ScopeData>`
    // struct is free (does not contain data related to an actual scope).
    // The `previous` and `isolate` fields never change; the `next` field is
    // set to `None` initially when the struct is created, but it may later be
    // assigned a `Some(Box<ScopeData>)` value, after which this field never
    // changes again.
    isolate: NonNull<Isolate>,
    previous: Option<NonNull<ScopeData>>,
    next: Option<Box<ScopeData>>,
    // The 'status' field is also always valid (but does change).
    status: Cell<ScopeStatus>,
    // The following fields are only valid when this ScopeData object is in use
    // (eiter current or shadowed -- not free).
    context: Cell<Option<NonNull<Context>>>,
    escape_slot: Option<NonNull<Option<raw::EscapeSlot>>>,
    scope_type_specific_data: ScopeTypeSpecificData,
  }

  impl ScopeData {
    /// Returns a mutable reference to the data associated with topmost scope
    /// on the scope stack. This function does not automatically exit zombie
    /// scopes, so it might return a zombie ScopeData reference.
    pub(crate) fn get_current_mut(isolate: &mut Isolate) -> &mut Self {
      let self_mut = isolate
        .get_current_scope_data()
        .map(NonNull::as_ptr)
        .map(|p| unsafe { &mut *p })
        .unwrap();
      match self_mut.status.get() {
        ScopeStatus::Current { .. } => self_mut,
        _ => unreachable!(),
      }
    }

    /// Initializes the scope stack by creating a 'dummy' `ScopeData` at the
    /// very bottom. This makes it possible to store the freelist of reusable
    /// ScopeData objects even when no scope is entered.
    pub(crate) fn new_root(isolate: &mut Isolate) {
      let root = Box::leak(Self::boxed(isolate.into()));
      root.status = ScopeStatus::Current { zombie: false }.into();
      debug_assert!(isolate.get_current_scope_data().is_none());
      isolate.set_current_scope_data(Some(root.into()));
    }

    /// Activates and returns the 'root' `ScopeData` object that is created when
    /// the isolate is initialized. In order to do so, any zombie scopes that
    /// remain on the scope stack are cleaned up.
    ///
    /// # Panics
    ///
    /// This function panics if the root can't be activated because there are
    /// still other scopes on the stack and they're not zombies.
    pub(crate) fn get_root_mut(isolate: &mut Isolate) -> &mut Self {
      let mut current_scope_data = Self::get_current_mut(isolate);
      loop {
        current_scope_data = match current_scope_data {
          root if root.previous.is_none() => break root,
          data => data.try_exit_scope(),
        };
      }
    }

    /// Drops the scope stack and releases all `Box<ScopeData>` allocations.
    /// This function should be called only when an Isolate is being disposed.
    pub(crate) fn drop_root(isolate: &mut Isolate) {
      let root = Self::get_root_mut(isolate);
      unsafe { Box::from_raw(root) };
      isolate.set_current_scope_data(None);
    }

    pub(super) fn new_context_scope_data<'s>(
      &'s mut self,
      context: Local<'s, Context>,
    ) -> &'s mut Self {
      self.new_scope_data_with(move |data| {
        data.scope_type_specific_data.init_with(|| {
          ScopeTypeSpecificData::ContextScope {
            raw_context_scope: raw::ContextScope::new(context),
          }
        });
        data.context.set(Some(context.as_non_null()));
      })
    }

    pub(super) fn new_handle_scope_data(&mut self) -> &mut Self {
      self.new_scope_data_with(|data| {
        let isolate = data.isolate;
        data.scope_type_specific_data.init_with(|| {
          ScopeTypeSpecificData::HandleScope {
            raw_handle_scope: unsafe { raw::HandleScope::uninit() },
          }
        });
        match &mut data.scope_type_specific_data {
          ScopeTypeSpecificData::HandleScope { raw_handle_scope } => {
            unsafe { raw_handle_scope.init(isolate) };
          }
          _ => unreachable!(),
        }
      })
    }

    pub(super) fn new_escapable_handle_scope_data(&mut self) -> &mut Self {
      self.new_scope_data_with(|data| {
        // Note: the `raw_escape_slot` field must be initialized _before_ the
        // `raw_handle_scope` field, otherwise the escaped local handle ends up
        // inside the `EscapableHandleScope` that's being constructed here,
        // rather than escaping from it.
        let isolate = data.isolate;
        data.scope_type_specific_data.init_with(|| {
          ScopeTypeSpecificData::EscapableHandleScope {
            raw_handle_scope: unsafe { raw::HandleScope::uninit() },
            raw_escape_slot: Some(raw::EscapeSlot::new(isolate)),
          }
        });
        match &mut data.scope_type_specific_data {
          ScopeTypeSpecificData::EscapableHandleScope {
            raw_handle_scope,
            raw_escape_slot,
          } => {
            unsafe { raw_handle_scope.init(isolate) };
            data.escape_slot.replace(raw_escape_slot.into());
          }
          _ => unreachable!(),
        }
      })
    }

    pub(super) fn new_callback_scope_data<'s>(
      &'s mut self,
      maybe_current_context: Option<Local<'s, Context>>,
    ) -> &'s mut Self {
      self.new_scope_data_with(|data| {
        debug_assert!(data.scope_type_specific_data.is_none());
        data
          .context
          .set(maybe_current_context.map(|cx| cx.as_non_null()));
      })
    }

    fn new_scope_data_with(
      &mut self,
      init_fn: impl FnOnce(&mut Self),
    ) -> &mut Self {
      // Mark this scope (the parent of the newly created scope) as 'shadowed';
      self.status.set(match self.status.get() {
        ScopeStatus::Current { zombie } => ScopeStatus::Shadowed { zombie },
        _ => unreachable!(),
      });
      // Copy fields that that will be inherited by the new scope.
      let context = self.context.get().into();
      let escape_slot = self.escape_slot;
      // Initialize the `struct ScopeData` for the new scope.
      let new_scope_data = self.allocate_or_reuse_scope_data();
      new_scope_data.status = ScopeStatus::Current { zombie: true }.into();
      new_scope_data.context = context;
      new_scope_data.escape_slot = escape_slot;
      (init_fn)(new_scope_data);
      // Make the newly created scope the 'current' scope for this isolate.
      let new_scope_nn = unsafe { NonNull::new_unchecked(new_scope_data) };
      new_scope_data
        .get_isolate_mut()
        .set_current_scope_data(Some(new_scope_nn));
      new_scope_data
    }

    /// Either returns a free `Box<ScopeData` for reuse, or allocates a new one.
    fn allocate_or_reuse_scope_data(&mut self) -> &mut Self {
      let self_nn = NonNull::new(self);
      match &mut self.next {
        Some(next_box) => {
          // Recycle an unused `Box<ScopeData>` allocation.
          debug_assert_eq!(next_box.isolate, self.isolate);
          debug_assert_eq!(next_box.previous, self_nn);
          debug_assert_eq!(next_box.status.get(), ScopeStatus::Free);
          debug_assert!(next_box.scope_type_specific_data.is_none());
          next_box.as_mut()
        }
        next_field @ None => {
          // Allocate a new `Box<ScopeData>`.
          let mut next_box = Self::boxed(self.isolate);
          next_box.previous = self_nn;
          next_field.replace(next_box);
          next_field.as_mut().unwrap()
        }
      }
    }

    pub(super) fn as_scope<S: Scope>(&mut self) -> S {
      self.status.set(match self.status.get() {
        ScopeStatus::Current { zombie: true } => {
          ScopeStatus::Current { zombie: false }
        }
        _ => unreachable!(),
      });
      assert_eq!(size_of::<&mut Self>(), size_of::<S>());
      let self_nn = NonNull::from(self);
      unsafe { ptr::read(&self_nn as *const _ as *const S) }
    }

    pub(super) fn get<S: Scope>(scope: &S) -> &Self {
      let self_mut = unsafe {
        (*(scope as *const S as *mut S as *mut NonNull<Self>)).as_mut()
      };
      self_mut.try_activate_scope();
      self_mut
    }

    pub(super) fn get_mut<S: Scope>(scope: &mut S) -> &mut Self {
      let self_mut =
        unsafe { (*(scope as *mut S as *mut NonNull<Self>)).as_mut() };
      self_mut.try_activate_scope();
      self_mut
    }

    #[inline(always)]
    fn try_activate_scope(mut self: &mut Self) -> &mut Self {
      self = match self.status.get() {
        ScopeStatus::Current { zombie: false } => self,
        ScopeStatus::Shadowed { zombie: false } => {
          self.next.as_mut().unwrap().try_exit_scope()
        }
        _ => unreachable!(),
      };
      debug_assert_eq!(
        self.get_isolate().get_current_scope_data(),
        NonNull::new(self as *mut _)
      );
      self
    }

    fn try_exit_scope(mut self: &mut Self) -> &mut Self {
      loop {
        self = match self.status.get() {
          ScopeStatus::Shadowed { .. } => {
            self.next.as_mut().unwrap().try_exit_scope()
          }
          ScopeStatus::Current { zombie: true } => break self.exit_scope(),
          ScopeStatus::Current { zombie: false } => {
            panic!("active scope can't be dropped")
          }
          _ => unreachable!(),
        }
      }
    }

    fn exit_scope(&mut self) -> &mut Self {
      // Clear out the scope type specific data field. None of the other fields
      // have a destructor, and there's no need to do any cleanup on them.
      take(&mut self.scope_type_specific_data);
      // Change the ScopeData's status field from 'Current' to 'Free', which
      // means that it is not associated with a scope and can be reused.
      self.status.set(ScopeStatus::Free);

      // Point the Isolate's current scope data slot at our parent scope.
      let previous_nn = self.previous.unwrap();
      self
        .get_isolate_mut()
        .set_current_scope_data(Some(previous_nn));
      // Update the parent scope's status field to reflect that it is now
      // 'Current' again an no longer 'Shadowed'.
      let previous_mut = unsafe { &mut *previous_nn.as_ptr() };
      previous_mut.status.set(match previous_mut.status.get() {
        ScopeStatus::Shadowed { zombie } => ScopeStatus::Current { zombie },
        _ => unreachable!(),
      });

      previous_mut
    }

    /// This function is called when any of the public scope objects (e.g
    /// `HandleScope`, `ContextScope`, etc.) are dropped.
    ///
    /// The Rust borrow checker allows values of type `HandleScope<'a>` and
    /// `EscapableHandleScope<'a, 'e>` to be dropped before their maximum
    /// lifetime ('a) is up. This creates a potential problem because any local
    /// handles that are created while these scopes are active are bound to
    /// that 'a lifetime. This means that we run the risk of creating local
    /// handles that outlive their creation scope.
    ///
    /// Therefore, we don't immediately exit the current scope at the very
    /// moment the user drops their Escapable/HandleScope handle.
    /// Instead, the current scope is marked as being a 'zombie': the scope
    /// itself is gone, but its data still on the stack. The zombie's data will
    /// be dropped when the user touches the parent scope; when that happens, it
    /// is certain that there are no accessible `Local<'a, T>` handles left,
    /// because the 'a lifetime ends there.
    ///
    /// Scope types that do no store local handles are exited immediately.
    pub(super) fn notify_scope_dropped(&mut self) {
      match &self.scope_type_specific_data {
        ScopeTypeSpecificData::HandleScope { .. }
        | ScopeTypeSpecificData::EscapableHandleScope { .. } => {
          // Defer scope exit until the parent scope is touched.
          self.status.set(match self.status.get() {
            ScopeStatus::Current { zombie: false } => {
              ScopeStatus::Current { zombie: true }
            }
            _ => unreachable!(),
          })
        }
        _ => {
          // Regular, immediate exit.
          self.exit_scope();
        }
      }
    }

    pub(crate) fn get_isolate(&self) -> &Isolate {
      unsafe { self.isolate.as_ref() }
    }

    pub(crate) fn get_isolate_mut(&mut self) -> &mut Isolate {
      unsafe { self.isolate.as_mut() }
    }

    pub(crate) fn get_isolate_ptr(&self) -> *mut Isolate {
      self.isolate.as_ptr()
    }

    pub(crate) fn get_current_context(&self) -> *const Context {
      // To avoid creating a new Local every time `get_current_context() is
      // called, the current context is usually cached in the `context` field.
      // If the `context` field contains `None`, this might mean that this cache
      // field never got populated, so we'll do that here when necessary.
      let get_current_context_from_isolate = || unsafe {
        raw::v8__Isolate__GetCurrentContext(self.get_isolate_ptr())
      };
      match self.context.get().map(|nn| nn.as_ptr() as *const _) {
        Some(context) => {
          debug_assert!(unsafe {
            raw::v8__Context__EQ(context, get_current_context_from_isolate())
          });
          context
        }
        None => {
          let context = get_current_context_from_isolate();
          self.context.set(NonNull::new(context as *mut _));
          context
        }
      }
    }

    pub(super) fn get_escape_slot_mut(
      &mut self,
    ) -> Option<&mut Option<raw::EscapeSlot>> {
      self
        .escape_slot
        .as_mut()
        .map(|escape_slot_nn| unsafe { escape_slot_nn.as_mut() })
    }

    /// Returns a new `Box<ScopeData>` with the `isolate` field set as specified
    /// by the first parameter, and the other fields initialized to their
    /// default values. This function exists solely because it turns out that
    /// Rust doesn't optimize `Box::new(Self{ .. })` very well (a.k.a. not at
    /// all) in this case, which is why `std::alloc::alloc()` is used directly.
    fn boxed(isolate: NonNull<Isolate>) -> Box<Self> {
      unsafe {
        #[allow(clippy::cast_ptr_alignment)]
        let self_ptr = alloc(Layout::new::<Self>()) as *mut Self;
        ptr::write(
          self_ptr,
          Self {
            isolate,
            previous: Default::default(),
            next: Default::default(),
            status: Default::default(),
            context: Default::default(),
            escape_slot: Default::default(),
            scope_type_specific_data: Default::default(),
          },
        );
        Box::from_raw(self_ptr)
      }
    }
  }

  #[derive(Debug, Clone, Copy, Eq, PartialEq)]
  enum ScopeStatus {
    Free,
    Current { zombie: bool },
    Shadowed { zombie: bool },
  }

  impl Default for ScopeStatus {
    fn default() -> Self {
      Self::Free
    }
  }

  #[allow(clippy::enum_variant_names)]
  enum ScopeTypeSpecificData {
    None,
    ContextScope {
      raw_context_scope: raw::ContextScope,
    },
    HandleScope {
      raw_handle_scope: raw::HandleScope,
    },
    EscapableHandleScope {
      raw_handle_scope: raw::HandleScope,
      raw_escape_slot: Option<raw::EscapeSlot>,
    },
  }

  impl Default for ScopeTypeSpecificData {
    fn default() -> Self {
      Self::None
    }
  }

  impl ScopeTypeSpecificData {
    pub fn is_none(&self) -> bool {
      match self {
        Self::None => true,
        _ => false,
      }
    }

    /// Replaces a `ScopeTypeSpecificData::None` value with the value returned
    /// from the specified closure. This function exists because initializing
    /// scopes is performance critical, and `ptr::write()` produces more
    /// efficient code than using a regular assign statement, which will try to
    /// drop the old value and move the new value into place, even after
    /// asserting `self.is_none()`.
    pub fn init_with(&mut self, init_fn: impl FnOnce() -> Self) {
      assert!(self.is_none());
      unsafe { ptr::write(self, (init_fn)()) }
    }
  }

  pub trait GetScopeData {
    fn get_scope_data_mut(&mut self) -> &mut data::ScopeData;
  }

  impl<T: Scope> GetScopeData for T {
    fn get_scope_data_mut(&mut self) -> &mut data::ScopeData {
      data::ScopeData::get_mut(self)
    }
  }

  impl GetScopeData for Isolate {
    fn get_scope_data_mut(&mut self) -> &mut data::ScopeData {
      data::ScopeData::get_root_mut(self)
    }
  }

  impl GetScopeData for OwnedIsolate {
    fn get_scope_data_mut(&mut self) -> &mut data::ScopeData {
      data::ScopeData::get_root_mut(self)
    }
  }
}

/// The `raw` module contains prototypes for all the `extern C` functions that
/// are used in this file, as well as definitions for the types they operate on.
mod raw {
  use super::*;

  #[derive(Clone, Copy)]
  #[repr(transparent)]
  pub(super) struct Address(NonZeroUsize);

  pub(super) struct ContextScope {
    entered_context: *const Context,
  }

  impl ContextScope {
    pub fn new(context: Local<Context>) -> Self {
      unsafe { v8__Context__Enter(&*context) };
      Self {
        entered_context: &*context,
      }
    }
  }

  impl Drop for ContextScope {
    fn drop(&mut self) {
      debug_assert!(!self.entered_context.is_null());
      unsafe { v8__Context__Exit(self.entered_context) };
    }
  }

  #[repr(C)]
  pub(super) struct HandleScope([usize; 3]);

  impl HandleScope {
    /// This function is marked unsafe because the caller must ensure that the
    /// returned value isn't dropped before `init()` has been called.
    pub unsafe fn uninit() -> Self {
      // This is safe because there is no combination of bits that would produce
      // an invalid `[usize; 3]`.
      #[allow(clippy::uninit_assumed_init)]
      Self(MaybeUninit::uninit().assume_init())
    }

    /// This function is marked unsafe because `init()` must be called exactly
    /// once, no more and no less, after constructing a HandleScope with
    /// `HandleScope::uninit()`.
    pub unsafe fn init(&mut self, isolate: NonNull<Isolate>) {
      let buf = NonNull::from(self).cast();
      v8__HandleScope__CONSTRUCT(buf.as_ptr(), isolate.as_ptr());
    }
  }

  impl Drop for HandleScope {
    fn drop(&mut self) {
      unsafe { v8__HandleScope__DESTRUCT(self) };
    }
  }

  #[repr(transparent)]
  pub(super) struct EscapeSlot(NonNull<raw::Address>);

  impl EscapeSlot {
    pub fn new(isolate: NonNull<Isolate>) -> Self {
      unsafe {
        let undefined = raw::v8__Undefined(isolate.as_ptr()) as *const _;
        let local = raw::v8__Local__New(isolate.as_ptr(), undefined);
        let slot_address_ptr = local as *const Address as *mut _;
        let slot_address_nn = NonNull::new_unchecked(slot_address_ptr);
        Self(slot_address_nn)
      }
    }

    pub fn escape<'e, T>(self, value: Local<'_, T>) -> Local<'e, T>
    where
      for<'l> Local<'l, T>: Into<Local<'l, Data>>,
    {
      assert_eq!(Layout::new::<Self>(), Layout::new::<Local<T>>());
      unsafe {
        let undefined = Local::<Value>::from_non_null(self.0.cast());
        debug_assert!(undefined.is_undefined());
        let value_address = *(&*value as *const T as *const Address);
        ptr::write(self.0.as_ptr(), value_address);
        Local::from_non_null(self.0.cast())
      }
    }
  }

  extern "C" {
    pub(super) fn v8__Isolate__GetCurrentContext(
      isolate: *mut Isolate,
    ) -> *const Context;
    pub(super) fn v8__Isolate__GetEnteredOrMicrotaskContext(
      isolate: *mut Isolate,
    ) -> *const Context;
    pub(super) fn v8__Isolate__ThrowException(
      isolate: *mut Isolate,
      exception: *const Value,
    ) -> *const Value;

    pub(super) fn v8__Context__EQ(
      this: *const Context,
      other: *const Context,
    ) -> bool;
    pub(super) fn v8__Context__Enter(this: *const Context);
    pub(super) fn v8__Context__Exit(this: *const Context);
    pub(super) fn v8__Context__GetIsolate(this: *const Context)
      -> *mut Isolate;

    pub(super) fn v8__HandleScope__CONSTRUCT(
      buf: *mut MaybeUninit<HandleScope>,
      isolate: *mut Isolate,
    );
    pub(super) fn v8__HandleScope__DESTRUCT(this: *mut HandleScope);

    pub(super) fn v8__Local__New(
      isolate: *mut Isolate,
      other: *const Data,
    ) -> *const Data;
    pub(super) fn v8__Undefined(isolate: *mut Isolate) -> *const Primitive;

    pub(super) fn v8__Message__GetIsolate(this: *const Message)
      -> *mut Isolate;
    pub(super) fn v8__Object__GetIsolate(this: *const Object) -> *mut Isolate;
    pub(super) fn v8__FunctionCallbackInfo__GetIsolate(
      this: *const FunctionCallbackInfo,
    ) -> *mut Isolate;
    pub(super) fn v8__PropertyCallbackInfo__GetIsolate(
      this: *const PropertyCallbackInfo,
    ) -> *mut Isolate;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::new_default_platform;
  use crate::V8;
  use std::any::type_name;
  use std::borrow::Borrow;
  use std::sync::Once;

  /// `AssertTypeOf` facilitates comparing types. This is done partially at
  /// compile-type (with the `Borrow` constraits) and partially at runtime
  /// (by comparing type names). The main difference with assigning a value
  /// to a variable with an explicitly stated type is that the latter allows
  /// coercions and dereferencing to change the type, whereas `AssertTypeOf`
  /// does not allow that to happen.  
  struct AssertTypeOf<'a, T>(pub &'a T);
  impl<'a, T> AssertTypeOf<'a, T> {
    pub fn is<A>(self)
    where
      A: Borrow<T>,
      T: Borrow<A>,
    {
      assert_eq!(type_name::<A>(), type_name::<T>());
    }
  }

  fn initialize_v8() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
      V8::initialize_platform(new_default_platform().unwrap());
      V8::initialize();
    });
  }

  #[test]
  fn deref_types() {
    initialize_v8();
    let isolate = &mut Isolate::new(Default::default());
    let hs = &mut HandleScope::new(isolate);
    AssertTypeOf(hs).is::<HandleScope<()>>();
    let context = Context::new(hs);
    {
      let ehs = &mut EscapableHandleScope::new(hs);
      AssertTypeOf(ehs).is::<EscapableHandleScope<()>>();
      let d = ehs.deref_mut();
      AssertTypeOf(d).is::<HandleScope<()>>();
      let d = d.deref_mut();
      AssertTypeOf(d).is::<Isolate>();
    }
    {
      let cs1 = &mut ContextScope::new(hs, context);
      AssertTypeOf(cs1).is::<ContextScope<HandleScope>>();
      let d = cs1.deref_mut();
      AssertTypeOf(d).is::<HandleScope>();
      let ehs = &mut EscapableHandleScope::new(cs1);
      AssertTypeOf(ehs).is::<EscapableHandleScope>();
      let cs2 = &mut ContextScope::new(ehs, context);
      AssertTypeOf(cs2).is::<ContextScope<EscapableHandleScope>>();
      let d = cs2.deref_mut();
      AssertTypeOf(d).is::<EscapableHandleScope>();
      let d = d.deref_mut();
      AssertTypeOf(d).is::<HandleScope>();
      let d = d.deref_mut();
      AssertTypeOf(d).is::<ContextScope>();
      let d = d.deref_mut();
      AssertTypeOf(d).is::<HandleScope<()>>();
      let d = d.deref_mut();
      AssertTypeOf(d).is::<Isolate>();
    }
    {
      // CallbackScope is not used as intended here. Push a ContextScope onto
      // the stack so that its assumptions aren't violated.
      let _ = ContextScope::new(hs, context);
      let cbs = &mut unsafe { CallbackScope::new(context) };
      AssertTypeOf(cbs).is::<CallbackScope>();
      let d = cbs.deref_mut();
      AssertTypeOf(d).is::<HandleScope>();
      let d = d.deref_mut();
      AssertTypeOf(d).is::<ContextScope>();
      let d = d.deref_mut();
      AssertTypeOf(d).is::<HandleScope<()>>();
      let d = d.deref_mut();
      AssertTypeOf(d).is::<Isolate>();
    }
  }

  #[test]
  fn new_scope_types() {
    initialize_v8();
    let isolate = &mut Isolate::new(Default::default());
    let l0_hs = &mut HandleScope::new(isolate);
    AssertTypeOf(l0_hs).is::<HandleScope<()>>();
    let context = Context::new(l0_hs);
    {
      let l1_cs = &mut ContextScope::new(l0_hs, context);
      AssertTypeOf(l1_cs).is::<ContextScope<HandleScope>>();
      AssertTypeOf(&ContextScope::new(l1_cs, context))
        .is::<ContextScope<HandleScope>>();
      {
        let l2_hs = &mut HandleScope::new(l1_cs);
        AssertTypeOf(l2_hs).is::<HandleScope>();
        AssertTypeOf(&EscapableHandleScope::new(l2_hs))
          .is::<EscapableHandleScope>();
      }
      {
        let l2_ehs = &mut EscapableHandleScope::new(l1_cs);
        AssertTypeOf(l2_ehs).is::<EscapableHandleScope>();
        AssertTypeOf(&ContextScope::new(l2_ehs, context))
          .is::<ContextScope<EscapableHandleScope>>();
        AssertTypeOf(&HandleScope::new(l2_ehs)).is::<EscapableHandleScope>();
        AssertTypeOf(&EscapableHandleScope::new(l2_ehs))
          .is::<EscapableHandleScope>();
      }
    }
    {
      let l1_hs = &mut HandleScope::new(l0_hs);
      AssertTypeOf(l1_hs).is::<HandleScope<()>>();
      {
        let l2_cs = &mut ContextScope::new(l1_hs, context);
        AssertTypeOf(l2_cs).is::<ContextScope<HandleScope>>();
        AssertTypeOf(&EscapableHandleScope::new(l2_cs))
          .is::<EscapableHandleScope>();
      }
      {
        let l2_ehs = &mut EscapableHandleScope::new(l1_hs);
        AssertTypeOf(l2_ehs).is::<EscapableHandleScope<()>>();
        AssertTypeOf(&ContextScope::new(l2_ehs, context))
          .is::<ContextScope<EscapableHandleScope>>();
      }
    }
    {
      let l1_ehs = &mut EscapableHandleScope::new(l0_hs);
      AssertTypeOf(l1_ehs).is::<EscapableHandleScope<()>>();
      {
        let l2_cs = &mut ContextScope::new(l1_ehs, context);
        AssertTypeOf(l2_cs).is::<ContextScope<EscapableHandleScope>>();
        AssertTypeOf(&ContextScope::new(l2_cs, context))
          .is::<ContextScope<EscapableHandleScope>>();
        AssertTypeOf(&HandleScope::new(l2_cs)).is::<EscapableHandleScope>();
        AssertTypeOf(&EscapableHandleScope::new(l2_cs))
          .is::<EscapableHandleScope>();
      }
      {
        let l2_hs = &mut HandleScope::new(l1_ehs);
        AssertTypeOf(l2_hs).is::<EscapableHandleScope<()>>();
        AssertTypeOf(&ContextScope::new(l2_hs, context))
          .is::<ContextScope<EscapableHandleScope>>();
      }
      AssertTypeOf(&EscapableHandleScope::new(l1_ehs))
        .is::<EscapableHandleScope<()>>();
    }
    {
      // CallbackScope is not used as intended here. Push a ContextScope onto
      // the stack so that its assumptions aren't violated.
      let _ = ContextScope::new(l0_hs, context);
      let l0_cbs = &mut unsafe { CallbackScope::new(context) };
      AssertTypeOf(l0_cbs).is::<CallbackScope>();
      AssertTypeOf(&ContextScope::new(l0_cbs, context))
        .is::<ContextScope<HandleScope>>();
      AssertTypeOf(&HandleScope::new(l0_cbs)).is::<HandleScope>();
      AssertTypeOf(&EscapableHandleScope::new(l0_cbs))
        .is::<EscapableHandleScope>();
    }
  }
}
