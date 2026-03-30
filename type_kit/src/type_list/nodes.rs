use std::{
    any::type_name,
    convert::Infallible,
    error::Error,
    fmt::{Debug, Display, Formatter},
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use crate::{Contains, Create, CreateResult, Destroy, DestroyResult, Here, Marker, There};

/// Type acting as zero sized terminator of a type-level list.
/// The type parameter `T` is added so that additional properties of tyepe-level list
/// can be defined by the user. e.g. user defined trait ListA can have
/// blanket implementation defined for type list types which all nested types implement trait A.
/// Having `TypedNil` be generic over `T` allows user to define custom Nil type whih would implement trait A,
/// without affecting other type-level lists which do not require such property.
pub struct TypedNil<T> {
    _phantom: PhantomData<T>,
}

impl<T> TypedNil<T> {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T> Contains<TypedNil<T>, Here> for TypedNil<T> {
    #[inline]
    fn get(&self) -> &TypedNil<T> {
        self
    }

    #[inline]
    fn get_mut(&mut self) -> &mut TypedNil<T> {
        self
    }
}

impl<T> Debug for TypedNil<T> {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.debug_struct("TypedNil")
            .field("T", &type_name::<T>())
            .finish()
    }
}

impl<T> Clone for TypedNil<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for TypedNil<T> {}

impl<T> Default for TypedNil<T> {
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<T> PartialEq for TypedNil<T> {
    #[inline]
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl<T> Eq for TypedNil<T> {}

impl<T: Create> Create for TypedNil<T> {
    type Config<'a> = ();
    type CreateError = Infallible;

    #[inline]
    fn create<'a, 'b>(_: Self::Config<'a>, _: Self::Context<'b>) -> CreateResult<Self> {
        Ok(TypedNil::new())
    }
}

impl<T: Destroy> Destroy for TypedNil<T> {
    type Context<'a> = T::Context<'a>;
    type DestroyError = Infallible;

    #[inline]
    fn destroy<'a>(&mut self, _: Self::Context<'a>) -> DestroyResult<Self> {
        Ok(())
    }
}

/// Type alias for a simple type-level list terminator.
pub type Nil = TypedNil<()>;

/// Type list terminator owning `H` type value.
#[derive(Debug, Default, Clone, Copy)]
pub struct Fin<H> {
    pub head: H,
}

impl<H> Fin<H> {
    #[inline]
    pub fn new(head: H) -> Self {
        Self { head }
    }
}

impl<H> Contains<H, Here> for Fin<H> {
    #[inline]
    fn get(&self) -> &H {
        &self.head
    }

    #[inline]
    fn get_mut(&mut self) -> &mut H {
        &mut self.head
    }
}

impl<H> Deref for Fin<H> {
    type Target = H;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.head
    }
}

impl<H> DerefMut for Fin<H> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.head
    }
}

impl<H: PartialEq> PartialEq for Fin<H> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.head == other.head
    }
}

impl<H: Eq> Eq for Fin<H> {}

impl<T: Create> Create for Fin<T> {
    type Config<'a> = T::Config<'a>;
    type CreateError = T::CreateError;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        Ok(Fin::new(T::create(config, context)?))
    }
}

impl<T: Destroy> Destroy for Fin<T> {
    type Context<'a> = T::Context<'a>;
    type DestroyError = T::DestroyError;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.head.destroy(context)
    }
}

/// Type list node owning `head` of type `H` and `tail` of type `T`
#[derive(Debug, Default, Clone, Copy)]
pub struct Cons<H, T> {
    pub head: H,
    pub tail: T,
}

impl<H, T> Cons<H, T> {
    #[inline]
    pub fn new(head: H, tail: T) -> Self {
        Self { head, tail }
    }

    #[inline]
    pub fn get<S, M: Marker>(&self) -> &S
    where
        Self: Contains<S, M>,
    {
        <Self as Contains<S, M>>::get(self)
    }

    #[inline]
    pub fn get_mut<S, M: Marker>(&mut self) -> &mut S
    where
        Self: Contains<S, M>,
    {
        <Self as Contains<S, M>>::get_mut(self)
    }
}

impl<S, N> Contains<S, Here> for Cons<S, N> {
    #[inline]
    fn get(&self) -> &S {
        &self.head
    }

    #[inline]
    fn get_mut(&mut self) -> &mut S {
        &mut self.head
    }
}

impl<O, S, T: Marker, N: Contains<S, T>> Contains<S, There<T>> for Cons<O, N> {
    #[inline]
    fn get(&self) -> &S {
        self.tail.get()
    }

    #[inline]
    fn get_mut(&mut self) -> &mut S {
        self.tail.get_mut()
    }
}

impl<H, T> Deref for Cons<H, T> {
    type Target = H;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.head
    }
}

impl<H, T> DerefMut for Cons<H, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.head
    }
}

impl<H: PartialEq, T: PartialEq> PartialEq for Cons<H, T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.head == other.head && self.tail == other.tail
    }
}

impl<H: Eq, T: Eq> Eq for Cons<H, T> {}

pub enum ConsCreateError<H: Create, T: Create> {
    Head(H::CreateError),
    Tail(T::CreateError),
}

impl<H: Create, T: Create> Debug for ConsCreateError<H, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Head(arg0) => f.debug_tuple("Head").field(arg0).finish(),
            Self::Tail(arg0) => f.debug_tuple("Tail").field(arg0).finish(),
        }
    }
}

impl<H: Create, T: Create> Display for ConsCreateError<H, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Head(arg0) => write!(f, "Head({})", arg0),
            Self::Tail(arg0) => write!(f, "Tail({})", arg0),
        }
    }
}

impl<H: Create, T: Create> Error for ConsCreateError<H, T> {}

impl<H: Create, T: Create> Create for Cons<H, T>
where
    for<'a> H::Context<'a>: Clone + Copy,
    for<'a> T: Destroy<Context<'a> = H::Context<'a>>,
{
    type Config<'a> = Cons<H::Config<'a>, T::Config<'a>>;
    type CreateError = ConsCreateError<H, T>;

    #[inline]
    fn create<'a, 'b>(config: Self::Config<'a>, context: Self::Context<'b>) -> CreateResult<Self> {
        let Cons { head, tail } = config;
        let head = H::create(head, context).map_err(|err| ConsCreateError::Head(err))?;
        let tail = T::create(tail, context).map_err(|err| ConsCreateError::Tail(err))?;
        Ok(Cons::new(head, tail))
    }
}

pub enum ConsDestroyError<H: Destroy, T: Destroy> {
    Head(H::DestroyError),
    Tail(T::DestroyError),
}

impl<H: Destroy, T: Destroy> Debug for ConsDestroyError<H, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Head(arg0) => f.debug_tuple("Head").field(arg0).finish(),
            Self::Tail(arg0) => f.debug_tuple("Tail").field(arg0).finish(),
        }
    }
}

impl<H: Destroy, T: Destroy> Display for ConsDestroyError<H, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Head(arg0) => write!(f, "Head({})", arg0),
            Self::Tail(arg0) => write!(f, "Tail({})", arg0),
        }
    }
}

impl<H: Destroy, T: Destroy> Error for ConsDestroyError<H, T> {}

impl<H: Destroy<DestroyError = Infallible>, T: Destroy> From<ConsDestroyError<H, T>> for Infallible
where
    T::DestroyError: Into<Infallible>,
{
    #[inline]
    fn from(err: ConsDestroyError<H, T>) -> Self {
        unreachable!(
            "ConsDestroyError with Infallible errors should never occur: {:?}",
            err
        )
    }
}

impl<H: Destroy, T: Destroy> Destroy for Cons<H, T>
where
    for<'a> H::Context<'a>: Clone + Copy,
    for<'a> T: Destroy<Context<'a> = H::Context<'a>>,
{
    type Context<'a> = T::Context<'a>;
    type DestroyError = ConsDestroyError<H, T>;

    #[inline]
    fn destroy<'a>(&mut self, context: Self::Context<'a>) -> DestroyResult<Self> {
        self.head
            .destroy(context)
            .map_err(|err| ConsDestroyError::Head(err))?;
        self.tail
            .destroy(context)
            .map_err(|err| ConsDestroyError::Tail(err))?;
        Ok(())
    }
}
