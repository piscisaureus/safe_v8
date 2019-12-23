use std::marker::PhantomData;
use std::mem::size_of;
use std::mem::take;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::ops::DerefMut;

// Note: the 's lifetime is there to ensure that after entering a scope once,
// the same scope object can't ever be entered again.

/// A trait for defining scoped objects.
pub unsafe trait Scoped<'s>
where
  Self: Sized,
{
  type Args;
  fn enter_scope(buf: &mut MaybeUninit<Self>, args: Self::Args) -> ();
}

/// A RAII scope wrapper object that will, when the `enter()` method is called,
/// initialize and activate the guarded object.
pub struct Scope<'s, S>(ScopeState<'s, S>)
where
  S: Scoped<'s>;

enum ScopeState<'s, S>
where
  S: Scoped<'s>,
{
  Empty,
  New(S::Args),
  Uninit(MaybeUninit<S>),
  Ready(Entered<'s, S>),
}

impl<'s, S> Scope<'s, S>
where
  S: Scoped<'s>,
{
  /// Create a new Scope object in unentered state.
  pub(crate) fn new(args: S::Args) -> Self {
    Self(ScopeState::New(args))
  }

  /// Initializes the guarded object and returns a mutable reference to it.
  /// A scope can only be entered once.
  pub fn enter(&'s mut self) -> &'s mut Entered<S> {
    assert_eq!(size_of::<S>(), size_of::<MaybeUninit<S>>());
    assert_eq!(size_of::<S>(), size_of::<Entered<S>>());

    use ScopeState::*;
    let state = &mut self.0;

    let args = match take(state) {
      New(f) => f,
      _ => unreachable!(),
    };

    *state = Uninit(MaybeUninit::uninit());
    let buf = match state {
      Uninit(b) => b,
      _ => unreachable!(),
    };

    S::enter_scope(buf, args);

    *state = match take(state) {
      Uninit(b) => Ready(unsafe { b.assume_init() }.into()),
      _ => unreachable!(),
    };

    match state {
      Ready(v) => v,
      _ => unreachable!(),
    }
  }
}

impl<'s, S> Default for ScopeState<'s, S>
where
  S: Scoped<'s>,
{
  fn default() -> Self {
    Self::Empty
  }
}

/// A wrapper around the an instantiated and entered scope object.
#[repr(transparent)]
pub struct Entered<'s, S>(S, PhantomData<&'s ()>);

impl<'s, S> From<S> for Entered<'s, S> {
  fn from(value: S) -> Self {
    Self(value, PhantomData)
  }
}

impl<'s, S> Deref for Entered<'s, S> {
  type Target = S;
  fn deref(&self) -> &S {
    unsafe { &*(self as *const _ as *const S) }
  }
}

impl<'s, S> DerefMut for Entered<'s, S> {
  fn deref_mut(&mut self) -> &mut S {
    unsafe { &mut *(self as *mut _ as *mut S) }
  }
}

impl<'s, S, T> AsRef<T> for Entered<'s, S>
where
  S: AsRef<T>,
{
  fn as_ref(&self) -> &T {
    self.deref().as_ref()
  }
}

impl<'s, S, T> AsMut<T> for Entered<'s, S>
where
  S: AsMut<T>,
{
  fn as_mut(&mut self) -> &mut T {
    self.deref_mut().as_mut()
  }
}

/// Trait to convert values to Entered<'s, T>.
pub trait AsEntered<'s, S> {
  fn entered(&mut self) -> &mut Entered<'s, S>;
}

// Scope hierarchy.
// TODO: ContextScope, TryCatch.
pub trait Implied<T> {}

impl Implied<crate::Isolate> for crate::Locker {}
impl Implied<crate::Isolate> for crate::FunctionCallbackInfo {}
impl Implied<crate::Isolate> for crate::PropertyCallbackInfo {}
impl Implied<crate::Isolate> for crate::HandleScope {}
impl Implied<crate::Isolate> for crate::EscapableHandleScope {}

impl Implied<crate::Locker> for crate::FunctionCallbackInfo {}
impl Implied<crate::Locker> for crate::PropertyCallbackInfo {}
impl Implied<crate::Locker> for crate::HandleScope {}
impl Implied<crate::Locker> for crate::EscapableHandleScope {}

impl Implied<crate::HandleScope> for crate::FunctionCallbackInfo {}
impl Implied<crate::HandleScope> for crate::PropertyCallbackInfo {}

// Above a EscapableHandleScope there must be HandleScope or where else would
// the escaped value go?
impl Implied<crate::EscapableHandleScope> for crate::HandleScope {}

impl<'s, S, P> AsEntered<'s, P> for S
where
  Self: Implied<P>, //+ AsMut<P>,
{
  fn entered(&mut self) -> &mut Entered<'s, S> {
    unsafe { &mut *(self.as_mut() as *mut _ as *mut Entered<'s, S>) }
  }
}

fn get<T>() -> T {
  unimplemented!()
}

fn tes() {
  let mut hs: Entered<'_, crate::HandleScope> = get();
  let mut iso = AsEntered::<crate::Isolate>::entered(&mut hs);
}

// These types should eventually be actual scope types too.
// For now hack them into the structure by implementing AsEntered on them.
pub trait ShouldBeScoped {}
impl ShouldBeScoped for crate::Isolate {}
impl ShouldBeScoped for crate::Locker {}

impl<'s, S, T> AsEntered<'s, S> for T
where
  T: ShouldBeScoped,
{
  fn entered(&mut self) -> &mut Entered<'s, S> {
    unsafe { &mut *(self as *mut _ as *mut Entered<'s, S>) }
  }
}
