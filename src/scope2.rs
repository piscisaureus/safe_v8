use std::any::Any;
use std::cell::Cell;
use std::marker::PhantomData;
use std::mem::replace;
use std::mem::size_of;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr;
use std::ptr::NonNull;

use crate::isolate::IsolateAnnex;
use crate::undefined;
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
    fn from_scope_state(scope_state: &mut NonNull<data::ScopeState>) -> Self {
      assert_eq!(size_of::<Self>(), size_of::<NonNull<data::ScopeState>>());
      let mut scope_state = *scope_state;
      unsafe { ptr::read(&mut scope_state as *mut _ as *mut Self) }
    }
    fn get_scope_state(&mut self) -> &mut NonNull<data::ScopeState> {
      assert_eq!(size_of::<Self>(), size_of::<NonNull<data::ScopeState>>());
      unsafe { &mut *(self as *mut _ as *mut NonNull<_>) }
    }
  }

  unsafe impl<S: Scope> Scope for &S {
    fn from_scope_state(scope_state: &mut NonNull<data::ScopeState>) -> Self {
      assert_eq!(size_of::<S>(), size_of::<NonNull<data::ScopeState>>());
      unsafe { &*(scope_state as *const _ as *const Self) }
    }
    fn get_scope_state(&mut self) -> &mut NonNull<data::ScopeState> {
      assert_eq!(size_of::<S>(), size_of::<NonNull<data::ScopeState>>());
      unsafe { &mut *(self as *mut Self as *mut NonNull<_>) }
    }
  }

  unsafe impl<S: Scope> Scope for &mut S {
    fn from_scope_state(scope_state: &mut NonNull<data::ScopeState>) -> Self {
      assert_eq!(size_of::<S>(), size_of::<NonNull<data::ScopeState>>());
      unsafe { &mut *(scope_state as *mut _ as *mut Self) }
    }
    fn get_scope_state(&mut self) -> &mut NonNull<data::ScopeState> {
      (**self).get_scope_state()
    }
  }

  pub struct ContextScope<'s, P> {
    scope_state: NonNull<data::ScopeState>,
    _phantom: PhantomData<&'s mut P>,
  }

  unsafe impl<'s, P> Scope for ContextScope<'s, P> {}

  impl<'s, P: Scope> Deref for ContextScope<'s, P> {
    type Target = P;
    fn deref(mut self: &Self) -> &Self::Target {
      Scope::from_scope_state(self.get_scope_state())
    }
  }

  impl<'s, P: Scope> DerefMut for ContextScope<'s, P> {
    fn deref_mut(&mut self) -> &mut Self::Target {
      Scope::from_scope_state(self.get_scope_state())
    }
  }

  impl<'s, P> Drop for ContextScope<'s, P> {
    fn drop(&mut self) {}
  }

  impl<'s, P> ContextScope<'s, P>
  where
    P: NewContextScopeParam<'s>,
  {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(mut param: P, context: Local<Context>) -> P::NewScope {
      let old_state = data::ScopeState::get_mut(param.get_scope_state());
      let isolate = old_state.get_isolate_mut();
      let mut new_state = data::ScopeState::new_context_scope(isolate, context);
      Scope::from_scope_state(&mut new_state)
    }
  }

  pub trait NewContextScopeParam<'s>: Scope {
    type NewScope: Scope;
  }

  impl<'s, 'p: 's, P: Scope> NewContextScopeParam<'s>
    for &'s mut ContextScope<'p, P>
  {
    type NewScope = ContextScope<'s, P>;
  }

  impl<'s, 'p: 's, P> NewContextScopeParam<'s> for &'s mut HandleScope<'p, P> {
    type NewScope = ContextScope<'s, HandleScope<'p>>;
  }

  impl<'s, 'p: 's, 'e: 'p> NewContextScopeParam<'s>
    for &'s mut EscapableHandleScope<'p, 'e>
  {
    type NewScope = ContextScope<'s, EscapableHandleScope<'p, 'e>>;
  }

  pub struct HandleScope<'s, P = Context> {
    scope_state: NonNull<data::ScopeState>,
    _phantom: PhantomData<&'s mut P>,
  }

  impl<'s> HandleScope<'s, ()> {
    pub fn add_local<T>(&'_ mut self) -> Local<'s, T>
    where
      Local<'s, T>: Into<Local<'s, Context>>,
    {
      unimplemented!()
    }
  }

  impl<'s> HandleScope<'s> {
    pub fn add_local<T>(&'_ mut self) -> Local<'s, T> {
      unimplemented!()
    }
  }

  unsafe impl<'s, P> Scope for HandleScope<'s, P> {}

  impl<'s, P> Drop for HandleScope<'s, P> {
    fn drop(&mut self) {}
  }

  impl<'s> HandleScope<'s> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<P>(param: P) -> P::NewScope
    where
      P: NewHandleScopeParam<'s>,
    {
      let isolate = param.get_isolate_mut();
      let mut new_state = data::ScopeState::new_handle_scope(isolate);
      Scope::from_scope_state(&mut new_state)
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
    &'s mut P: Scope + NewHandleScopeParam<'s>,
  {
    type NewScope = <&'s mut P as NewHandleScopeParam<'s>>::NewScope;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self.get_scope_state()).get_isolate_mut()
    }
  }

  impl<'s, 'p: 's> NewHandleScopeParam<'s> for &'s mut HandleScope<'p, ()> {
    type NewScope = HandleScope<'s, ()>;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self.get_scope_state()).get_isolate_mut()
    }
  }

  impl<'s, 'p: 's> NewHandleScopeParam<'s> for &'s mut HandleScope<'p> {
    type NewScope = HandleScope<'s>;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self.get_scope_state()).get_isolate_mut()
    }
  }

  impl<'s, 'p: 's, 'e: 'p> NewHandleScopeParam<'s>
    for &'s mut EscapableHandleScope<'p, 'e>
  {
    type NewScope = EscapableHandleScope<'s, 'e>;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self.get_scope_state()).get_isolate_mut()
    }
  }

  /************* WIP: HERE *************/

  pub struct EscapableHandleScope<'s, 'e: 's> {
    scope_state: NonNull<data::ScopeState>,
    _phantom: PhantomData<(&'s mut raw::HandleScope, &'e raw::EscapeSlot)>,
  }

  unsafe impl<'s, 'e: 's> Scope for EscapableHandleScope<'s, 'e> {}

  impl<'s, 'e: 's> Deref for EscapableHandleScope<'s, 'e> {
    type Target = HandleScope<'s>;
    fn deref(mut self: &Self) -> &Self::Target {
      Scope::from_scope_state(self.get_scope_state())
    }
  }

  impl<'s, 'e: 's> DerefMut for EscapableHandleScope<'s, 'e> {
    fn deref_mut(&mut self) -> &mut Self::Target {
      Scope::from_scope_state(self.get_scope_state())
    }
  }

  impl<'s, 'e: 's> Drop for EscapableHandleScope<'s, 'e> {
    fn drop(&mut self) {}
  }

  impl<'s, 'e: 's> EscapableHandleScope<'s, 'e> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new<P>(param: P) -> P::NewScope
    where
      P: NewEscapableHandleScopeParam<'s, 'e>,
    {
      let isolate = param.get_isolate_mut();
      let mut new_state = data::ScopeState::new_escapable_handle_scope(isolate);
      Scope::from_scope_state(&mut new_state)
    }
  }

  pub trait NewEscapableHandleScopeParam<'s, 'e: 's>: Scope {
    type NewScope: Scope;
    fn get_isolate_mut(self) -> &'s mut Isolate;
  }

  impl<'s, 'p: 's, 'e: 'p, P> NewEscapableHandleScopeParam<'s, 'e>
    for &'s mut ContextScope<'p, P>
  where
    &'s mut P: Scope + NewEscapableHandleScopeParam<'s, 'e>,
  {
    type NewScope =
      <&'s mut P as NewEscapableHandleScopeParam<'s, 'e>>::NewScope;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self.get_scope_state()).get_isolate_mut()
    }
  }

  impl<'s, 'p: 's> NewEscapableHandleScopeParam<'s, 'p>
    for &'s mut HandleScope<'p>
  {
    type NewScope = EscapableHandleScope<'s, 'p>;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self.get_scope_state()).get_isolate_mut()
    }
  }

  impl<'s, 'p: 's, 'e: 'p> NewEscapableHandleScopeParam<'s, 'p>
    for &'s mut EscapableHandleScope<'p, 'e>
  {
    type NewScope = EscapableHandleScope<'s, 'p>;
    fn get_isolate_mut(self) -> &'s mut Isolate {
      data::ScopeState::get_mut(self.get_scope_state()).get_isolate_mut()
    }
  }
}

pub(crate) mod data {
  use super::*;

  pub struct ScopeState {
    pub(super) isolate: NonNull<Isolate>,
    pub(super) context: Option<NonNull<Context>>,
    pub(super) escape_slot: Option<NonNull<raw::EscapeSlot>>,
    parent: Option<NonNull<ScopeState>>,
    data: ScopeData,
  }

  impl ScopeState {
    // TODO(piscisaureus): use something more efficient than a separate heap
    // allocation for every scope.
    fn new_with<F>(isolate: &mut Isolate, init_fn: F) -> NonNull<Self>
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
        data: Default::default(),
      };
      let mut state_box = Box::new(state);
      (init_fn)(&mut *state_box);
      let state_ptr = Box::into_raw(state_box);
      let state_nn = unsafe { NonNull::new_unchecked(state_ptr) };
      isolate.set_current_scope(Some(state_nn));
      state_nn
    }

    pub(super) fn new_context_scope(
      isolate: &mut Isolate,
      context: Local<Context>,
    ) -> NonNull<Self> {
      Self::new_with(isolate, |state| {
        state.context =
          NonNull::new(&*context as *const Context as *mut Context);
      })
    }

    pub(super) fn new_handle_scope(isolate: &mut Isolate) -> NonNull<Self> {
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
    ) -> NonNull<Self> {
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

    fn check_current(self_nn: NonNull<Self>) {
      let current_scope_nn =
        unsafe { self_nn.as_ref().get_isolate().get_current_scope() };
      assert_eq!(Some(self_nn), current_scope_nn);
    }

    pub(super) fn get<'a>(self_nn: &'a NonNull<Self>) -> &'a Self {
      Self::check_current(*self_nn);
      unsafe { self_nn.as_ref() }
    }

    pub(super) fn get_mut<'a>(self_nn: &'a mut NonNull<Self>) -> &'a mut Self {
      Self::check_current(*self_nn);
      unsafe { self_nn.as_mut() }
    }

    pub(super) fn get_isolate(&self) -> &Isolate {
      unsafe { self.isolate.as_ref() }
    }

    pub(super) fn get_isolate_mut(&mut self) -> &mut Isolate {
      unsafe { self.isolate.as_mut() }
    }
  }

  pub(super) enum ScopeData {
    None,
    Context,
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

  #[repr(C)]
  #[derive(Eq, PartialEq)]
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
      assert!(self.isolate_.is_null());
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

  pub(super) struct EscapeSlot(Option<NonNull<raw::Address>>);

  impl EscapeSlot {
    fn uninit() -> Self {
      Self(None)
    }

    fn init(&mut self, isolate: *mut Isolate) {
      unsafe {
        let undefined = v8__Undefined(isolate);
        let local = v8__Local__New(isolate, undefined as *const _);
        let address = &*local as *const _ as *mut raw::Address;
        let slot = NonNull::new_unchecked(address);
        let none = self.0.replace(slot.cast());
        debug_assert!(none.is_none());
      }
    }
  }

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
      // Note: the `EscapeSlot` must be initialized *before* the `HandleScope`.
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

  fn test_scope(owned_isolate: &mut OwnedIsolate) {
    let mut h = HandleScope::new(owned_isolate);
    let context = h.add_local::<Context>();
    let mut h = HandleScope::new(&mut h);
    let mut h = ContextScope::new(&mut h, context);
    let mut h = ContextScope::new(&mut h, context);
    let mut h = HandleScope::new(&mut h);
    let l1 = h.add_local::<Value>();
    let l2 = h.add_local::<Value>();
    let mut hx = EscapableHandleScope::new(&mut h);
    l1;
    l2;
    let _lr = {
      let mut h = HandleScope::new(&mut hx);
      let l3 = h.add_local::<Value>();
      l3
    };
    let l4 = hx.add_local::<Value>();
    //_lr;
    //let mut h = ContextScope::new(&mut h, context);

    // /hx;
  }
}
