use std::any::type_name;
use std::cell::Cell;
use std::marker::PhantomData;
use std::mem::replace;
use std::mem::size_of;
use std::mem::MaybeUninit;
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr;
use std::ptr::null;
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
use crate::TryCatch;
use crate::Value;

#[doc(inline)]
pub use api::{CallbackScope, ContextScope, EscapableHandleScope, HandleScope};
pub(crate) use data::ScopeData;

mod api {
  use super::*;

  /// Stack-allocated class which sets the execution context for all operations
  /// executed within a local scope.
  #[derive(Debug)]
  pub struct ContextScope<'s, P = ()> {
    data: NonNull<data::ScopeData>,
    _phantom: PhantomData<&'s mut P>,
  }

  impl<'s, P> ContextScope<'s, P>
  where
    P: NewContextScopeParam<'s>,
  {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(param: &'s mut P, context: Local<Context>) -> P::NewScope {
      let isolate = param.get_isolate_mut();

      let isolate_ptr_1 = isolate as *mut Isolate;
      let isolate_ptr_2 = unsafe { raw::v8__Context__GetIsolate(&*context) };
      if isolate_ptr_1 != isolate_ptr_2 {
        panic!(
          "{} and Context do not belong to the same Isolate",
          type_name::<P>()
        )
      }

      let data = data::ScopeData::new_context_scope(isolate, context);
      data.as_scope()
    }
  }

  impl<'s, P> ContextScope<'s, P> {
    /// Returns the context of the currently running JavaScript, or the context
    /// on the top of the stack if no JavaScript is running.
    pub fn get_current_context(&self) -> Local<'s, Context> {
      // To avoid creating a new Local every time GetCurrentContext is called,
      // the current context is cached in `struct ScopeData`.
      let get_current_context_from_isolate =
        |data: &data::ScopeData| -> Local<Context> {
          let isolate_ptr = data.get_isolate_ptr();
          let context_ptr =
            unsafe { raw::v8__Isolate__GetCurrentContext(isolate_ptr) };
          unsafe { Local::from_raw(context_ptr) }.unwrap()
        };
      let data = data::ScopeData::get(self);
      match data.context.get() {
        Some(context_nn) => {
          let context = unsafe { Local::from_non_null(context_nn) };
          debug_assert!(context == get_current_context_from_isolate(data));
          context
        }
        None => {
          let context = get_current_context_from_isolate(data);
          data.context.set(Some(context.as_non_null()));
          context
        }
      }
    }

    /// Returns either the last context entered through V8's C++ API, or the
    /// context of the currently running microtask while processing microtasks.
    /// If a context is entered while executing a microtask, that context is
    /// returned.
    pub fn get_entered_or_microtask_context(&mut self) -> Local<'s, Context> {
      let data = data::ScopeData::get(self);
      let isolate_ptr = data.get_isolate_ptr();
      let context_ptr =
        unsafe { raw::v8__Isolate__GetEnteredOrMicrotaskContext(isolate_ptr) };
      unsafe { Local::from_raw(context_ptr) }.unwrap()
    }
  }

  unsafe impl<'s, P> Scope for ContextScope<'s, P> {}

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

  pub unsafe trait NewContextScopeParam<'s> {
    type NewScope: Scope;
    fn get_isolate_mut(&mut self) -> &mut Isolate;
  }

  unsafe impl<'s, 'p: 's, P: Scope> NewContextScopeParam<'s>
    for ContextScope<'p, P>
  {
    type NewScope = ContextScope<'s, P>;
    fn get_isolate_mut(&mut self) -> &mut Isolate {
      data::ScopeData::get_mut(self).get_isolate_mut()
    }
  }

  unsafe impl<'s, 'p: 's, C> NewContextScopeParam<'s> for HandleScope<'p, C> {
    type NewScope = ContextScope<'s, HandleScope<'p>>;
    fn get_isolate_mut(&mut self) -> &mut Isolate {
      data::ScopeData::get_mut(self).get_isolate_mut()
    }
  }

  unsafe impl<'s, 'p: 's, 'e: 'p, C> NewContextScopeParam<'s>
    for EscapableHandleScope<'p, 'e, C>
  {
    type NewScope = EscapableHandleScope<'s, 'e>;
    fn get_isolate_mut(&mut self) -> &mut Isolate {
      data::ScopeData::get_mut(self).get_isolate_mut()
    }
  }

  unsafe impl<'s, 'p: 's> NewContextScopeParam<'s> for CallbackScope<'p> {
    type NewScope = ContextScope<'s, HandleScope<'p>>;
    fn get_isolate_mut(&mut self) -> &mut Isolate {
      data::ScopeData::get_mut(self).get_isolate_mut()
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
  #[derive(Debug)]
  pub struct HandleScope<'s, C = Context> {
    data: NonNull<data::ScopeData>,
    _phantom: PhantomData<&'s mut C>,
  }

  impl<'s> HandleScope<'s> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<P>(param: &'s mut P) -> P::NewScope
    where
      P: NewHandleScopeParam<'s>,
    {
      let isolate = param.get_isolate_mut();
      let data = data::ScopeData::new_handle_scope(isolate);
      data.as_scope()
    }
  }

  impl<'s, C> HandleScope<'s, C> {
    pub fn get_isolate(&self) -> &Isolate {
      data::ScopeData::get(self).get_isolate()
    }

    pub fn get_isolate_mut(&mut self) -> &mut Isolate {
      data::ScopeData::get_mut(self).get_isolate_mut()
    }

    pub(crate) fn get_isolate_ptr(&self) -> *mut Isolate {
      data::ScopeData::get(self).get_isolate_ptr()
    }

    /// Schedules an exception to be thrown when returning to JavaScript. When an
    /// exception has been scheduled it is illegal to invoke any JavaScript
    /// operation; the caller must return immediately and only after the exception
    /// has been handled does it become legal to invoke JavaScript operations.
    ///
    /// This function always returns the `undefined` value.
    pub fn throw_exception(
      &mut self,
      exception: Local<Value>,
    ) -> Local<'s, Value> {
      unsafe {
        self.cast_local(|scope| {
          raw::v8__Isolate__ThrowException(scope.get_isolate_ptr(), &*exception)
        })
      }
      .unwrap()
    }

    pub(crate) unsafe fn cast_local<F, T>(
      &mut self,
      f: F,
    ) -> Option<Local<'s, T>>
    where
      F: FnOnce(&mut Self) -> *const T,
    {
      // `ScopeData::get_mut()` is called here for its side effects: it checks
      // that `self` is actually the active scope, and if necessary it will
      // drop (escapable) handle scopes of which the drop call has been
      // deferred.
      data::ScopeData::get_mut(self);
      Local::from_raw(f(self))
    }
  }

  impl<'s, C> AsRef<Isolate> for HandleScope<'s, C> {
    fn as_ref(&self) -> &Isolate {
      self.get_isolate()
    }
  }

  impl<'s, C> AsMut<Isolate> for HandleScope<'s, C> {
    fn as_mut(&mut self) -> &mut Isolate {
      self.get_isolate_mut()
    }
  }

  unsafe impl<'s, C> Scope for HandleScope<'s, C> {}

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

  pub unsafe trait NewHandleScopeParam<'s> {
    type NewScope: Scope;
    fn get_isolate_mut(&mut self) -> &mut Isolate;
  }

  unsafe impl<'s> NewHandleScopeParam<'s> for OwnedIsolate {
    type NewScope = HandleScope<'s, ()>;
    fn get_isolate_mut(&mut self) -> &mut Isolate {
      &mut **self
    }
  }

  unsafe impl<'s, 'p: 's, P> NewHandleScopeParam<'s> for ContextScope<'p, P>
  where
    P: Scope + NewHandleScopeParam<'s>,
  {
    type NewScope = <P as NewHandleScopeParam<'s>>::NewScope;
    fn get_isolate_mut(&mut self) -> &mut Isolate {
      data::ScopeData::get_mut(self).get_isolate_mut()
    }
  }

  unsafe impl<'s, 'p: 's, C> NewHandleScopeParam<'s> for HandleScope<'p, C> {
    type NewScope = HandleScope<'s, C>;
    fn get_isolate_mut(&mut self) -> &mut Isolate {
      data::ScopeData::get_mut(self).get_isolate_mut()
    }
  }

  unsafe impl<'s, 'p: 's, 'e: 'p, C> NewHandleScopeParam<'s>
    for EscapableHandleScope<'p, 'e, C>
  {
    type NewScope = EscapableHandleScope<'s, 'e, C>;
    fn get_isolate_mut(&mut self) -> &mut Isolate {
      data::ScopeData::get_mut(self).get_isolate_mut()
    }
  }

  unsafe impl<'s, 'p: 's> NewHandleScopeParam<'s> for CallbackScope<'p> {
    type NewScope = HandleScope<'s>;
    fn get_isolate_mut(&mut self) -> &mut Isolate {
      data::ScopeData::get_mut(self).get_isolate_mut()
    }
  }

  /// A HandleScope which first allocates a handle in the current scope
  /// which will be later filled with the escape value.
  #[derive(Debug)]
  pub struct EscapableHandleScope<'s, 'e: 's, C = Context> {
    data: NonNull<data::ScopeData>,
    _phantom:
      PhantomData<(&'s mut raw::HandleScope, &'e raw::EscapeSlot, &'s C)>,
  }

  impl<'s, 'e: 's> EscapableHandleScope<'s, 'e> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<P>(param: &'s mut P) -> P::NewScope
    where
      P: NewEscapableHandleScopeParam<'s, 'e>,
    {
      let isolate = param.get_isolate_mut();
      let data = data::ScopeData::new_escapable_handle_scope(isolate);
      data.as_scope()
    }
  }

  impl<'s, 'e: 's, C> EscapableHandleScope<'s, 'e, C> {
    /// Pushes the value into the previous scope and returns a handle to it.
    /// Cannot be called twice.
    pub fn escape<T>(&mut self, value: Local<T>) -> Local<'e, T> {
      let mut escape_slot_nn = data::ScopeData::get_mut(self)
        .escape_slot
        .expect("internal error: EscapableHandleScope has no escape slot");
      let escape_slot = unsafe { escape_slot_nn.as_mut() };
      let value_raw: *const T = &*value;
      let escaped_value_raw = escape_slot.escape(value_raw);
      unsafe { Local::from_raw(escaped_value_raw) }.unwrap()
    }
  }

  unsafe impl<'s, 'e: 's, C> Scope for EscapableHandleScope<'s, 'e, C> {}

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

  pub unsafe trait NewEscapableHandleScopeParam<'s, 'e: 's> {
    type NewScope: Scope;
    fn get_isolate_mut(&mut self) -> &mut Isolate;
  }

  unsafe impl<'s, 'p: 's, 'e: 'p, P> NewEscapableHandleScopeParam<'s, 'e>
    for ContextScope<'p, P>
  where
    P: NewEscapableHandleScopeParam<'s, 'e>,
  {
    type NewScope = <P as NewEscapableHandleScopeParam<'s, 'e>>::NewScope;
    fn get_isolate_mut(&mut self) -> &mut Isolate {
      data::ScopeData::get_mut(self).get_isolate_mut()
    }
  }

  unsafe impl<'s, 'p: 's, C> NewEscapableHandleScopeParam<'s, 'p>
    for HandleScope<'p, C>
  {
    type NewScope = EscapableHandleScope<'s, 'p, C>;
    fn get_isolate_mut(&mut self) -> &mut Isolate {
      data::ScopeData::get_mut(self).get_isolate_mut()
    }
  }

  unsafe impl<'s, 'p: 's, 'e: 'p, C> NewEscapableHandleScopeParam<'s, 'p>
    for EscapableHandleScope<'p, 'e, C>
  {
    type NewScope = EscapableHandleScope<'s, 'p, C>;
    fn get_isolate_mut(&mut self) -> &mut Isolate {
      data::ScopeData::get_mut(self).get_isolate_mut()
    }
  }

  unsafe impl<'s, 'p: 's> NewEscapableHandleScopeParam<'s, 'p>
    for CallbackScope<'p>
  {
    type NewScope = EscapableHandleScope<'s, 'p>;
    fn get_isolate_mut(&mut self) -> &mut Isolate {
      data::ScopeData::get_mut(self).get_isolate_mut()
    }
  }

  /// A CallbackScope can be used to bootstrap a HandleScope + ContextScope in a
  /// callback function that gets called by V8. Note that, per the V8 API
  /// contract, there are a number of callback types in which you should not
  /// create scopes, execute JavaScript, nor create JavaScript values.
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
  #[derive(Debug)]
  pub struct CallbackScope<'s> {
    data: NonNull<data::ScopeData>,
    _phantom: PhantomData<&'s mut HandleScope<'s>>,
  }

  impl<'s> CallbackScope<'s> {
    pub fn new<P>(param: P) -> Self
    where
      P: NewCallbackScopeParam<'s>,
    {
      let isolate = param.get_isolate_mut();
      let maybe_current_context = param.get_current_context_maybe();
      let data =
        data::ScopeData::new_callback_scope(isolate, maybe_current_context);
      data.as_scope()
    }
  }

  unsafe impl<'s> Scope for CallbackScope<'s> {}

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

  pub unsafe trait NewCallbackScopeParam<'s>: Copy + Sized {
    fn get_current_context_maybe(self) -> Option<Local<'s, Context>> {
      None
    }
    fn get_isolate_mut(self) -> &'s mut Isolate;
  }

  unsafe impl<'s> NewCallbackScopeParam<'s> for Local<'s, Context> {
    fn get_current_context_maybe(self) -> Option<Local<'s, Context>> {
      Some(self)
    }
    fn get_isolate_mut(self) -> &'s mut Isolate {
      unsafe { &mut *raw::v8__Context__GetIsolate(&*self) }
    }
  }

  unsafe impl<'s> NewCallbackScopeParam<'s> for Local<'s, Message> {
    fn get_isolate_mut(self) -> &'s mut Isolate {
      unsafe { &mut *raw::v8__Message__GetIsolate(&*self) }
    }
  }

  unsafe impl<'s, T> NewCallbackScopeParam<'s> for T
  where
    T: Copy + Into<Local<'s, Object>>,
  {
    fn get_isolate_mut(self) -> &'s mut Isolate {
      let object: Local<Object> = self.into();
      unsafe { &mut *raw::v8__Object__GetIsolate(&*object) }
    }
  }

  unsafe impl<'s> NewCallbackScopeParam<'s> for &'s PromiseRejectMessage<'s> {
    fn get_isolate_mut(self) -> &'s mut Isolate {
      let object: Local<Object> = self.get_promise().into();
      unsafe { &mut *raw::v8__Object__GetIsolate(&*object) }
    }
  }

  unsafe impl<'s> NewCallbackScopeParam<'s> for &'s FunctionCallbackInfo {
    fn get_isolate_mut(self) -> &'s mut Isolate {
      unsafe { &mut *raw::v8__FunctionCallbackInfo__GetIsolate(self) }
    }
  }

  unsafe impl<'s> NewCallbackScopeParam<'s> for &'s PropertyCallbackInfo {
    fn get_isolate_mut(self) -> &'s mut Isolate {
      unsafe { &mut *raw::v8__PropertyCallbackInfo__GetIsolate(self) }
    }
  }

  pub unsafe trait Scope: Sized {
    fn cast_ref<S: Scope>(&self) -> &S {
      unsafe { &*(self as *const _ as *const S) }
    }
    fn cast_mut<S: Scope>(&mut self) -> &mut S {
      unsafe { &mut *(self as *mut _ as *mut S) }
    }
  }
}

mod data {
  use super::*;

  #[derive(Debug)]
  pub struct ScopeData {
    pub(super) isolate: NonNull<Isolate>,
    pub(super) context: Cell<Option<NonNull<Context>>>,
    pub(super) escape_slot: Option<NonNull<raw::EscapeSlot>>,
    parent: Option<NonNull<ScopeData>>,
    deferred_drop: bool,
    type_specific_data: ScopeTypeSpecificData,
  }

  impl ScopeData {
    pub(super) fn new_context_scope<'s>(
      isolate: &'s mut Isolate,
      context: Local<'s, Context>,
    ) -> &'s mut Self {
      Self::new_with(isolate, move |data| {
        data.context.set(Some(context.as_non_null()));
        data.type_specific_data =
          ScopeTypeSpecificData::ContextScope(raw::ContextScope::zeroed());
        match &mut data.type_specific_data {
          ScopeTypeSpecificData::ContextScope(raw) => raw.init(&*context),
          _ => unreachable!(),
        }
      })
    }

    pub(super) fn new_handle_scope(isolate: &mut Isolate) -> &mut Self {
      Self::new_with(isolate, |data| {
        data.type_specific_data =
          ScopeTypeSpecificData::HandleScope(raw::HandleScope::zeroed());
        match &mut data.type_specific_data {
          ScopeTypeSpecificData::HandleScope(raw) => {
            raw.init(data.isolate.as_ptr())
          }
          _ => unreachable!(),
        }
      })
    }

    pub(super) fn new_escapable_handle_scope(
      isolate: &mut Isolate,
    ) -> &mut Self {
      Self::new_with(isolate, |data| {
        data.type_specific_data = ScopeTypeSpecificData::EscapableHandleScope(
          raw::EscapableHandleScope::zeroed(),
        );
        match &mut data.type_specific_data {
          ScopeTypeSpecificData::EscapableHandleScope(raw) => {
            data.escape_slot = raw.init(data.isolate.as_ptr());
          }
          _ => unreachable!(),
        }
      })
    }

    pub(super) fn new_callback_scope<'s>(
      isolate: &'s mut Isolate,
      maybe_current_context: Option<Local<'s, Context>>,
    ) -> &'s mut Self {
      Self::new_with(isolate, |data| {
        data
          .context
          .set(maybe_current_context.map(|cx| cx.as_non_null()));
      })
    }

    // TODO(piscisaureus): use something more efficient than a separate heap
    // allocation for every scope.
    fn new_with<F>(isolate: &mut Isolate, init_fn: F) -> &mut Self
    where
      F: FnOnce(&mut Self),
    {
      let isolate_nn = unsafe { NonNull::new_unchecked(isolate) };
      let mut parent = isolate.get_current_scope();
      let context = parent
        .as_mut()
        .map(|p| unsafe { p.as_mut() })
        .and_then(|p| p.context.get())
        .map(NonNull::from);
      let escape_slot = parent
        .as_mut()
        .map(|p| unsafe { p.as_mut() })
        .and_then(|p| p.escape_slot)
        .map(NonNull::from);
      let data = Self {
        isolate: isolate_nn,
        parent,
        context: Cell::new(context),
        escape_slot,
        deferred_drop: false,
        type_specific_data: ScopeTypeSpecificData::default(),
      };
      let mut data_box = Box::new(data);
      (init_fn)(&mut *data_box);
      let data_ptr = Box::into_raw(data_box);
      isolate.set_current_scope(NonNull::new(data_ptr));
      unsafe { &mut *data_ptr }
    }

    fn drop(&mut self) {
      // Make our parent scope 'current' again.
      let parent = self.parent;
      let isolate = self.get_isolate_mut();
      isolate.set_current_scope(parent);

      // Turn the &mut self pointer back into a box and drop it.
      let _ = unsafe { Box::from_raw(self) };
    }

    pub(super) fn notify_scope_dropped(&mut self) {
      // This function is called when the `api::Scope` object is dropped.
      // With regard to (escapable) handle scopes: the Rust borrow checker
      // allows these to be dropped before all the local handles that were
      // created inside the HandleScope have gone out of scope. In order to
      // avoid turning these locals into invalid references the HandleScope is
      // kept alive for now -- it'll be actually dropped when the user touches
      // the HandleScope's parent scope.
      match &self.type_specific_data {
        ScopeTypeSpecificData::HandleScope(_)
        | ScopeTypeSpecificData::EscapableHandleScope(_) => {
          // Defer drop.
          let prev_flag_value = replace(&mut self.deferred_drop, true);
          assert_eq!(prev_flag_value, false);
        }
        _ => {
          // Regular, immediate drop.
          self.drop();
        }
      }
    }

    pub(super) fn as_scope<S: api::Scope>(&mut self) -> S {
      assert_eq!(size_of::<&mut Self>(), size_of::<S>());
      let self_nn = NonNull::from(self);
      unsafe { ptr::read(&self_nn as *const _ as *const S) }
    }

    pub(super) fn get<S: api::Scope>(scope: &S) -> &Self {
      let self_nn = unsafe { *(scope as *const _ as *const NonNull<Self>) };
      Self::touch(self_nn);
      unsafe { &*self_nn.as_ptr() }
    }

    pub(super) fn get_mut<S: api::Scope>(scope: &mut S) -> &mut Self {
      let self_nn = unsafe { *(scope as *mut _ as *mut NonNull<Self>) };
      Self::touch(self_nn);
      unsafe { &mut *self_nn.as_ptr() }
    }

    pub(super) fn get_isolate(&self) -> &Isolate {
      unsafe { self.isolate.as_ref() }
    }

    pub(super) fn get_isolate_mut(&mut self) -> &mut Isolate {
      unsafe { self.isolate.as_mut() }
    }

    pub(super) fn get_isolate_ptr(&self) -> *mut Isolate {
      self.isolate.as_ptr()
    }

    fn touch(self_nn: NonNull<ScopeData>) {
      loop {
        let current_scope = unsafe { self_nn.as_ref() }
          .get_isolate()
          .get_current_scope();
        match current_scope {
          Some(current_scope_nn) if current_scope_nn == self_nn => break,
          Some(mut current_scope_nn)
            if unsafe { current_scope_nn.as_ref().deferred_drop } =>
          unsafe { current_scope_nn.as_mut().drop() }
          _ => panic!("can't use scope that has been shadowed by another"),
        }
      }
    }

    pub(crate) fn reset(isolate: &mut Isolate) {
      loop {
        let current_scope = isolate.get_current_scope();
        match current_scope {
          None => break,
          Some(mut current_scope_nn)
            if unsafe { current_scope_nn.as_ref().deferred_drop } =>
          unsafe { current_scope_nn.as_mut().drop() }
          _ => panic!("can't reset scopes while they're in use"),
        }
      }
    }
  }

  #[derive(Debug)]
  pub(super) enum ScopeTypeSpecificData {
    None,
    ContextScope(raw::ContextScope),
    HandleScope(raw::HandleScope),
    EscapableHandleScope(raw::EscapableHandleScope),
  }

  impl Default for ScopeTypeSpecificData {
    fn default() -> Self {
      Self::None
    }
  }
}

mod raw {
  use super::*;

  #[derive(Clone, Copy)]
  #[repr(transparent)]
  pub(super) struct Address(NonZeroUsize);

  #[derive(Debug)]
  pub(super) struct ContextScope {
    entered_context: *const Context,
  }

  impl ContextScope {
    pub(super) fn zeroed() -> Self {
      Self {
        entered_context: null(),
      }
    }

    pub(super) fn init(&mut self, context: *const Context) {
      debug_assert!(self.entered_context.is_null());
      unsafe { v8__Context__Enter(context) };
      self.entered_context = context;
    }
  }

  impl Drop for ContextScope {
    fn drop(&mut self) {
      assert!(!self.entered_context.is_null());
      unsafe { v8__Context__Exit(self.entered_context) };
    }
  }

  #[repr(C)]
  #[derive(Debug)]
  pub(super) struct HandleScope {
    isolate_: *mut Isolate,
    prev_next_: *mut Address,
    prev_limit_: *mut Address,
  }

  impl HandleScope {
    pub(super) fn zeroed() -> Self {
      unsafe { MaybeUninit::<Self>::zeroed().assume_init() }
    }

    pub(super) fn init(&mut self, isolate: *mut Isolate) {
      debug_assert!(self.isolate_.is_null());
      let buf = self as *mut _ as *mut MaybeUninit<Self>;
      unsafe { v8__HandleScope__CONSTRUCT(buf, isolate) };
    }
  }

  impl Drop for HandleScope {
    fn drop(&mut self) {
      assert!(!self.isolate_.is_null());
      unsafe { v8__HandleScope__DESTRUCT(self) };
    }
  }

  #[derive(Debug)]
  pub(super) struct EscapeSlot(Option<NonNull<raw::Address>>);

  impl EscapeSlot {
    fn zeroed() -> Self {
      Self(None)
    }

    fn init(&mut self, isolate: *mut Isolate) {
      unsafe {
        let undefined = v8__Undefined(isolate) as *const Data;
        let local = v8__Local__New(isolate, undefined);
        let address_ptr =
          &*local as *const Data as *mut Data as *mut raw::Address;
        let slot = NonNull::new_unchecked(address_ptr);
        let uninit_slot = self.0.replace(slot);
        debug_assert!(uninit_slot.is_none());
      }
    }

    pub(super) fn escape<T>(&mut self, value: *const T) -> *const T {
      let slot_address_ptr = self
        .0
        .take()
        .map(NonNull::as_ptr)
        .expect("EscapableHandleScope::escape() called twice");
      let slot_address_mut = unsafe { &mut *slot_address_ptr };
      let value_address = unsafe { *(value as *const Address) };
      let prev_address = replace(slot_address_mut, value_address);
      debug_assert!(
        unsafe { &*(&prev_address as *const _ as *const Value) }.is_undefined()
      );
      slot_address_ptr as *mut T
    }
  }

  #[derive(Debug)]
  pub(super) struct EscapableHandleScope {
    handle_scope: raw::HandleScope,
    escape_slot: raw::EscapeSlot,
  }

  impl EscapableHandleScope {
    pub(super) fn zeroed() -> Self {
      Self {
        handle_scope: HandleScope::zeroed(),
        escape_slot: EscapeSlot::zeroed(),
      }
    }

    pub(super) fn init(
      &mut self,
      isolate: *mut Isolate,
    ) -> Option<NonNull<EscapeSlot>> {
      // Note: the `EscapeSlot` must be initialized *before* the HandleScope,
      // otherwise the escaped Local handle ends up in the EscapableHandleScope
      // itself rather than escaping from it.
      self.escape_slot.init(isolate);
      self.handle_scope.init(isolate);
      Some(NonNull::from(&mut self.escape_slot))
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

    pub(super) fn v8__Context__GetIsolate(this: *const Context)
      -> *mut Isolate;
    pub(super) fn v8__Context__Enter(this: *const Context);
    pub(super) fn v8__Context__Exit(this: *const Context);

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

    pub(super) fn v8__TryCatch__CONSTRUCT(
      buf: *mut MaybeUninit<TryCatch>,
      isolate: *mut Isolate,
    );
    pub(super) fn v8__TryCatch__DESTRUCT(this: *mut TryCatch);
    pub(super) fn v8__TryCatch__HasCaught(this: *const TryCatch) -> bool;
    pub(super) fn v8__TryCatch__CanContinue(this: *const TryCatch) -> bool;
    pub(super) fn v8__TryCatch__HasTerminated(this: *const TryCatch) -> bool;
    pub(super) fn v8__TryCatch__Exception(
      this: *const TryCatch,
    ) -> *const Value;
    pub(super) fn v8__TryCatch__StackTrace(
      this: *const TryCatch,
      context: *const Context,
    ) -> *const Value;
    pub(super) fn v8__TryCatch__Message(
      this: *const TryCatch,
    ) -> *const Message;
    pub(super) fn v8__TryCatch__Reset(this: *mut TryCatch);
    pub(super) fn v8__TryCatch__ReThrow(this: *mut TryCatch) -> *const Value;
    pub(super) fn v8__TryCatch__IsVerbose(this: *const TryCatch) -> bool;
    pub(super) fn v8__TryCatch__SetVerbose(this: *mut TryCatch, value: bool);
    pub(super) fn v8__TryCatch__SetCaptureMessage(
      this: *mut TryCatch,
      value: bool,
    );

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
