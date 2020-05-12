use std::cell::Cell;
use std::marker::PhantomData;
use std::mem::align_of;
use std::mem::needs_drop;
use std::mem::replace;
use std::mem::size_of;
use std::mem::take;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr;
use std::ptr::drop_in_place;
use std::ptr::NonNull;

use crate::get_isolate::GetRawIsolate;
use crate::Context;
use crate::Data;
use crate::Isolate;
use crate::Local;
use crate::Message;
use crate::Primitive;
use crate::Value;
pub(crate) use internal::ScopeStore;

use internal::ActiveScopeData;
use internal::ScopeCookie;
use internal::ScopeData;
use params::ScopeParams;
use params::{No, Yes};

pub struct Ref<'a, Scope: ScopeParams> {
  scope: Scope,
  _lifetime: PhantomData<&'a mut ()>,
}

impl<'a, Scope: ScopeParams> Ref<'a, Scope> {
  #[inline(always)]
  fn new(scope: Scope) -> Self {
    Self {
      scope,
      _lifetime: PhantomData,
    }
  }

  pub fn enter(&mut self) -> &mut Scope {
    &mut self.scope
  }
}

impl<'a, Scope: ScopeParams> Drop for Ref<'a, Scope> {
  #[inline(always)]
  fn drop(&mut self) {
    ScopeStore::drop_scope(&mut self.scope)
  }
}

#[derive(Debug)]
pub struct Scope<Handles = No, Escape = No, TryCatch = No> {
  store: *const ScopeStore,
  cookie: ScopeCookie,
  frame_count: u32,
  _phantom: PhantomData<(Handles, Escape, TryCatch)>,
}

impl<Handles, Escape, TryCatch> Drop for Scope<Handles, Escape, TryCatch> {
  fn drop(&mut self) {
    println!("Drop Scope");
  }
}

impl<'t, Handles, Escape> Deref for Scope<Handles, Escape, Yes<'t>> {
  type Target = Scope<Handles, Escape, No>;
  #[inline(always)]
  fn deref(&self) -> &Self::Target {
    unsafe { Self::Target::cast(self) }
  }
}

impl<'t, Handles, Escape> DerefMut for Scope<Handles, Escape, Yes<'t>> {
  #[inline(always)]
  fn deref_mut(&mut self) -> &mut Self::Target {
    unsafe { Self::Target::cast_mut(self) }
  }
}

impl<'h, 'e: 'h> Deref for Scope<Yes<'h>, Yes<'e>, No> {
  type Target = Scope<Yes<'h>, No, No>;
  #[inline(always)]
  fn deref(&self) -> &Self::Target {
    unsafe { Self::Target::cast(self) }
  }
}

impl<'h, 'e: 'h> DerefMut for Scope<Yes<'h>, Yes<'e>, No> {
  #[inline(always)]
  fn deref_mut(&mut self) -> &mut Self::Target {
    unsafe { Self::Target::cast_mut(self) }
  }
}

impl<'h> Deref for Scope<Yes<'h>, No, No> {
  type Target = Scope<No, No, No>;
  #[inline(always)]
  fn deref(&self) -> &Self::Target {
    unsafe { Self::Target::cast(self) }
  }
}

impl<'h> DerefMut for Scope<Yes<'h>, No, No> {
  #[inline(always)]
  fn deref_mut(&mut self) -> &mut Self::Target {
    unsafe { Self::Target::cast_mut(self) }
  }
}

impl Deref for Scope<No, No, No> {
  type Target = Isolate;
  #[inline(always)]
  fn deref(&self) -> &Self::Target {
    let p = self as *const Self;
    let p = p as *mut Self;
    let p = unsafe { &mut *p };
    p.isolate()
  }
}

impl DerefMut for Scope<No, No, No> {
  #[inline(always)]
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.isolate()
  }
}

impl<Handles, Escape, TryCatch> Scope<Handles, Escape, TryCatch> {
  #[inline(always)]
  unsafe fn cast<Handles_, Escape_, TryCatch_>(
    from: &Scope<Handles_, Escape_, TryCatch_>,
  ) -> &Self {
    &*(from as *const _ as *const Self)
  }

  #[inline(always)]
  pub(crate) unsafe fn cast_mut<Handles_, Escape_, TryCatch_>(
    from: &mut Scope<Handles_, Escape_, TryCatch_>,
  ) -> &mut Self {
    &mut *(from as *mut _ as *mut Self)
  }
}

impl Scope {
  #[inline(always)]
  pub(crate) fn root(scope_store: &'_ ScopeStore) -> Self {
    ScopeStore::new_root_scope_with(scope_store, |_| ())
  }

  #[inline(always)]
  pub(crate) fn drop_root(&mut self) {
    ScopeStore::drop_scope(self);
  }

  #[inline(always)]
  pub fn isolate_scope<'a>(isolate: &'_ Isolate) -> Ref<'a, Self> {
    ScopeStore::new_scope_with(isolate.get_scopes(), |s| {
      s.push::<data::Context>(None);
    })
  }
}

impl<Handles, Escape, TryCatch> Scope<Handles, Escape, TryCatch> {
  #[inline(always)]
  pub fn context_scope<'a>(
    parent: &'a mut Scope<Handles, Escape, TryCatch>,
    context: Local<'_, Context>,
  ) -> Ref<'a, Self> {
    let context_ptr: *const Context = &*context;
    let context_ptr = NonNull::new(context_ptr as *mut _).unwrap();
    ScopeStore::new_inner_scope_with(parent, |s| {
      s.push::<data::Context>(Some(context_ptr));
    })
  }

  #[inline(always)]
  pub fn isolate(&mut self) -> &mut Isolate {
    let isolate_ptr = ScopeStore::with_mut(self, |s| s.get_isolate_ptr());
    unsafe { &mut *isolate_ptr }
  }

  #[inline(always)]
  pub fn context(&mut self) -> &Context {
    let context_data: data::Context = ScopeStore::get_data(self);
    let context_nn = match context_data {
      data::Context::CurrentCached(maybe_nn) => {
        maybe_nn.expect("no context has been entered")
      }
      data::Context::Entered(nn) => nn,
      _ => unreachable!(),
    };
    unsafe { &*context_nn.as_ptr() }
  }

  #[inline(always)]
  pub(super) fn get_store<'a>(&'_ self) -> &'a ScopeStore {
    unsafe { &*self.store }
  }
}

impl<'h, Escape, TryCatch> Scope<Yes<'h>, Escape, TryCatch> {
  #[inline(always)]
  pub fn handle_scope<'a, Handles_>(
    parent: &'a mut Scope<Handles_, Escape, TryCatch>,
  ) -> Ref<'h, Self> {
    ScopeStore::new_inner_scope_with(parent, |s| {
      s.push::<data::HandleScope>(());
    })
  }

  #[inline(always)]
  #[allow(clippy::wrong_self_convention)]
  pub unsafe fn to_local<'a, T>(
    &'_ mut self,
    ptr: *const T,
  ) -> Option<Local<'a, T>>
  where
    'h: 'a,
  {
    // Do not remove. This access verifies that `self` is the topmost scope.
    let _: data::HandleScope = ScopeStore::get_data(self);
    Local::from_raw(ptr)
  }

  pub fn get_current_context(&mut self) -> Option<Local<'h, Context>> {
    let isolate = self.isolate();
    let context_ptr = unsafe { raw::v8__Isolate__GetCurrentContext(isolate) };
    unsafe { self.to_local(context_ptr) }
  }

  pub fn get_entered_or_microtask_context(
    &mut self,
  ) -> Option<Local<'h, Context>> {
    let isolate = self.isolate();
    let context_ptr =
      unsafe { raw::v8__Isolate__GetEnteredOrMicrotaskContext(isolate) };
    unsafe { self.to_local(context_ptr) }
  }
}

impl<'h, 'e: 'h, TryCatch> Scope<Yes<'h>, Yes<'e>, TryCatch> {
  #[inline(always)]
  pub fn escapable_handle_scope<'a, Escape_>(
    parent: &'a mut Scope<Yes<'e>, Escape_, TryCatch>,
  ) -> Ref<'a, Self> {
    ScopeStore::new_inner_scope_with(parent, |s| {
      s.push::<data::EscapeSlot>(());
      s.push::<data::HandleScope>(());
    })
  }

  #[inline(always)]
  pub fn escape<T>(&'_ mut self, value: Local<'h, T>) -> Local<'e, T> {
    let escape_slot_data: data::EscapeSlot = ScopeStore::take_data(self);
    let mut slot_nn = escape_slot_data
      .expect("only one value can escape from an EscapableHandleScope");
    let slot_mut = unsafe { slot_nn.as_mut() };
    let address = unsafe { *(&*value as *const T as *const raw::Address) };
    replace(slot_mut, address);
    let result_nn = slot_nn.cast::<T>();
    unsafe { Local::from_raw_non_null(result_nn) }
  }
}

impl<'t, Handles, Escape> Scope<Handles, Escape, Yes<'t>> {
  #[inline(always)]
  pub fn try_catch<'a, TryCatch_>(
    parent: &'a mut Scope<Handles, Escape, TryCatch_>,
  ) -> Ref<'a, Self> {
    ScopeStore::new_inner_scope_with(parent, |s| {
      s.push::<data::TryCatch>(());
    })
  }

  /// Returns true if an exception has been caught by this try/catch block.
  #[inline(always)]
  pub fn has_caught(&mut self) -> bool {
    let data: data::TryCatch = ScopeStore::get_data(self);
    unsafe { raw::v8__TryCatch__HasCaught(&*data) }
  }

  /// For certain types of exceptions, it makes no sense to continue execution.
  ///
  /// If CanContinue returns false, the correct action is to perform any C++
  /// cleanup needed and then return. If CanContinue returns false and
  /// HasTerminated returns true, it is possible to call
  /// CancelTerminateExecution in order to continue calling into the engine.
  #[inline(always)]
  pub fn can_continue(&mut self) -> bool {
    let data: data::TryCatch = ScopeStore::get_data(self);
    unsafe { raw::v8__TryCatch__CanContinue(&*data) }
  }

  /// Returns true if an exception has been caught due to script execution
  /// being terminated.
  ///
  /// There is no JavaScript representation of an execution termination
  /// exception. Such exceptions are thrown when the TerminateExecution
  /// methods are called to terminate a long-running script.
  ///
  /// If such an exception has been thrown, HasTerminated will return true,
  /// indicating that it is possible to call CancelTerminateExecution in order
  /// to continue calling into the engine.
  #[inline(always)]
  pub fn has_terminated(&mut self) -> bool {
    let data: data::TryCatch = ScopeStore::get_data(self);
    unsafe { raw::v8__TryCatch__HasTerminated(&*data) }
  }

  /// Returns the exception caught by this try/catch block. If no exception has
  /// been caught an empty handle is returned.
  ///
  /// The returned handle is valid until this TryCatch block has been destroyed.
  #[inline(always)]
  pub fn exception(&'_ mut self) -> Option<Local<'t, Value>> {
    let data: data::TryCatch = ScopeStore::get_data(self);
    unsafe { Local::from_raw(raw::v8__TryCatch__Exception(&*data)) }
  }

  /// Returns the message associated with this exception. If there is
  /// no message associated an empty handle is returned.
  ///
  /// The returned handle is valid until this TryCatch block has been
  /// destroyed.
  #[inline(always)]
  pub fn message(&'_ mut self) -> Option<Local<'t, Message>> {
    let data: data::TryCatch = ScopeStore::get_data(self);
    unsafe { Local::from_raw(raw::v8__TryCatch__Message(&*data)) }
  }

  /// Clears any exceptions that may have been caught by this try/catch block.
  /// After this method has been called, HasCaught() will return false. Cancels
  /// the scheduled exception if it is caught and ReThrow() is not called before.
  ///
  /// It is not necessary to clear a try/catch block before using it again; if
  /// another exception is thrown the previously caught exception will just be
  /// overwritten. However, it is often a good idea since it makes it easier
  /// to determine which operation threw a given exception.
  #[inline(always)]
  pub fn reset(&mut self) {
    let mut data: data::TryCatch = ScopeStore::get_data(self);
    unsafe { raw::v8__TryCatch__Reset(&mut *data) };
  }

  /// Returns true if verbosity is enabled.
  #[inline(always)]
  pub fn is_verbose(&mut self) -> bool {
    let data: data::TryCatch = ScopeStore::get_data(self);
    unsafe { raw::v8__TryCatch__IsVerbose(&*data) }
  }

  /// Set verbosity of the external exception handler.
  ///
  /// By default, exceptions that are caught by an external exception
  /// handler are not reported. Call SetVerbose with true on an
  /// external exception handler to have exceptions caught by the
  /// handler reported as if they were not caught.
  #[inline(always)]
  pub fn set_verbose(&mut self, value: bool) {
    let mut data: data::TryCatch = ScopeStore::get_data(self);
    unsafe { raw::v8__TryCatch__SetVerbose(&mut *data, value) };
  }

  /// Set whether or not this TryCatch should capture a Message object
  /// which holds source information about where the exception
  /// occurred. True by default.
  #[inline(always)]
  pub fn set_capture_message(&mut self, value: bool) {
    let mut data: data::TryCatch = ScopeStore::get_data(self);
    unsafe { raw::v8__TryCatch__SetCaptureMessage(&mut *data, value) };
  }
}

impl<'h, 't, Escape> Scope<Yes<'h>, Escape, Yes<'t>> {
  /// Returns the .stack property of the thrown object. If no .stack
  /// property is present an empty handle is returned.
  #[inline(always)]
  pub fn stack_trace(&'_ mut self) -> Option<Local<'h, Value>> {
    let data: data::TryCatch = ScopeStore::get_data(self);
    let context = self.context();
    unsafe { Local::from_raw(raw::v8__TryCatch__StackTrace(&*data, context)) }
  }

  /// Throws the exception caught by this TryCatch in a way that avoids
  /// it being caught again by this same TryCatch. As with ThrowException
  /// it is illegal to execute any JavaScript operations after calling
  /// ReThrow; the caller must return immediately to where the exception
  /// is caught.
  #[inline(always)]
  pub fn rethrow(&'_ mut self) -> Option<Local<'h, Value>> {
    let mut data: data::TryCatch = ScopeStore::get_data(self);
    unsafe { Local::from_raw(raw::v8__TryCatch__ReThrow(&mut *data)) }
  }
}

pub type HandleScope<'h> = Scope<Yes<'h>, No, No>;

impl<'h> HandleScope<'h> {
  #[allow(clippy::new_ret_no_self)]
  #[inline(always)]
  pub fn new<'a, Handles_, Escape, TryCatch>(
    parent: &'a mut Scope<Handles_, Escape, TryCatch>,
  ) -> Ref<'a, Scope<Yes<'h>, Escape, TryCatch>> {
    Scope::handle_scope(parent)
  }
}

pub type EscapableHandleScope<'h, 'e> = Scope<Yes<'h>, Yes<'e>, No>;

impl<'h, 'e: 'h> EscapableHandleScope<'h, 'e> {
  #[allow(clippy::new_ret_no_self)]
  #[inline(always)]
  pub fn new<'a, Escape_, TryCatch>(
    parent: &'a mut Scope<Yes<'e>, Escape_, TryCatch>,
  ) -> Ref<'a, Scope<Yes<'h>, Yes<'e>, TryCatch>> {
    Scope::escapable_handle_scope(parent)
  }
}

pub type TryCatch<'t> = Scope<No, No, Yes<'t>>;

impl<'t> TryCatch<'t> {
  #[allow(clippy::new_ret_no_self)]
  #[inline(always)]
  pub fn new<'a, Handles, Escape, TryCatch_>(
    parent: &'a mut Scope<Handles, Escape, TryCatch_>,
  ) -> Ref<'a, Scope<Handles, Escape, Yes<'t>>> {
    Scope::try_catch(parent)
  }
}

// TODO: Remove me. Temporarily added for compatibility with the old API.
impl<Handles, Escape, TryCatch> Scope<Handles, Escape, TryCatch> {
  #[inline(always)]
  pub fn enter(&mut self) -> &mut Self {
    self
  }
}

// TODO: Remove me. Temporarily added for compatibility with the old API.
pub struct ContextScope;
impl ContextScope {
  #[allow(clippy::new_ret_no_self)]
  #[inline(always)]
  pub fn new<'a, Handles, Escape, TryCatch>(
    parent: &'a mut Scope<Handles, Escape, TryCatch>,
    context: Local<'_, Context>,
  ) -> Ref<'a, Scope<Handles, Escape, TryCatch>> {
    Scope::context_scope(parent, context)
  }
}

// TODO: Remove me. Temporarily added for compatibility with the old API.
impl Scope {
  #[inline(always)]
  pub fn for_callback<'a>(
    bearer: &impl GetRawIsolate,
  ) -> Ref<'a, Scope<No, No, No>> {
    let isolate = bearer.get_raw_isolate();
    let isolate = unsafe { &*isolate };
    let scope_store = isolate.get_scopes();
    ScopeStore::new_scope_with(scope_store, |s| {
      s.assert_same_isolate(isolate);
      s.push::<data::Context>(None);
    })
  }

  #[inline(always)]
  pub fn for_callback_with_handle_scope<'a>(
    bearer: &impl GetRawIsolate,
  ) -> Ref<'a, Scope<Yes<'a>, No, No>> {
    let isolate = bearer.get_raw_isolate();
    let isolate = unsafe { &*isolate };
    let scope_store = isolate.get_scopes();
    ScopeStore::new_scope_with(scope_store, |s| {
      s.assert_same_isolate(isolate);
      s.push::<data::Context>(None);
    })
  }

  #[inline(always)]
  pub(crate) fn for_function_or_property_callback<'a, I: GetRawIsolate>(
    info: *const I,
  ) -> Ref<'a, Scope<Yes<'a>, No, No>> {
    let info = unsafe { &*info };
    Self::for_callback_with_handle_scope(info)
  }
}

mod params {
  use super::*;

  #[derive(Debug)]
  pub struct Yes<'t>(PhantomData<&'t ()>);
  #[derive(Debug)]
  pub struct No;

  pub trait ScopeParams: Sized {
    type Handles;
    type Escape;
    type TryCatch;

    fn as_scope(&self) -> &Scope<Self::Handles, Self::Escape, Self::TryCatch>;
    fn as_scope_mut(
      &mut self,
    ) -> &mut Scope<Self::Handles, Self::Escape, Self::TryCatch>;
  }

  impl<Handles, Escape, TryCatch> ScopeParams
    for Scope<Handles, Escape, TryCatch>
  {
    type Handles = Handles;
    type Escape = Escape;
    type TryCatch = TryCatch;

    #[inline(always)]
    fn as_scope(&self) -> &Self {
      self
    }
    #[inline(always)]
    fn as_scope_mut(&mut self) -> &mut Self {
      self
    }
  }
}

mod data {
  use super::*;

  #[derive(Clone, Copy, Debug)]
  pub(super) enum Context {
    Current,
    CurrentCached(Option<NonNull<super::Context>>),
    Entered(NonNull<super::Context>),
  }

  impl Default for Context {
    fn default() -> Self {
      Self::Current
    }
  }

  impl ScopeData for Context {
    type Args = Option<NonNull<super::Context>>;
    type Raw = ();

    #[inline(always)]
    fn activate(
      _raw: *mut Self::Raw,
      args: &mut Self::Args,
      _isolate: &mut Isolate,
      active_scope_data: &mut ActiveScopeData,
    ) -> Self {
      let new_context_data = match args.take() {
        None => Self::default(),
        Some(context_nn) => {
          unsafe { context_nn.as_ref() }.enter();
          Self::Entered(context_nn)
        }
      };
      replace(&mut active_scope_data.context, new_context_data)
    }

    #[inline(always)]
    fn deactivate(
      _raw: *mut Self::Raw,
      previous: Self,
      _isolate: &mut Isolate,
      active_scope_data: &mut ActiveScopeData,
    ) {
      let prev_context_data = replace(&mut active_scope_data.context, previous);
      if let Self::Entered(context_nn) = prev_context_data {
        unsafe { context_nn.as_ref() }.exit();
      }
    }

    #[inline(always)]
    fn get_mut<'a>(
      isolate: &'a mut Isolate,
      active_scope_data: &'a mut ActiveScopeData,
    ) -> &'a mut Self {
      if let Self::Current = active_scope_data.context {
        let context = unsafe { raw::v8__Isolate__GetCurrentContext(isolate) };
        let context = NonNull::new(context as *mut _);
        replace(&mut active_scope_data.context, Self::CurrentCached(context));
      }
      &mut active_scope_data.context
    }
  }

  #[derive(Clone, Copy, Debug, Default)]
  pub(super) struct HandleScope(Option<NonNull<raw::HandleScope>>);

  impl ScopeData for HandleScope {
    type Args = ();
    type Raw = raw::HandleScope;

    #[inline(always)]
    fn construct(
      buf: *mut Self::Raw,
      _args: &mut Self::Args,
      isolate: &mut Isolate,
    ) {
      let buf = buf as *mut MaybeUninit<Self::Raw>;
      unsafe { raw::v8__HandleScope__CONSTRUCT(buf, isolate) }
    }

    fn destruct(raw: *mut Self::Raw) {
      unsafe { raw::v8__HandleScope__DESTRUCT(raw) }
    }

    #[inline(always)]
    fn activate(
      raw: *mut Self::Raw,
      _args: &mut Self::Args,
      _isolate: &mut Isolate,
      active_scope_data: &mut ActiveScopeData,
    ) -> Self {
      replace(&mut active_scope_data.handle_scope, Self(NonNull::new(raw)))
    }

    #[inline(always)]
    fn get_mut<'a>(
      _isolate: &'a mut Isolate,
      active_scope_data: &'a mut ActiveScopeData,
    ) -> &'a mut Self {
      &mut active_scope_data.handle_scope
    }
  }

  #[derive(Clone, Copy, Debug, Default)]
  pub(super) struct EscapeSlot(Option<NonNull<raw::Address>>);

  impl ScopeData for EscapeSlot {
    type Args = ();
    type Raw = ();

    #[inline(always)]
    fn activate(
      _raw: *mut Self::Raw,
      _args: &mut Self::Args,
      isolate: &mut Isolate,
      active_scope_data: &mut ActiveScopeData,
    ) -> Self {
      let undefined: &Data = unsafe { &*raw::v8__Undefined(isolate) };
      let slot_ref: &Data =
        unsafe { &*raw::v8__Local__New(isolate, undefined) };
      let slot_nn = NonNull::from(slot_ref).cast::<raw::Address>();
      let escape_slot_data = Self(Some(slot_nn));
      replace(&mut active_scope_data.escape_slot, escape_slot_data)
    }

    #[inline(always)]
    fn get_mut<'a>(
      _isolate: &'a mut Isolate,
      active_scope_data: &'a mut ActiveScopeData,
    ) -> &'a mut Self {
      &mut active_scope_data.escape_slot
    }
  }

  impl Deref for EscapeSlot {
    type Target = Option<NonNull<raw::Address>>;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
      &self.0
    }
  }

  impl DerefMut for EscapeSlot {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
      &mut self.0
    }
  }

  #[derive(Clone, Copy, Debug, Default)]
  pub(super) struct TryCatch(Option<NonNull<raw::TryCatch>>);

  impl ScopeData for TryCatch {
    type Args = ();
    type Raw = raw::TryCatch;

    #[inline(always)]
    fn construct(
      buf: *mut Self::Raw,
      _args: &mut Self::Args,
      isolate: &mut Isolate,
    ) {
      let buf = buf as *mut MaybeUninit<Self::Raw>;
      unsafe { raw::v8__TryCatch__CONSTRUCT(buf, isolate) };
    }

    #[inline(always)]
    fn destruct(raw: *mut Self::Raw) {
      unsafe { raw::v8__TryCatch__DESTRUCT(raw) };
    }

    #[inline(always)]
    fn activate(
      raw: *mut Self::Raw,
      _args: &mut Self::Args,
      _isolate: &mut Isolate,
      active_scope_data: &mut ActiveScopeData,
    ) -> Self {
      replace(&mut active_scope_data.try_catch, Self(NonNull::new(raw)))
    }

    #[inline(always)]
    fn get_mut<'a>(
      _isolate: &'a mut Isolate,
      active_scope_data: &'a mut ActiveScopeData,
    ) -> &'a mut Self {
      &mut active_scope_data.try_catch
    }
  }

  impl Deref for TryCatch {
    type Target = raw::TryCatch;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
      unsafe { &*self.0.unwrap().as_ptr() }
    }
  }

  impl DerefMut for TryCatch {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
      unsafe { &mut *self.0.unwrap().as_ptr() }
    }
  }
}

mod internal {
  use super::*;

  pub(super) trait ScopeInit: ScopeParams {
    fn new_with_store(store: &ScopeStore) -> Self;
  }

  impl<Handles, Escape, TryCatch> ScopeInit for Scope<Handles, Escape, TryCatch> {
    #[inline(always)]
    fn new_with_store(store: &ScopeStore) -> Self {
      Self {
        store,
        cookie: ScopeCookie::NONE,
        frame_count: 0,
        _phantom: PhantomData,
      }
    }
  }

  #[derive(Debug)]
  pub(crate) struct ScopeStore {
    top_scope_cookie: Cell<ScopeCookie>,
    inner: ScopeStoreInner,
  }

  impl ScopeStore {
    pub fn new(isolate: &mut Isolate) -> Self {
      Self {
        top_scope_cookie: Default::default(),
        inner: ScopeStoreInner::new(isolate),
      }
    }

    #[inline(always)]
    pub(super) fn with_mut<R>(
      scope: &mut impl ScopeParams,
      f: impl Fn(&mut ScopeStoreInner) -> R,
    ) -> R {
      let scope = scope.as_scope_mut();
      let self_ = scope.get_store();
      ScopeCookie::borrow(&self_.top_scope_cookie, scope.cookie);
      let result = {
        // This is safe because we can only reach this point when `scope.cookie`
        // matches `top_scope_cookie`. There is only one scope at any time with
        // a matching cookie, and it can only enter here once as our cookie
        // temporarily changes to `ScopeCookie::BORROWED` when it does.
        #[allow(clippy::cast_ref_to_mut)]
        let inner =
          unsafe { &mut *(&self_.inner as *const _ as *mut ScopeStoreInner) };
        // TODO: assigning `scope.frame_count` to `inner.top_scope_frame_count`
        // and back does not seem to get optimized out, even if it should be
        // clear that there is no aliasing taking place. E.g. `to_local()`
        // produces this assembly code:
        //  mov ecx, dword ptr [rdi + 12]  # top_scope_frame_count = frame_count
        //  mov dword ptr [rax + 88], 0
        //  mov dword ptr [rdi + 12], ecx  # frame_count = top_scope_frame_count
        // It should be possible to avoid this.
        debug_assert_eq!(inner.top_scope_frame_count, 0);
        inner.top_scope_frame_count = scope.frame_count;
        let result = f(inner);
        scope.frame_count = take(&mut inner.top_scope_frame_count);
        result
      };
      ScopeCookie::unborrow(&self_.top_scope_cookie, scope.cookie);
      result
    }

    #[inline(always)]
    pub(super) fn get_data<D: ScopeData + Copy, Scope: ScopeParams>(
      scope: &mut Scope,
    ) -> D {
      Self::with_mut(scope, |inner| *inner.get_data_mut::<D>())
    }

    #[inline(always)]
    pub(super) fn take_data<D: ScopeData, Scope: ScopeParams>(
      scope: &mut Scope,
    ) -> D {
      Self::with_mut(scope, |inner| take(inner.get_data_mut::<D>()))
    }

    #[inline(always)]
    fn init_scope_with<Scope: ScopeParams>(
      &self,
      scope: &mut Scope,
      f: impl Fn(&mut ScopeStoreInner) -> (),
    ) {
      //println!("New scope: {}", std::any::type_name::<Scope>());
      let scope = scope.as_scope_mut();

      let next_cookie = ScopeCookie::next(&self.top_scope_cookie);
      ScopeCookie::set(&mut scope.cookie, next_cookie);

      debug_assert_eq!(scope.frame_count, 0);
      Self::with_mut(scope, f);
    }

    #[inline(always)]
    pub(super) fn new_scope_with<'a, Scope: ScopeInit>(
      &self,
      f: impl Fn(&mut ScopeStoreInner),
    ) -> Ref<'a, Scope> {
      let mut scope = Scope::new_with_store(self);
      self.init_scope_with(&mut scope, f);
      Ref::<'a, Scope>::new(scope)
    }

    #[inline(always)]
    pub(super) fn new_inner_scope_with<'a, Scope: ScopeInit>(
      parent: &'_ mut impl ScopeParams,
      f: impl Fn(&mut ScopeStoreInner),
    ) -> Ref<'a, Scope> {
      let parent = parent.as_scope_mut();
      let store = parent.get_store();
      assert_eq!(parent.cookie, store.top_scope_cookie.get());
      store.new_scope_with(f)
    }

    #[inline(always)]
    pub(super) fn new_root_scope_with<Scope: ScopeInit>(
      &self,
      f: impl Fn(&mut ScopeStoreInner),
    ) -> Scope {
      assert_eq!(ScopeCookie::NONE, self.top_scope_cookie.get());
      let mut scope = Scope::new_with_store(self);
      self.init_scope_with(&mut scope, f);
      scope
    }

    #[inline(always)]
    pub fn drop_scope<Scope: ScopeParams>(scope: &mut Scope) {
      //println!("Drop scope: {}", std::any::type_name::<Scope>());
      let scope = scope.as_scope_mut();

      Self::with_mut(scope, |inner| {
        while inner.top_scope_frame_count > 0 {
          inner.pop()
        }
      });
      debug_assert_eq!(scope.frame_count, 0);

      let self_ = scope.get_store();
      let cookie = ScopeCookie::revert(&self_.top_scope_cookie);
      ScopeCookie::reset(&mut scope.cookie, cookie);
    }
  }

  impl Drop for ScopeStore {
    fn drop(&mut self) {
      assert_eq!(self.top_scope_cookie.get(), ScopeCookie::default());
    }
  }

  #[derive(Debug)]
  pub(super) struct ScopeStoreInner {
    isolate: *mut Isolate,
    active_scope_data: ActiveScopeData,
    frame_stack: Vec<u8>,
    top_scope_frame_count: u32,
  }

  impl ScopeStoreInner {
    fn new(isolate: &mut Isolate) -> Self {
      Self {
        isolate,
        active_scope_data: Default::default(),
        frame_stack: Vec::with_capacity(Self::FRAME_STACK_SIZE),
        top_scope_frame_count: 0,
      }
    }
  }

  impl Drop for ScopeStoreInner {
    fn drop(&mut self) {
      //println!("Drop ScopeStoreInner")
      assert_eq!(self.top_scope_frame_count, 0);
      assert_eq!(self.frame_stack.len(), 0);
    }
  }

  impl ScopeStoreInner {
    const FRAME_STACK_SIZE: usize = 4096 - size_of::<usize>();

    #[inline(always)]
    pub fn assert_same_isolate(&self, isolate: &Isolate) {
      let isolate = isolate as *const _ as *mut Isolate;
      assert_eq!(isolate, self.isolate);
    }

    #[allow(dead_code)]
    #[inline(always)]
    pub fn get_isolate_ptr(&self) -> *mut Isolate {
      self.isolate
    }

    #[inline(always)]
    pub fn get_data_mut<D: ScopeData>(&mut self) -> &mut D {
      let isolate = unsafe { &mut *self.isolate };
      D::get_mut(isolate, &mut self.active_scope_data)
    }

    #[inline(always)]
    pub fn push<D: ScopeData>(&mut self, mut args: D::Args) {
      let Self {
        isolate,
        active_scope_data,
        frame_stack,
        top_scope_frame_count,
      } = self;
      let isolate = unsafe { &mut **isolate };

      *top_scope_frame_count += 1;

      unsafe {
        // Determine byte range on the stack that the new frame will occupy.
        let frame_byte_length = size_of::<ScopeStackFrame<D>>();
        let frame_byte_offset = frame_stack.len();

        // Increase the stack limit to fit the new frame.
        let new_stack_byte_length = frame_byte_offset + frame_byte_length;
        assert!(new_stack_byte_length <= frame_stack.capacity());
        frame_stack.set_len(new_stack_byte_length);

        // Obtain a pointer to the new stack frame.
        let frame_ptr = frame_stack.get_mut(frame_byte_offset).unwrap();
        let frame_ptr: *mut ScopeStackFrame<D> = cast_mut_ptr(frame_ptr);

        // Intialize the raw data part of the new stack frame.
        let raw_ptr: *mut D::Raw = &mut (*frame_ptr).raw;
        D::construct(raw_ptr, &mut args, isolate);

        // Update the reference in the ActiveScopeData structure.
        let previous_active =
          D::activate(raw_ptr, &mut args, isolate, active_scope_data);
        let previous_active_ptr: *mut D = &mut (*frame_ptr).previous_active;
        ptr::write(previous_active_ptr, previous_active);

        // Write the metadata part of the new stack frame. It contains the
        // pointer to a cleanup function specific to this type of frame.
        let metadata = ScopeStackFrameMetadata {
          cleanup_fn: Self::cleanup_frame::<D>,
        };
        let metadata_ptr: *mut _ = &mut (*frame_ptr).metadata;
        ptr::write(metadata_ptr, metadata);
      };
    }

    #[inline(always)]
    pub fn pop(&mut self) {
      let Self {
        isolate,
        active_scope_data,
        frame_stack,
        top_scope_frame_count,
      } = self;
      let isolate = unsafe { &mut **isolate };

      debug_assert!(*top_scope_frame_count > 0);
      *top_scope_frame_count -= 1;

      // Locate the metadata part of the stack frame we want to pop.
      let metadata_byte_length = size_of::<ScopeStackFrameMetadata>();
      let metadata_byte_offset = frame_stack.len() - metadata_byte_length;
      let metadata_ptr = frame_stack.get_mut(metadata_byte_offset).unwrap();
      let metadata_ptr: *mut ScopeStackFrameMetadata =
        cast_mut_ptr(metadata_ptr);
      let metadata = unsafe { ptr::read(metadata_ptr) };

      // Call the frame's cleanup handler.
      let cleanup_fn = metadata.cleanup_fn;
      let frame_byte_length =
        unsafe { cleanup_fn(metadata_ptr, isolate, active_scope_data) };
      let frame_byte_offset = frame_stack.len() - frame_byte_length;

      // Decrease the stack limit.
      unsafe { frame_stack.set_len(frame_byte_offset) };
    }

    unsafe fn cleanup_frame<D: ScopeData>(
      metadata_ptr: *mut ScopeStackFrameMetadata,
      isolate: &mut Isolate,
      active_scope_data: &mut ActiveScopeData,
    ) -> usize {
      // From the stack frame metadata pointer, determine the start address of
      // the whole stack frame.
      let frame_byte_length = size_of::<ScopeStackFrame<D>>();
      let metadata_byte_length = size_of::<ScopeStackFrameMetadata>();
      let byte_offset_from_frame = frame_byte_length - metadata_byte_length;
      let frame_address = (metadata_ptr as usize) - byte_offset_from_frame;
      let frame_ptr = frame_address as *mut u8;
      let frame_ptr: *mut ScopeStackFrame<D> = cast_mut_ptr(frame_ptr);

      // Locate the pointers to the other data members within the frame.
      let raw_ptr: *mut D::Raw = &mut (*frame_ptr).raw;
      let previous_active_ptr: *mut D = &mut (*frame_ptr).previous_active;

      // Restore the relevant ActiveScopeData slot to its previous value.
      let previous_active = ptr::read(previous_active_ptr);
      D::deactivate(raw_ptr, previous_active, isolate, active_scope_data);

      // Call the destructor for the raw data part of the frame.
      D::destruct(raw_ptr);

      // Return the number of bytes that this frame used to occupy on the stack,
      // so `pop()` can adjust the stack limit accordingly.
      frame_byte_length
    }
  }

  /// Raw mutable pointer cast that checks (if necessary) that the returned
  /// pointer is properly aligned.
  #[inline(always)]
  fn cast_mut_ptr<Source, Target>(source: *mut Source) -> *mut Target {
    let source_align = align_of::<Source>();
    let target_align = align_of::<Target>();
    let address = source as usize;
    if target_align > source_align {
      let mask = target_align - 1;
      assert_eq!(address & mask, 0);
    }
    address as *mut Target
  }

  pub(super) trait ScopeData: Default + Sized {
    type Args: Sized;
    type Raw: Sized;

    #[inline(always)]
    fn construct(
      _buf: *mut Self::Raw,
      _args: &mut Self::Args,
      _isolate: &mut Isolate,
    ) {
      assert_eq!(size_of::<Self::Raw>(), 0);
    }

    #[inline(always)]
    fn destruct(raw: *mut Self::Raw) {
      if needs_drop::<Self::Raw>() {
        unsafe { drop_in_place(raw) }
      }
    }

    fn activate(
      raw: *mut Self::Raw,
      args: &mut Self::Args,
      _isolate: &mut Isolate,
      active_scope_data: &mut ActiveScopeData,
    ) -> Self;

    #[inline(always)]
    fn deactivate(
      _raw: *mut Self::Raw,
      previous: Self,
      isolate: &mut Isolate,
      active_scope_data: &mut ActiveScopeData,
    ) {
      replace(Self::get_mut(isolate, active_scope_data), previous);
    }

    fn get_mut<'a>(
      _isolate: &'a mut Isolate,
      active_scope_data: &'a mut ActiveScopeData,
    ) -> &'a mut Self;
  }

  #[derive(Debug, Default)]
  pub(super) struct ActiveScopeData {
    pub context: data::Context,
    pub handle_scope: data::HandleScope,
    pub escape_slot: data::EscapeSlot,
    pub try_catch: data::TryCatch,
  }

  struct ScopeStackFrame<D: ScopeData> {
    raw: D::Raw,
    previous_active: D,
    metadata: ScopeStackFrameMetadata,
  }

  struct ScopeStackFrameMetadata {
    cleanup_fn:
      unsafe fn(*mut Self, &mut Isolate, &mut ActiveScopeData) -> usize,
  }

  #[repr(transparent)]
  #[derive(Copy, Clone, Debug, Eq, PartialEq)]
  pub(super) struct ScopeCookie(u32);

  impl ScopeCookie {
    pub const NONE: Self = Self(0);
    pub const BORROWED: Self = Self(!0);

    // Methods for manipulating `ScopeStore::top_scope_cookie`.

    #[inline(always)]
    fn next(cell: &Cell<Self>) -> Self {
      let cur_cookie = cell.get();
      let next_cookie = Self(cur_cookie.0 + 1);
      cell.set(next_cookie);
      next_cookie
    }

    #[inline(always)]
    fn revert(cell: &Cell<Self>) -> Self {
      let cur_cookie = cell.get();
      assert_ne!(cur_cookie, Self::default());
      let old_cookie = Self(cur_cookie.0 - 1);
      cell.set(old_cookie);
      cur_cookie
    }

    #[inline(always)]
    fn borrow(cell: &Cell<Self>, scope_cookie: Self) {
      let cookie = cell.replace(Self::BORROWED);
      assert_eq!(cookie, scope_cookie);
    }

    #[inline(always)]
    fn unborrow(cell: &Cell<Self>, scope_cookie: Self) {
      let borrowed_cookie = cell.replace(scope_cookie);
      assert_eq!(borrowed_cookie, Self::BORROWED);
    }

    // Methods for manipulating `Scope::cookie`.

    #[inline(always)]
    fn set(&mut self, value: Self) {
      let none_cookie = replace(self, value);
      assert_eq!(none_cookie, Self::NONE)
    }

    #[inline(always)]
    fn reset(&mut self, top_scope_cookie: Self) {
      let cookie = replace(self, Self::NONE);
      assert_eq!(cookie, top_scope_cookie);
    }
  }

  impl Default for ScopeCookie {
    fn default() -> Self {
      Self::NONE
    }
  }
}

mod raw {
  use super::*;

  #[repr(C)]
  pub struct HandleScope([usize; 3]);

  #[derive(Clone, Copy)]
  #[repr(transparent)]
  pub struct Address(usize);

  #[repr(C)]
  pub struct TryCatch([usize; 6]);

  extern "C" {
    pub fn v8__Isolate__GetCurrentContext(
      isolate: *mut Isolate,
    ) -> *const Context;
    pub fn v8__Isolate__GetEnteredOrMicrotaskContext(
      isolate: *mut Isolate,
    ) -> *const Context;

    pub fn v8__HandleScope__CONSTRUCT(
      buf: *mut MaybeUninit<HandleScope>,
      isolate: *mut Isolate,
    );
    pub fn v8__HandleScope__DESTRUCT(this: *mut HandleScope);

    pub fn v8__Undefined(isolate: *mut Isolate) -> *const Primitive;
    pub fn v8__Local__New(
      isolate: *mut Isolate,
      other: *const Data,
    ) -> *const Data;

    pub fn v8__TryCatch__CONSTRUCT(
      buf: *mut MaybeUninit<TryCatch>,
      isolate: *mut Isolate,
    );
    pub fn v8__TryCatch__DESTRUCT(this: *mut TryCatch);
    pub fn v8__TryCatch__HasCaught(this: *const TryCatch) -> bool;
    pub fn v8__TryCatch__CanContinue(this: *const TryCatch) -> bool;
    pub fn v8__TryCatch__HasTerminated(this: *const TryCatch) -> bool;
    pub fn v8__TryCatch__Exception(this: *const TryCatch) -> *const Value;
    pub fn v8__TryCatch__StackTrace(
      this: *const TryCatch,
      context: *const Context,
    ) -> *const Value;
    pub fn v8__TryCatch__Message(this: *const TryCatch) -> *const Message;
    pub fn v8__TryCatch__Reset(this: *mut TryCatch);
    pub fn v8__TryCatch__ReThrow(this: *mut TryCatch) -> *const Value;
    pub fn v8__TryCatch__IsVerbose(this: *const TryCatch) -> bool;
    pub fn v8__TryCatch__SetVerbose(this: *mut TryCatch, value: bool);
    pub fn v8__TryCatch__SetCaptureMessage(this: *mut TryCatch, value: bool);
  }
}

mod raw_unused {
  use super::*;

  #[repr(C)]
  pub struct EscapableHandleScope([usize; 4]);

  extern "C" {
    fn v8__EscapableHandleScope__CONSTRUCT(
      buf: *mut MaybeUninit<EscapableHandleScope>,
      isolate: *mut Isolate,
    );
    fn v8__EscapableHandleScope__DESTRUCT(this: *mut EscapableHandleScope);
    fn v8__EscapableHandleScope__GetIsolate(
      this: &EscapableHandleScope,
    ) -> *mut Isolate;
    fn v8__EscapableHandleScope__Escape(
      this: *mut EscapableHandleScope,
      value: *const Data,
    ) -> *const Data;
  }
}
