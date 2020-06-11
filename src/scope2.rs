use std::convert::Into;
use std::marker::PhantomData;
use std::mem::replace;
use std::mem::size_of;
use std::mem::transmute_copy;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr::null;
use std::ptr::NonNull;

use crate::Context;
use crate::Data;
use crate::Isolate;
use crate::Local;
use crate::Message;
use crate::OwnedIsolate;
use crate::Primitive;
use crate::TryCatch;
use crate::Value;

#[doc(inline)]
pub use reference::{ContextScope, EscapableHandleScope, HandleScope};

pub(self) mod reference {
  use super::*;

  pub unsafe trait Scope: Sized {
    fn cast_ref<S: Scope>(&self) -> &S {
      unsafe { &*(self as *const _ as *const S) }
    }
    fn cast_mut<S: Scope>(&mut self) -> &mut S {
      unsafe { &mut *(self as *mut _ as *mut S) }
    }
  }

  #[derive(Debug)]
  pub struct ContextScope<'s, P> {
    scope_state: NonNull<data::ScopeState>,
    _phantom: PhantomData<&'s mut P>,
  }

  unsafe impl<'s, P> Scope for ContextScope<'s, P> {}

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
      data::ScopeState::get_mut(self).notify_scope_dropped();
    }
  }

  impl<'s, P> ContextScope<'s, P>
  where
    P: NewContextScopeParam<'s>,
  {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(param: P, context: Local<Context>) -> P::NewScope {
      let isolate = param.get_isolate_mut();
      let state = data::ScopeState::new_context_scope(isolate, context);
      state.as_scope()
    }
  }

  pub trait NewContextScopeParam<'s> {
    type NewScope: Scope;
    fn get_isolate_mut(self) -> &'s mut Isolate;
  }

  impl<'s, 'p: 's, P: Scope> NewContextScopeParam<'s>
    for &'s mut ContextScope<'p, P>
  {
    type NewScope = ContextScope<'s, P>;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self).get_isolate_mut()
    }
  }

  impl<'s, 'p: 's, P> NewContextScopeParam<'s> for &'s mut HandleScope<'p, P> {
    type NewScope = ContextScope<'s, HandleScope<'p>>;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self).get_isolate_mut()
    }
  }

  impl<'s, 'p: 's, 'e: 'p> NewContextScopeParam<'s>
    for &'s mut EscapableHandleScope<'p, 'e>
  {
    type NewScope = EscapableHandleScope<'s, 'e>;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self).get_isolate_mut()
    }
  }

  #[derive(Debug)]
  pub struct HandleScope<'s, P = Context> {
    scope_state: NonNull<data::ScopeState>,
    _phantom: PhantomData<&'s mut P>,
  }

  impl<'s> HandleScope<'s, ()> {
    pub unsafe fn cast_local<F, T>(&mut self, f: F) -> Option<Local<'s, T>>
    where
      F: FnOnce(&mut Self) -> *const T,
      Local<'s, T>: Into<Local<'s, Context>>,
    {
      // `ScopeState::get_mut()` is called here for its side effects: it checks
      // that `self` is actually the active scope, and if necessary it will
      // drop (escapable) handle scopes of which the drop call has been
      // deferred.
      data::ScopeState::get_mut(self);
      Local::from_raw(f(self))
    }
  }

  impl<'s> HandleScope<'s> {
    pub unsafe fn cast_local<F, T>(&mut self, f: F) -> Option<Local<'s, T>>
    where
      F: FnOnce(&mut Self) -> *const T,
    {
      // `ScopeState::get_mut()` is called here for its side effects: it checks
      // that `self` is actually the active scope, and if necessary it will
      // drop (escapable) handle scopes of which the drop call has been
      // deferred.
      data::ScopeState::get_mut(self);
      Local::from_raw(f(self))
    }
  }

  unsafe impl<'s, P> Scope for HandleScope<'s, P> {}

  impl<'s, P> Deref for HandleScope<'s, P> {
    type Target = Isolate;
    fn deref(&self) -> &Self::Target {
      data::ScopeState::get(self).get_isolate()
    }
  }

  impl<'s, P> DerefMut for HandleScope<'s, P> {
    fn deref_mut(&mut self) -> &mut Self::Target {
      data::ScopeState::get_mut(self).get_isolate_mut()
    }
  }

  impl<'s, P> Drop for HandleScope<'s, P> {
    fn drop(&mut self) {
      data::ScopeState::get_mut(self).notify_scope_dropped();
    }
  }

  impl<'s> HandleScope<'s> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<P>(param: P) -> P::NewScope
    where
      P: NewHandleScopeParam<'s>,
    {
      let isolate = param.get_isolate_mut();
      let state = data::ScopeState::new_handle_scope(isolate);
      state.as_scope()
    }
  }

  pub trait NewHandleScopeParam<'s> {
    type NewScope: Scope;
    fn get_isolate_mut(self) -> &'s mut Isolate;
  }

  impl<'s> NewHandleScopeParam<'s> for &'s mut OwnedIsolate {
    type NewScope = HandleScope<'s, ()>;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      &mut *self
    }
  }

  impl<'s, 'p: 's, P> NewHandleScopeParam<'s> for &'s mut ContextScope<'p, P>
  where
    P: Scope,
    &'s mut P: NewHandleScopeParam<'s>,
  {
    type NewScope = <&'s mut P as NewHandleScopeParam<'s>>::NewScope;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self).get_isolate_mut()
    }
  }

  impl<'s, 'p: 's> NewHandleScopeParam<'s> for &'s mut HandleScope<'p, ()> {
    type NewScope = HandleScope<'s, ()>;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self).get_isolate_mut()
    }
  }

  impl<'s, 'p: 's> NewHandleScopeParam<'s> for &'s mut HandleScope<'p> {
    type NewScope = HandleScope<'s>;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self).get_isolate_mut()
    }
  }

  impl<'s, 'p: 's, 'e: 'p> NewHandleScopeParam<'s>
    for &'s mut EscapableHandleScope<'p, 'e>
  {
    type NewScope = EscapableHandleScope<'s, 'e>;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self).get_isolate_mut()
    }
  }

  #[derive(Debug)]
  pub struct EscapableHandleScope<'s, 'e: 's> {
    scope_state: NonNull<data::ScopeState>,
    _phantom: PhantomData<(&'s mut raw::HandleScope, &'e raw::EscapeSlot)>,
  }

  unsafe impl<'s, 'e: 's> Scope for EscapableHandleScope<'s, 'e> {}

  impl<'s, 'e: 's> Deref for EscapableHandleScope<'s, 'e> {
    type Target = HandleScope<'s>;
    fn deref(&self) -> &Self::Target {
      self.cast_ref()
    }
  }

  impl<'s, 'e: 's> DerefMut for EscapableHandleScope<'s, 'e> {
    fn deref_mut(&mut self) -> &mut Self::Target {
      self.cast_mut()
    }
  }

  impl<'s, 'e: 's> Drop for EscapableHandleScope<'s, 'e> {
    fn drop(&mut self) {
      data::ScopeState::get_mut(self).notify_scope_dropped();
    }
  }

  impl<'s, 'e: 's> EscapableHandleScope<'s, 'e> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<P>(param: P) -> P::NewScope
    where
      P: NewEscapableHandleScopeParam<'s, 'e>,
    {
      let isolate = param.get_isolate_mut();
      let state = data::ScopeState::new_escapable_handle_scope(isolate);
      state.as_scope()
    }
  }

  pub trait NewEscapableHandleScopeParam<'s, 'e: 's> {
    type NewScope: Scope;
    fn get_isolate_mut(self) -> &'s mut Isolate;
  }

  impl<'s, 'p: 's, 'e: 'p, P> NewEscapableHandleScopeParam<'s, 'e>
    for &'s mut ContextScope<'p, P>
  where
    &'s mut P: NewEscapableHandleScopeParam<'s, 'e>,
  {
    type NewScope =
      <&'s mut P as NewEscapableHandleScopeParam<'s, 'e>>::NewScope;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self).get_isolate_mut()
    }
  }

  impl<'s, 'p: 's> NewEscapableHandleScopeParam<'s, 'p>
    for &'s mut HandleScope<'p>
  {
    type NewScope = EscapableHandleScope<'s, 'p>;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self).get_isolate_mut()
    }
  }

  impl<'s, 'p: 's, 'e: 'p> NewEscapableHandleScopeParam<'s, 'p>
    for &'s mut EscapableHandleScope<'p, 'e>
  {
    type NewScope = EscapableHandleScope<'s, 'p>;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self).get_isolate_mut()
    }
  }
}

pub(crate) mod data {
  use super::*;

  #[derive(Debug)]
  pub struct ScopeState {
    pub(super) isolate: NonNull<Isolate>,
    pub(super) context: Option<NonNull<Context>>,
    pub(super) escape_slot: Option<NonNull<raw::EscapeSlot>>,
    parent: Option<NonNull<ScopeState>>,
    deferred_drop: bool,
    data: ScopeData,
  }

  impl ScopeState {
    pub(super) fn new_context_scope<'s>(
      isolate: &'s mut Isolate,
      context: Local<'s, Context>,
    ) -> &'s mut Self {
      Self::new_with(isolate, move |state| {
        state.context = NonNull::new(&*context as *const _ as *mut Context);
        state.data = ScopeData::ContextScope(raw::ContextScope::uninit());
        match &mut state.data {
          ScopeData::ContextScope(raw) => raw.init(&*context),
          _ => unreachable!(),
        }
      })
    }

    pub(super) fn new_handle_scope(isolate: &mut Isolate) -> &mut Self {
      Self::new_with(isolate, |state| {
        state.data = ScopeData::HandleScope(raw::HandleScope::uninit());
        match &mut state.data {
          ScopeData::HandleScope(raw) => raw.init(state.isolate.as_ptr()),
          _ => unreachable!(),
        }
      })
    }

    pub(super) fn new_escapable_handle_scope(
      isolate: &mut Isolate,
    ) -> &mut Self {
      Self::new_with(isolate, |state| {
        state.data =
          ScopeData::EscapableHandleScope(raw::EscapableHandleScope::uninit());
        match &mut state.data {
          ScopeData::EscapableHandleScope(raw) => {
            state.escape_slot = raw.init(state.isolate.as_ptr());
          }
          _ => unreachable!(),
        }
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
        .and_then(|p| p.context)
        .map(NonNull::from);
      let escape_slot = parent
        .as_mut()
        .map(|p| unsafe { p.as_mut() })
        .and_then(|p| p.escape_slot)
        .map(NonNull::from);
      let state = Self {
        isolate: isolate_nn,
        parent,
        context,
        escape_slot,
        deferred_drop: false,
        data: ScopeData::default(),
      };
      let mut state_box = Box::new(state);
      (init_fn)(&mut *state_box);
      let state_ptr = Box::into_raw(state_box);
      isolate.set_current_scope(NonNull::new(state_ptr));
      unsafe { &mut *state_ptr }
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
      // This function is called when the `reference::Scope` object is dropped.
      // With regard to (escapable) handle scopes: the Rust borrow checker
      // allows these to be dropped before all the local handles that were
      // created inside the HandleScope have gone out of scope. In order to
      // avoid turning these locals into invalid references the HandleScope is
      // kept alive for now -- it'll be actually dropped when the user touches
      // the HandleScope's parent scope.
      match &self.data {
        ScopeData::HandleScope(_) | ScopeData::EscapableHandleScope(_) => {
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

    pub(super) fn as_scope<S: reference::Scope>(&mut self) -> S {
      assert_eq!(size_of::<&mut Self>(), size_of::<S>());
      let self_nn = NonNull::from(self);
      unsafe { transmute_copy(&self_nn) }
    }

    pub(super) fn get<S: reference::Scope>(scope: &S) -> &Self {
      let self_nn = unsafe { *(scope as *const _ as *const NonNull<Self>) };
      Self::touch(self_nn);
      unsafe { &*self_nn.as_ptr() }
    }

    pub(super) fn get_mut<S: reference::Scope>(scope: &mut S) -> &mut Self {
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

    fn touch(self_nn: NonNull<ScopeState>) {
      loop {
        let current_scope = unsafe { self_nn.as_ref() }
          .get_isolate()
          .get_current_scope();
        match current_scope {
          Some(current_scope_nn) if current_scope_nn == self_nn => break,
          Some(mut current_scope_nn)
            if unsafe { current_scope_nn.as_ref().deferred_drop } =>
          unsafe { current_scope_nn.as_mut().drop() }
          _ => panic!("an attempt has been made to use an inactive scope"),
        }
      }
    }
  }

  #[derive(Debug)]
  pub(super) enum ScopeData {
    None,
    ContextScope(raw::ContextScope),
    HandleScope(raw::HandleScope),
    EscapableHandleScope(raw::EscapableHandleScope),
  }

  impl Default for ScopeData {
    fn default() -> Self {
      Self::None
    }
  }
}

mod raw {
  use super::*;

  #[derive(Clone, Copy)]
  #[repr(transparent)]
  pub(super) struct Address(usize);

  #[derive(Debug, Eq, PartialEq)]
  pub(super) struct ContextScope {
    entered_context: *const Context,
  }

  impl ContextScope {
    pub(super) fn uninit() -> Self {
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
  #[derive(Debug, Eq, PartialEq)]
  pub(super) struct HandleScope {
    isolate_: *mut Isolate,
    prev_next_: *mut Address,
    prev_limit_: *mut Address,
  }

  impl HandleScope {
    pub(super) fn uninit() -> Self {
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
    fn uninit() -> Self {
      Self(None)
    }

    fn init(&mut self, isolate: *mut Isolate) {
      unsafe {
        let undefined = v8__Undefined(isolate) as *const Data;
        let local = v8__Local__New(isolate, undefined);
        let address = &*local as *const _ as *mut raw::Address;
        let slot = NonNull::new_unchecked(address);
        let uninit_slot = self.0.replace(slot);
        debug_assert!(uninit_slot.is_none());
      }
    }
  }

  #[derive(Debug)]
  pub(super) struct EscapableHandleScope {
    handle_scope: raw::HandleScope,
    escape_slot: raw::EscapeSlot,
  }

  impl EscapableHandleScope {
    pub(super) fn uninit() -> Self {
      Self {
        handle_scope: HandleScope::uninit(),
        escape_slot: EscapeSlot::uninit(),
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

    pub(super) fn v8__Context__GetIsolate(this: *const Context)
      -> *mut Isolate;
    pub(super) fn v8__Context__Enter(this: *const Context);
    pub(super) fn v8__Context__Exit(this: *const Context);

    pub(super) fn v8__HandleScope__CONSTRUCT(
      buf: *mut MaybeUninit<HandleScope>,
      isolate: *mut Isolate,
    );
    pub(super) fn v8__HandleScope__DESTRUCT(this: *mut HandleScope);

    pub(super) fn v8__Undefined(isolate: *mut Isolate) -> *const Primitive;
    pub(super) fn v8__Local__New(
      isolate: *mut Isolate,
      other: *const Data,
    ) -> *const Data;

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

#[cfg(test)]
mod tests {
  use super::*;

  use crate::support::char;
  use crate::support::int;
  use crate::NewStringType;
  use crate::ObjectTemplate;
  use crate::String;
  use crate::Value;

  use std::convert::TryInto;
  use std::ptr::null;

  extern "C" {
    fn v8__Context__New(
      isolate: *mut Isolate,
      templ: *const ObjectTemplate,
      global_object: *const Value,
    ) -> *const Context;

    fn v8__String__NewFromUtf8(
      isolate: *mut Isolate,
      data: *const char,
      new_type: NewStringType,
      length: int,
    ) -> *const String;
  }

  impl Context {
    fn new2<'s>(scope: &mut HandleScope<'s, ()>) -> Local<'s, Context> {
      // TODO: optional arguments;
      unsafe {
        scope.cast_local(|scope| v8__Context__New(&mut **scope, null(), null()))
      }
      .unwrap()
    }
  }

  impl String {
    pub fn new2<'s>(
      scope: &mut HandleScope<'s>,
      buffer: &[u8],
    ) -> Option<Local<'s, String>> {
      let buffer_len = buffer.len().try_into().ok()?;
      unsafe {
        scope.cast_local(|scope| {
          v8__String__NewFromUtf8(
            &mut **scope,
            buffer.as_ptr() as *const char,
            Default::default(),
            buffer_len,
          )
        })
      }
    }
  }

  #[test]
  fn test_scopes() {
    crate::V8::initialize_platform(crate::new_default_platform().unwrap());
    crate::V8::initialize();
    let mut isolate = Isolate::new(Default::default());
    let mut h = HandleScope::new(&mut isolate);
    let context = Context::new2(&mut h);
    let mut h = HandleScope::new(&mut h);
    let mut h = ContextScope::new(&mut h, context);
    let mut h = ContextScope::new(&mut h, context);
    let mut h = HandleScope::new(&mut h);
    let l1 = String::new2(&mut h, b"AAA").unwrap();
    let l2 = String::new2(&mut h, b"BBB").unwrap();
    let mut hx = EscapableHandleScope::new(&mut h);
    let _ = l1;
    let _ = l2;
    let lr = {
      let mut h = HandleScope::new(&mut hx);
      let l3 = String::new2(&mut h, b"CCC").unwrap();
      let _l4 = fn_with_scope_arg(&mut h);
      l3
    };
    let _ = lr == l1;
    let _l5 = String::new2(&mut hx, b"EEE").unwrap();
  }

  fn fn_with_scope_arg<'a>(scope: &mut HandleScope<'a>) -> Local<'a, Value> {
    String::new2(scope, b"hey hey").unwrap().into()
  }
}
