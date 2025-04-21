//! Future Utilities

use std::task::{Poll, Context};
use std::pin::{Pin, pin};
use std::future::Future;

/// Borrowed Future
pub type Fut<'a, T> = &'a mut (dyn Future<Output = T> + Unpin);

/// Future wrapper which races inner futures against each other
pub struct Race<'a, const N: usize, T> {
    contenders: [Fut<'a, T>; N],
}

pub fn race<'a, const N: usize, T>(contenders: [Fut<'a, T>; N]) -> Race<'a, N, T> {
    Race {
        contenders,
    }
}

impl<'a, const N: usize, T> Future for Race<'a, N, T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        for contender in self.contenders.iter_mut() {
            let pinned = pin!(contender);
            if let Poll::Ready(ret) = pinned.poll(ctx) {
                return Poll::Ready(ret);
            }
        }

        Poll::Pending
    }
}

pub enum Sel2<T1, T2> {
    A(T1),
    B(T2),
}

pub enum Sel3<T1, T2, T3> {
    A(T1),
    B(T2),
    C(T3),
}

pub enum Sel4<T1, T2, T3, T4> {
    A(T1),
    B(T2),
    C(T3),
    D(T4),
}

pub async fn select2<T1, T2, A, B>(a: A, b: B) -> Sel2<T1, T2>
where
    A: Future<Output = T1>,
    B: Future<Output = T2>,
{
    let mut wrapper_a = pin!(async { Sel2::A(a.await) });
    let mut wrapper_b = pin!(async { Sel2::B(b.await) });
    race([&mut wrapper_a, &mut wrapper_b]).await
}

pub async fn select3<T1, T2, T3, A, B, C>(a: A, b: B, c: C) -> Sel3<T1, T2, T3>
where
    A: Future<Output = T1>,
    B: Future<Output = T2>,
    C: Future<Output = T3>,
{
    let mut wrapper_a = pin!(async { Sel3::A(a.await) });
    let mut wrapper_b = pin!(async { Sel3::B(b.await) });
    let mut wrapper_c = pin!(async { Sel3::C(c.await) });
    race([&mut wrapper_a, &mut wrapper_b, &mut wrapper_c]).await
}

pub async fn select4<T1, T2, T3, T4, A, B, C, D>(a: A, b: B, c: C, d: D) -> Sel4<T1, T2, T3, T4>
where
    A: Future<Output = T1>,
    B: Future<Output = T2>,
    C: Future<Output = T3>,
    D: Future<Output = T4>,
{
    let mut wrapper_a = pin!(async { Sel4::A(a.await) });
    let mut wrapper_b = pin!(async { Sel4::B(b.await) });
    let mut wrapper_c = pin!(async { Sel4::C(c.await) });
    let mut wrapper_d = pin!(async { Sel4::D(d.await) });
    race([&mut wrapper_a, &mut wrapper_b, &mut wrapper_c, &mut wrapper_d]).await
}

#[test]
fn test_select() {
    let task = async {
        let task_1 = ();
        let task_2 = ();

        let opt_1 = sel_opt(task_1);
        let opt_2 = sel_opt(task_2);

        let ret = match select(task_1, task_2) {
            Sel::A(ret_1) => self.handle_1(ret_1),
            Sel::B(ret_2) => self.handle_2(ret_2),
        };
    };
}
