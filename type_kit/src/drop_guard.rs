#[cfg(test)]
pub(crate) mod test_types {
    use std::{
        convert::Infallible,
        error::Error,
        fmt::{Display, Formatter},
    };

    use super::{Create, CreateResult, Destroy, DestroyResult};

    #[derive(Debug)]
    pub struct E;

    impl Display for E {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(f, "E")
        }
    }

    impl Error for E {}

    #[derive(Debug)]
    pub struct C;

    #[derive(Debug)]
    pub struct A(pub u32);

    impl Create for A {
        type Config<'a> = u32;
        type CreateError = E;

        fn create<'a, 'b>(config: Self::Config<'a>, _: Self::Context<'b>) -> CreateResult<Self> {
            Ok(Self(config))
        }
    }

    impl Destroy for A {
        type Context<'a> = &'a C;
        type DestroyError = Infallible;
        fn destroy<'a>(&mut self, _context: Self::Context<'a>) -> DestroyResult<Self> {
            Ok(())
        }
    }

    #[derive(Debug)]
    pub struct B(pub u32);

    impl Create for B {
        type Config<'a> = u32;
        type CreateError = E;

        fn create<'a, 'b>(config: Self::Config<'a>, _: Self::Context<'b>) -> CreateResult<Self> {
            Ok(Self(config))
        }
    }

    impl Destroy for B {
        type Context<'a> = ();
        type DestroyError = Infallible;

        fn destroy<'a>(&mut self, _context: Self::Context<'a>) -> DestroyResult<Self> {
            Ok(())
        }
    }

    #[derive(Debug)]
    pub struct FaillingCreate;

    impl Create for FaillingCreate {
        type Config<'a> = ();
        type CreateError = E;

        fn create<'a, 'b>(_: Self::Config<'a>, _: Self::Context<'b>) -> CreateResult<Self> {
            Err(E)
        }
    }

    impl Destroy for FaillingCreate {
        type Context<'a> = &'a C;
        type DestroyError = Infallible;
        fn destroy<'a>(&mut self, _: Self::Context<'a>) -> DestroyResult<Self> {
            Ok(())
        }
    }

    #[derive(Debug)]
    pub struct FaillingDestroy;

    impl Create for FaillingDestroy {
        type Config<'a> = ();
        type CreateError = Infallible;

        fn create<'a, 'b>(_: Self::Config<'a>, _: Self::Context<'b>) -> CreateResult<Self> {
            Ok(Self)
        }
    }

    impl Destroy for FaillingDestroy {
        type Context<'a> = &'a C;
        type DestroyError = E;
        fn destroy<'a>(&mut self, _: Self::Context<'a>) -> DestroyResult<Self> {
            Err(E)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_types::{FaillingCreate, FaillingDestroy, A, B, C, E};

    #[test]
    fn test_drop_guard_destroyed_before_drop() {
        let c = C {};
        let mut a = DropGuard::new(A(42));
        assert_eq!(a.0, 42);
        let _ = a.destroy(&c);
    }

    #[test]
    #[should_panic]
    #[cfg(debug_assertions)]
    fn test_drop_guard_not_destroyed_panic_on_drop_in_debug() {
        let _ = DropGuard::new(A(42));
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn test_drop_guard_not_destroyed_no_panic_on_drop_in_release() {
        let _ = DropGuard::new(A(42));
    }

    #[test]
    fn test_drop_into_blanket_impl() {
        let mut b: DropGuard<_> = B(42).into();
        assert_eq!(b.0, 42);
        let _ = b.finalize();
    }

    #[test]
    fn test_drop_create_destroy_blanket_impl() {
        let c = C {};
        let mut a = DropGuard::<A>::create(42, &c).unwrap();
        assert_eq!(a.0, 42);
        let _ = a.destroy(&c);
    }

    #[test]
    fn test_drop_initialize_and_finalize_blanket_impl() {
        let mut b = DropGuard::<B>::initialize(42).unwrap();
        assert_eq!(b.0, 42);
        let _ = b.finalize();
    }

    #[test]
    fn test_drop_create_and_destroy_collection() {
        let c = C {};
        let mut b: Vec<DropGuard<A>> = (0..4u32).create(&c).collect::<Result<_, _>>().unwrap();
        for (value, guard) in (0..4u32).zip(&b) {
            assert_eq!(guard.0, value);
        }
        let _ = b.iter_mut().destroy(&c);
    }

    #[test]
    fn test_drop_initialize_and_finalize_collection() {
        let mut b: Vec<DropGuard<B>> = (0..4u32).initialize().collect::<Result<_, _>>().unwrap();
        for (value, guard) in (0..4u32).zip(&b) {
            assert_eq!(guard.0, value);
        }
        let _ = b.iter_mut().finalize();
    }

    #[test]
    fn test_create_failure_returns_error() {
        let c = C {};
        let result = FaillingCreate::create((), &c);
        assert!(matches!(result, Err(E {})));
    }

    #[test]
    fn test_guard_create_failure_returns_inner_type_error() {
        let c = C {};
        let result = DropGuard::<FaillingCreate>::create((), &mut &c);
        assert!(matches!(result, Err(E {})));
    }

    #[test]
    fn test_destroy_failure_returns_error() {
        let c = C {};
        let mut failing = FaillingDestroy::create((), &mut &c).unwrap();
        assert!(matches!(failing.destroy(&c), Err(E {})));
    }

    #[test]
    fn test_guard_destroy_failure_returns_inner_type_error() {
        let c = C {};
        let mut failing = DropGuard::<FaillingDestroy>::create((), &mut &c).unwrap();
        assert!(matches!(
            failing.destroy(&c),
            Err(DropGuardError::DestroyError(E {}))
        ));
    }
}

use std::{
    any::type_name,
    error::Error,
    fmt::{Debug, Display, Formatter},
    ops::{Deref, DerefMut},
};

pub type CreateResult<T> = Result<T, <T as Create>::CreateError>;

pub trait Create: Destroy {
    type Config<'a>;
    type CreateError: Error;

    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self>;
}

pub type DestroyResult<T> = Result<(), <T as Destroy>::DestroyError>;

pub trait Destroy: Sized {
    type Context<'a>;
    type DestroyError: Error;
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self>;
}

pub trait Initialize: Create
where
    Self::Context<'static>: Default,
{
    #[inline]
    fn initialize<'a>(config: Self::Config<'a>) -> CreateResult<Self> {
        Self::create(config, Self::Context::default())
    }
}

pub trait Finalize: Destroy
where
    Self::Context<'static>: Default,
{
    #[inline]
    fn finalize(&mut self) -> DestroyResult<Self> {
        self.destroy(Self::Context::default())
    }
}

impl<T: Create> Initialize for T where T::Context<'static>: Default {}
impl<T: Destroy> Finalize for T where T::Context<'static>: Default {}

pub trait CreateCollection<I: Create>: Sized + IntoIterator
where
    for<'a> I::Context<'a>: Clone + Copy,
    for<'a> Self::Item: Into<I::Config<'a>>,
{
    #[inline]
    fn create<'a>(self, context: I::Context<'a>) -> impl Iterator<Item = CreateResult<I>> {
        self.into_iter()
            .map(move |config| I::create(config.into(), context))
    }
}

pub trait DestroyCollection<I: Destroy>: Sized + IntoIterator
where
    for<'a> I::Context<'a>: Clone + Copy,
    for<'a> Self::Item: DerefMut<Target = I>,
{
    #[inline]
    fn destroy<'a>(self, context: I::Context<'a>) -> DestroyResult<I> {
        self.into_iter()
            .try_for_each(|mut item| item.destroy(context))
    }
}

impl<T: Create, I: Sized + IntoIterator> CreateCollection<T> for I
where
    for<'a> T::Context<'a>: Clone + Copy,
    for<'a> I::Item: Into<T::Config<'a>>,
{
}

impl<T: Destroy, I: Sized + IntoIterator> DestroyCollection<T> for I
where
    for<'a> T::Context<'a>: Clone + Copy,
    for<'a> Self::Item: DerefMut<Target = T>,
{
}

pub trait InitializeCollection<I: Initialize>: Sized + IntoIterator
where
    for<'a> I::Context<'a>: Default,
    for<'a> Self::Item: Into<I::Config<'a>>,
{
    #[inline]
    fn initialize<'a>(self) -> impl Iterator<Item = CreateResult<I>> {
        self.into_iter()
            .map(move |config| I::create(config.into(), I::Context::default()))
    }
}

pub trait FinalizeCollection<I: Finalize>: Sized + IntoIterator
where
    for<'a> I::Context<'a>: Default,
    for<'a> Self::Item: DerefMut<Target = I>,
{
    #[inline]
    fn finalize<'a>(self) -> DestroyResult<I> {
        self.into_iter()
            .try_for_each(|mut item| item.destroy(I::Context::default()))
    }
}

impl<T: Initialize, I: Sized + IntoIterator> InitializeCollection<T> for I
where
    for<'a> T::Context<'a>: Default,
    for<'a> I::Item: Into<T::Config<'a>>,
{
}

impl<T: Finalize, I: Sized + IntoIterator> FinalizeCollection<T> for I
where
    for<'a> T::Context<'a>: Default,
    for<'a> Self::Item: DerefMut<Target = T>,
{
}

impl<T: Create> Create for Option<T> {
    type Config<'a> = T::Config<'a>;
    type CreateError = T::CreateError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        T::create(config, context).map(Some)
    }
}

impl<T: Destroy> Destroy for Option<T> {
    type Context<'a> = T::Context<'a>;
    type DestroyError = T::DestroyError;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        if let Some(mut inner) = self.take() {
            inner.destroy(context)?;
        }
        Ok(())
    }
}

pub struct VecDestoryError<T: Destroy> {
    _err_item: T,
    err: T::DestroyError,
}

// TODO: It is reasonable to require for Destory: Debug,
// as printing the type for destory failure could be common
impl<T: Destroy> Debug for VecDestoryError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VecDestoryError")
            .field("err", &self.err)
            .finish()
    }
}

impl<T: Destroy> Display for VecDestoryError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.err)
    }
}

impl<T: Destroy> Error for VecDestoryError<T> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.err.source()
    }
}

impl<T: Destroy> Destroy for Vec<T>
where
    for<'a> T::Context<'a>: Copy,
{
    type Context<'a> = T::Context<'a>;

    type DestroyError = VecDestoryError<T>;

    // TODO: Error handling in this case is only viable for aborting the application,
    // as some iterm for which .destory(..) returns error is kept in the collection
    // what are viable approaches for the end user to handle such situaion?
    // - inspect failing object and fix the issue? The failing object should be returned in the result
    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        if let Err((index, err)) = self
            .iter_mut()
            .enumerate()
            .rev()
            .try_for_each(|(index, item)| item.destroy(context).map_err(|err| (index, err)))
        {
            let err_item = self.swap_remove(index);
            self.truncate(index);
            Err(VecDestoryError {
                _err_item: err_item,
                err,
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Default)]
pub struct DropGuard<T: Destroy> {
    #[cfg(debug_assertions)]
    inner: Option<T>,
    #[cfg(not(debug_assertions))]
    inner: T,
}

impl<T: Destroy> DropGuard<T> {
    #[inline]
    pub fn new(inner: T) -> Self {
        #[cfg(debug_assertions)]
        let inner = Some(inner);
        Self { inner }
    }
}

impl<T: Create + Destroy> From<T> for DropGuard<T> {
    #[inline]
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T: Create + Destroy> Create for DropGuard<T> {
    type Config<'a> = T::Config<'a>;
    type CreateError = T::CreateError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        T::create(config, context).map(Self::new)
    }
}

pub enum DropGuardError<T: Error> {
    DestroyError(T),
    DoubleDestroy,
}

impl<T: Error> Debug for DropGuardError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DestroyError(error) => {
                write!(
                    f,
                    "DropGuard inner item destroy raised an error: {:?}",
                    error
                )
            }
            Self::DoubleDestroy => write!(f, "DropGuard inner resource was already destroyed"),
        }
    }
}

impl<T: Error> Display for DropGuardError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DestroyError(error) => {
                write!(f, "DropGuard inner item destroy raised an error: {}", error)
            }
            Self::DoubleDestroy => write!(f, "DropGuard inner resource was already destroyed"),
        }
    }
}

impl<T: Error> From<T> for DropGuardError<T> {
    #[inline]
    fn from(value: T) -> Self {
        Self::DestroyError(value)
    }
}

impl<T: Error> From<DropGuardError<DropGuardError<T>>> for DropGuardError<T> {
    #[inline]
    fn from(value: DropGuardError<DropGuardError<T>>) -> Self {
        match value {
            DropGuardError::DestroyError(DropGuardError::DestroyError(error)) => error.into(),
            _ => DropGuardError::DoubleDestroy,
        }
    }
}

impl<T: Error> Error for DropGuardError<T> {}

impl<T: Destroy> Destroy for DropGuard<T> {
    type Context<'a> = T::Context<'a>;
    type DestroyError = DropGuardError<T::DestroyError>;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        #[cfg(debug_assertions)]
        {
            if let Some(mut inner) = self.inner.take() {
                inner
                    .destroy(context)
                    .map_err(|err| DropGuardError::DestroyError(err))?;
                self.inner = None;
                Ok(())
            } else {
                Err(DropGuardError::DoubleDestroy)
            }
        }
        #[cfg(not(debug_assertions))]
        {
            self.inner
                .destroy(context)
                .map_err(|err| DropGuardError::DestroyError(err))?;
            Ok(())
        }
    }
}

impl<T: Destroy> Deref for DropGuard<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        #[cfg(debug_assertions)]
        let inner = self.inner.as_ref().unwrap();
        #[cfg(not(debug_assertions))]
        let inner = &self.inner;
        inner
    }
}

impl<T: Destroy> DerefMut for DropGuard<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        // WARNING:
        //
        // While this `DerefMut` implementation allows you to obtain a mutable reference
        // to the inner resource, **do not call `destroy()` directly on the inner resource**.
        //
        // Calling `destroy()` on the inner resource can lead to double-destruction when
        // `DropGuard` attempts to destroy the resource again, potentially causing undefined
        // behavior or resource leaks.
        //
        // Instead, always use `DropGuard::destroy()` to properly destroy the resource.
        // `DropGuard` ensures that the resource is destroyed only once and provides safety
        // checks in debug builds to catch misuse.

        #[cfg(debug_assertions)]
        let inner = self.inner.as_mut().unwrap();
        #[cfg(not(debug_assertions))]
        let inner = &mut self.inner;
        inner
    }
}

impl<T: Destroy> AsRef<T> for DropGuard<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T: Destroy> AsMut<T> for DropGuard<T> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        // WARNING:
        //
        // While this `AsMut` implementation allows you to obtain a mutable reference
        // to the inner resource, **do not call `destroy()` directly on the inner resource**.
        //
        // Calling `destroy()` on the inner resource can lead to double-destruction when
        // `DropGuard` attempts to destroy the resource again, potentially causing undefined
        // behavior or resource leaks.
        //
        // Instead, always use `DropGuard::destroy()` to properly destroy the resource.
        // `DropGuard` ensures that the resource is destroyed only once and provides safety
        // checks in debug builds to catch misuse.

        self
    }
}

impl<T: Destroy> Drop for DropGuard<T> {
    #[inline]
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        if self.inner.is_some() {
            panic!(
                "DropGuard<{}> inner resource was not destroyed before drop! \
                 Ensure DropGuard::destroy is called before it's dropped",
                &type_name::<T>(),
            )
        }
    }
}
