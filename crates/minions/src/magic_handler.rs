use std::{
    any::TypeId,
    future::Future,
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use minions_derive::{async_message_handler, message_handler, Message};

use crate::{
    message::{Message, Msg},
    resources::Res,
};

pub trait ConstructFrom<U> {
    fn construct(from: &mut U) -> Self
    where
        Self: Sized;
}

pub trait Handler<U, T> {
    fn call(self, context: &mut U);
}

#[async_trait]
pub trait AsyncHandler<U, T> {
    async fn async_call(self, context: &mut U);
}

macro_rules! tuple_impls {
    ($($t:ident),*) => {
        impl<U, $($t),*, F> Handler<U, ($($t,)*)> for F
        where
            F: Fn($($t),*),
            $($t: ConstructFrom<U>,)*
        {
            fn call(self, from: &mut U) {
                (self)($(<$t>::construct(from),)*); // używam unwrap() założywszy, że chcesz to zrobić
            }
        }
        #[async_trait]
        impl<U, $($t),*, F, Fut> AsyncHandler<U, ($($t,)*)> for F
        where
            U: Send,
            F: Fn($($t),*) -> Fut + Send,
            $($t: ConstructFrom<U>,)*
            Fut: Future<Output = ()> + Send,
        {
            async fn async_call(self, from: &mut U) {
                (self)($(<$t>::construct(from),)*).await;
            }
        }
    }
}

macro_rules! impl_handler {
    ($($t:ident),*) => {
        tuple_impls!($($t),*);
    };
}

impl_handler!(T1);
impl_handler!(T1, T2);
impl_handler!(T1, T2, T3);
impl_handler!(T1, T2, T3, T4);
impl_handler!(T1, T2, T3, T4, T5);
impl_handler!(T1, T2, T3, T4, T5, T6);
impl_handler!(T1, T2, T3, T4, T5, T6, T7);
impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);

pub fn trigger<U, T, H>(from: &mut U, handler: H)
where
    H: Handler<U, T>,
{
    handler.call(from);
}

pub async fn async_trigger<U, T, H>(from: &mut U, handler: H)
where
    H: AsyncHandler<U, T>,
{
    handler.async_call(from).await;
}

pub struct Context {
    pub x: i32,
    pub y: f32,
}

impl ConstructFrom<Context> for Res<i32> {
    fn construct(from: &mut Context) -> Self
    where
        Self: Sized,
    {
        Res(Some(from.x))
    }
}

impl ConstructFrom<Context> for Res<f32> {
    fn construct(from: &mut Context) -> Self
    where
        Self: Sized,
    {
        Res(None)
    }
}

#[derive(Clone, Default, Message)]
pub struct SomeMessage {}

pub struct SomeStruct {}

impl<T: Default> ConstructFrom<Context> for Msg<T> {
    fn construct(from: &mut Context) -> Self
    where
        Self: Sized,
    {
        Msg(T::default())
    }
}

#[message_handler]
impl SomeStruct {
    pub fn handle(self, a: Res<i32>, b: Res<f32>) {
        println!("SomeStruct {:#?} {:#?}", a, b.is_some());
    }
}

// #[async_trait]
// impl AsyncHandler<Context, (Res<i32>, Res<f32>)> for SomeStruct {
//     async fn async_call(self, from: &mut Context) {
//         self.handle(Res::construct(from), Res::construct(from))
//             .await;
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn query_1() {
        let mut context = Context {
            x: 2137,
            y: 0.1 as f32,
        };

        fn print_res(a: Res<i32>, b: Res<f32>) {
            println!("klsajdl aja: {:#?}, b: {:#?}", a, b);
        }

        let some_struct = SomeStruct {};

        // fn print_res(a: Res<i32>, b: Res<f32>) {
        //     println!("a: {:#?}, b: {:#?}", a, b);
        // }
        // trigger(&mut context, print_res);
        trigger(&mut context, some_struct);
    }
}
