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

pub trait Handler<M, U, T> {
    fn call(self, msg: M, context: &mut U);
}

#[async_trait]
pub trait AsyncHandler<M, U, T> {
    async fn async_call(self, msg: M, context: &mut U);
}

macro_rules! tuple_impls {
    ($($t:ident),*) => {
        impl<M, U, $($t),*, F> Handler<M, U, ($($t,)*)> for F
        where
            F: Fn(M, $($t),*),
            $($t: ConstructFrom<U>,)*
        {
            fn call(self, msg: M, from: &mut U) {
                (self)(msg, $(<$t>::construct(from),)*); // używam unwrap() założywszy, że chcesz to zrobić
            }
        }
        #[async_trait]
        impl<M, U, $($t),*, F, Fut> AsyncHandler<M, U, ($($t,)*)> for F
        where
            M: 'static + Send,
            U: Send,
            F: Fn(M, $($t),*) -> Fut + Send,
            $($t: ConstructFrom<U>,)*
            Fut: Future<Output = ()> + Send,
        {
            async fn async_call(self, msg: M, from: &mut U) {
                (self)(msg, $(<$t>::construct(from),)*).await;
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

pub fn trigger<M, U, T, H>(msg: M, from: &mut U, handler: H)
where
    H: Handler<M, U, T>,
{
    handler.call(msg, from);
}

pub async fn async_trigger<M, U, T, H>(msg: M, from: &mut U, handler: H)
where
    H: AsyncHandler<M, U, T>,
{
    handler.async_call(msg, from).await;
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

#[derive(Clone, Debug, Default, Message)]
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

#[message_handler(Context)]
impl SomeStruct {
    pub fn handle(self, msg: SomeMessage, a: Res<i32>, b: Res<f32>) {
        println!("SomeStruct {:#?} {:#?} {:#?}", msg, a, b.is_some());
    }
}

// impl Handler<SomeMessage, Context, (Res<i32>, Res<f32>)> for SomeStruct {
//     fn call(self, msg: SomeMessage, from: &mut Context) {
//         self.handle(msg, Res::construct(from), Res::construct(from));
//     }
// }

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
        trigger(SomeMessage {}, &mut context, some_struct);
    }
}
