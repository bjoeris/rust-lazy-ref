// This code is based on code from Jonathan Reem's rust-lazy library (https://github.com/reem/rust-lazy)

use std::ops::Deref;
use std::cell::UnsafeCell;
use std::rc::Rc;
use std::mem;
use std::fmt;

use self::Inner::{Evaluated, EvaluationInProgress, Unevaluated, Redirect};

/// A lazy, reference-counted value on the heap.
pub struct Lazy<T> (UnsafeCell<Rc<UnsafeCell<Inner<T>>>>);

impl<T> Lazy<T> {
    /// Create a `Lazy<T>` from a function that returns the value.
    ///
    /// You can construct Lazy's manually using this, but the lazy! macro
    /// is preferred.
    ///
    /// ```rust
    /// # use lazy_ref::*;
    /// let expensive:Lazy<u32> = Lazy::new(move || {
    ///   println!("Evaluated!");
    ///   value(7)
    /// });
    /// assert_eq!(*expensive, 7u32); // "Evaluated!" gets printed here.
    /// assert_eq!(*expensive, 7u32); // Nothing printed.
    /// ```
    pub fn new<F>(producer: F) -> Lazy<T>
    where F: FnOnce() -> LazyResult<T> + 'static {
        Lazy(UnsafeCell::new(Rc::new(UnsafeCell::new(Unevaluated(Producer::new(producer))))))
    }

    /// Create a new, evaluated, `Lazy<T>` from a value.
    pub fn evaluated(val: T) -> Lazy<T> {
        Lazy(UnsafeCell::new(Rc::new(UnsafeCell::new(Evaluated(val)))))
    }

    /// Force evaluation of a `Lazy<T>`.
    ///
    /// You do not usually need to call this explicitly, as derefing calls `force`
    ///
    /// Calling `force` on an evaluated `Lazy` does nothing.
    pub fn force(&self) {
        loop {
            match *self.inner() {
                Evaluated(_) => return,
                EvaluationInProgress => {
                    panic!("Lazy::force called recursively. (A Thunk tried to force itself while trying to force itself).")
                },
                Redirect(ref t) => {
                    self.redirect(t.clone());
                    continue;
                },
                Unevaluated(_) => ()
            };
            break;
        }

        match mem::replace(self.inner(), EvaluationInProgress) {
            Unevaluated(producer) => {
                *self.inner() = EvaluationInProgress;
                match producer.invoke() {
                    LazyResult::Value(x) =>
                        *self.inner() = Evaluated(x),
                    LazyResult::Redirect(t) => {
                        t.force();
                        *self.inner() = Redirect(t.clone());
                        self.redirect(t);
                    }
                }
            }
            _ => {
                unsafe { debug_unreachable!() }   
            }
        }
    }

    fn inner(&self) -> &mut Inner<T> {
        match *self {
            Lazy(ref cell) => unsafe {
                &mut *(**cell.get()).get()
            }
        }
    }

    fn rc(&self) -> &mut Rc<UnsafeCell<Inner<T>>> {
        match *self {
            Lazy(ref cell) => unsafe {
                &mut *cell.get()
            }
        }
    }

    fn redirect(&self, t: Lazy<T>) {
        *self.rc() = t.rc().clone();
    }
}

impl<T> Deref for Lazy<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.force();
        match *self.inner()  {
            Evaluated(ref val) => val,
            _ => unreachable!(),
        }
    }
}

impl<T> Clone for Lazy<T> {
    fn clone(&self) -> Lazy<T> {
        Lazy(UnsafeCell::new(self.rc().clone()))
    }
}

impl<T> fmt::Debug for Lazy<T>
    where T: fmt::Debug
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("Lazy")
            .field(self.inner())
            .finish()
    }
}


/// Type returned by functions defining a `Lazy<T>`.
///
/// This contains either the concrete `T` value, or the `Lazy<T>` it redirects to.
#[derive(Debug)]
pub enum LazyResult<T> {
    Value(T),
    Redirect(Lazy<T>)
}

struct Producer<T> {
    inner: Box<Invoke<T>>
}

impl<T> fmt::Debug for Producer<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Producer{{...}}")
    }
}

impl<T> Producer<T> {
    fn new<F: FnOnce() -> T + 'static>(f: F) -> Producer<T> {
        Producer {
            inner: Box::new(move || {
                f()
            }) as Box<Invoke<T>>
        }
    }

    fn invoke(self) -> T {
        self.inner.invoke()
    }
}

#[derive(Debug)]
enum Inner<T> {
    Evaluated(T),
    EvaluationInProgress,
    Unevaluated(Producer<LazyResult<T>>),
    Redirect(Lazy<T>),
}

#[doc(hidden)]
pub trait Invoke<T> {
    fn invoke(self: Box<Self>) -> T;
}

impl<T, F> Invoke<T> for F
    where F: FnOnce() -> T
{
    fn invoke(self: Box<F>) -> T {
        let f = *self;
        f()
    }
}

/// Construct a `Lazy<T>` from an expression returning a `LazyResult<T>`.
///
/// ```rust
/// #[macro_use] extern crate lazy_ref;
/// # use lazy_ref::*;
/// # fn main() {
/// let thunk: Lazy<u32> = lazy!{
///     println!("Evaluated!");
///     value(7)
/// };
/// assert_eq!(*thunk, 7);
/// # }
/// ```
#[macro_export]
macro_rules! lazy {
    ($($e: stmt);*) => {
        $crate::Lazy::new(move || { $($e);* })
    }
}

/// Construct a `Lazy<T>` from an expression returning a `T`.
///
/// ```rust
/// #[macro_use] extern crate lazy_ref;
/// # use lazy_ref::*;
/// # fn main() {
/// let thunk: Lazy<u32> = lazy_val!{
///     println!("Evaluated!");
///     7
/// };
/// assert_eq!(*thunk, 7);
/// # }
/// ```
#[macro_export]
macro_rules! lazy_val {
    ($($e: stmt);*) => {
        $crate::Lazy::new(move || { value({$($e);*}) })
    }
}


/// Construct a `Lazy<T>` from an expression returning a `Lazy<T>`.
///
/// ```rust
/// #[macro_use] extern crate lazy_ref;
/// # use lazy_ref::*;
/// # fn main() {
/// let thunk1: Lazy<u32> = lazy_val!{
///     println!("Evaluated 1!");
///     7
/// };
/// let thunk2: Lazy<u32> = lazy_redirect!{
///     println!("Evaluated 2!");
///     thunk1
/// };
/// assert_eq!(*thunk2, 7);
/// # }
/// ```
#[macro_export]
macro_rules! lazy_redirect {
    ($($e: stmt);*) => {
        $crate::Lazy::new(move || { redirect({$($e);*}) })
    }
}

/// Construct a `Lazy<T>` from a `T` value.
///
/// ```rust
/// #[macro_use] extern crate lazy_ref;
/// # use lazy_ref::*;
/// # fn main() {
/// let thunk: Lazy<u32> = strict(7);
/// assert_eq!(*thunk, 7);
/// # }
/// ```
pub fn strict<T>(v: T) -> Lazy<T> {
    Lazy::evaluated(v)
}

/// Construct a `LazyResult<T>` from a `Lazy<T>` value.
///
/// Use this in a `lazy!` expression to return the value of another `Lazy<T>`, whithout needing to deref and copy the value.
///
/// ```rust
/// #[macro_use] extern crate lazy_ref;
/// # use lazy_ref::*;
/// # fn main() {
/// let thunk1: Lazy<u32> = strict(7);
/// let thunk2: Lazy<u32> = lazy! {
///   redirect(thunk1)
/// };
/// assert_eq!(*thunk2, 7);
/// # }
/// ```
pub fn redirect<T>(t: Lazy<T>) -> LazyResult<T> {
    LazyResult::Redirect(t)
}

/// Construct a `LazyResult<T>` from a `T` value.
///
/// Use this in a `lazy!` expression to return a concrete value.
////
//// To return a value from another lazy expression, you should probably use `redirect` instead.
///
/// ```rust
/// #[macro_use] extern crate lazy_ref;
/// # use lazy_ref::*;
/// # fn main() {
/// let thunk: Lazy<u32> = lazy! {
///   value(7)
/// };
/// assert_eq!(*thunk, 7);
/// # }
/// ```
pub fn value<T>(v: T) -> LazyResult<T> {
    LazyResult::Value(v)
}

