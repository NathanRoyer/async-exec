//! Simple async task executor with a built-in HTTP task monitor

use std::thread::{Thread, Result, JoinHandle, current, sleep, spawn, park};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize};
use std::task::{Poll, Wake, Waker, Context};
use std::sync::atomic::Ordering::SeqCst;
use std::time::{Instant, Duration};
use std::pin::{pin, Pin};
use std::future::Future;
use std::sync::Arc;

#[cfg(feature = "monitor")]
use serde::Serialize;

use async_fifo::{Sender, Receiver};
use async_fifo::non_blocking::Producer;

pub mod utils;

#[cfg(feature = "time")]
pub mod time;

#[cfg(feature = "monitor")]
mod monitor;

#[allow(dead_code)]
enum TaskExec {
    Polling(TaskId),
    PollReady(TaskId),
    PollPending(TaskId),
}

#[cfg_attr(feature = "monitor", derive(Serialize))]
#[allow(dead_code)]
struct TaskDecl {
    id: TaskId,
    name: String,
    runner: usize,
}

type TaskId = usize;

/// Pinned future that can be send across threads
///
/// Automatically implements `From<F>` for `Send` futures.
pub struct Task {
    inner: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl<F: Future<Output = ()> + Send + 'static> From<F> for Task {
    fn from(fut: F) -> Self {
        Self { inner: Box::pin(fut) }
    }
}

const FLAGS: usize = 64;

struct WakerCommon {
    thread: Thread,
    ready_flags: AtomicU64,
    task_rx_flag: AtomicBool,
}

struct ThreadWaker {
    common: Arc<WakerCommon>,
    index: usize,
}

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        Self::wake_by_ref(&self)
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if self.index < FLAGS {
            let mask = 1 << self.index;
            self.common.ready_flags.fetch_or(mask, SeqCst);
        } else {
            self.common.task_rx_flag.store(true, SeqCst);
        }

        self.common.thread.unpark();
    }
}

fn init_waker(i: usize, common: &Arc<WakerCommon>) -> Waker {
    let waker = ThreadWaker {
        common: common.clone(),
        index: i,
    };

    Waker::from(Arc::new(waker))
}

struct TaskData {
    task: Task,
    id: TaskId,
}

fn try_tx_exec(tx: &Option<Producer<(TaskExec, Instant)>>, exec: TaskExec) {
    if let Some(tx) = tx {
        tx.send((exec, Instant::now()));
    }
}

fn runner(
    mut rx_tasks: Receiver<TaskData>,
    tx_mon_exec: Option<Producer<(TaskExec, Instant)>>,
) {
    let common = Arc::new(WakerCommon {
        thread: current(),
        ready_flags: AtomicU64::new(0),
        task_rx_flag: AtomicBool::new(true),
    });

    let new_task_waker = init_waker(FLAGS, &common);

    let mut receiver = pin!(rx_tasks.recv());
    let mut tasks: Vec<Option<TaskData>> = Vec::new();
    let mut can_receive = true;
    let mut num_started = 0;
    let mut num_ended = 0;

    while num_started != num_ended || can_receive {
        // read and reset the flags
        let mut ready_flags = common.ready_flags.swap(0, SeqCst);
        let task_rx_flag = common.task_rx_flag.swap(false, SeqCst);

        // is there anything to do?
        if ready_flags == 0 && !task_rx_flag {
            // wait for someone to wake this thread back up
            park();
            continue;
        }

        if task_rx_flag {
            let mut context = Context::from_waker(&new_task_waker);
            can_receive = loop {
                let task = match Future::poll(receiver.as_mut(), &mut context) {
                    Poll::Ready(Ok(new_task)) => new_task,
                    Poll::Ready(Err(_)) => break false,
                    Poll::Pending => break true,
                };

                let len = tasks.len();
                let i = tasks.iter().position(Option::is_none).unwrap_or(len);

                match i < len {
                    true => tasks[i] = Some(task),
                    false => tasks.push(Some(task)),
                }

                ready_flags |= 1 << (i % FLAGS);
                num_started += 1;
            };
        }


        for (i, maybe_task) in tasks.iter_mut().enumerate() {
            let slot = i % FLAGS;
            let ready = (ready_flags & (1 << slot)) != 0;

            if !ready { continue; }
            let Some(data) = maybe_task else { continue };

            let waker = init_waker(slot, &common);
            let fut = data.task.inner.as_mut();
            let mut context = Context::from_waker(&waker);

            try_tx_exec(&tx_mon_exec, TaskExec::Polling(data.id));

            if let Poll::Ready(()) = Future::poll(fut, &mut context) {
                try_tx_exec(&tx_mon_exec, TaskExec::PollReady(data.id));
                num_ended += 1;
                *maybe_task = None;
            } else {
                try_tx_exec(&tx_mon_exec, TaskExec::PollPending(data.id));
            }
        }
    }

    println!("thread down, ran {num_ended} task(s)");
}

/// Executor for asynchronous tasks
pub struct Executor {
    tx_tasks: Vec<Sender<TaskData>>,
    handles: Vec<JoinHandle<()>>,
    next_id: AtomicUsize,
    #[cfg(feature = "monitor")]
    tx_mon_decl: Option<Producer<TaskDecl>>,
}

impl Executor {
    /// Creates a new Executor with a number of threads
    #[allow(unused_mut)]
    #[allow(unused_variables)]
    pub fn new(threads: usize, monitor_port: Option<u16>) -> Self {
        let mut tx_tasks = Vec::new();
        let mut handles = Vec::new();
        let mut tx_mon_exec = None;
        let mut monitor_task: Option<Task> = None;

        #[cfg(feature = "monitor")]
        let tx_mon_decl = if let Some(port) = monitor_port {
            type Fifo = async_fifo::fifo::DefaultBlockSize;
            let (tx_exec, rx_exec) = Fifo::non_blocking();
            let (tx_name, rx_name) = Fifo::non_blocking();
            let task = monitor::server(port, rx_exec, rx_name);
            monitor_task = Some(task.into());
            tx_mon_exec = Some(tx_exec);
            Some(tx_name)
        } else {
            None
        };

        for _ in 0..threads {
            let (tx, rx) = async_fifo::new();
            let tx_mon_exec = tx_mon_exec.clone();
            handles.push(spawn(|| runner(rx, tx_mon_exec)));
            tx_tasks.push(tx);
        }

        let this = Self {
            tx_tasks,
            handles,
            next_id: AtomicUsize::new(0),
            #[cfg(feature = "monitor")]
            tx_mon_decl,
        };

        if let Some(monitor_task) = monitor_task {
            this.spawn_with_name(monitor_task, "monitor-server");
        }

        this
    }

    /// Start a task on an executor thread
    pub fn spawn<T: Into<Task>>(&self, task: T) {
        self.spawn_with_name(task, "[unnamed]")
    }

    /// Start a task on an executor thread, specifying its name for the monitor
    pub fn spawn_with_name<T: Into<Task>, S: Into<String>>(&self, task: T, _name: S) {
        let task = task.into();
        let id = self.next_id.fetch_add(1, SeqCst);

        let data = TaskData {
            task,
            id: id,
        };

        let i = id % self.tx_tasks.len();

        #[cfg(feature = "monitor")]
        if let Some(tx_mon_decl) = &self.tx_mon_decl {
            let decl = TaskDecl {
                id,
                name: _name.into(),
                runner: i,
            };

            tx_mon_decl.send(decl);
        }

        self.tx_tasks[i].send(data);
    }

    /// Wait for all started tasks to finish
    pub fn join(mut self) -> Result<()> {
        self.tx_tasks.drain(..);

        for handle in self.handles {
            handle.join()?;
        }

        Ok(())
    }

    pub fn join_arc(this: Arc<Self>) -> Result<()> {
        let exec = loop {
            sleep(Duration::from_millis(100));

            if Arc::strong_count(&this) == 1 {
                let exec = Arc::into_inner(this);
                break exec.expect("Failed to recover Executor");
            }
        };

        exec.join()
    }
}

#[test]
fn test_bad_timer() {
    use std::time::Instant;

    struct Timer {
        expiration: Instant,
    }

    impl Future for Timer {
        type Output = ();
        fn poll(self: Pin<&mut Self>, _ctx: &mut Context) -> Poll<()> {
            while Instant::now() < self.expiration {}
            Poll::Ready(())
        }
    }

    const ONE_SEC: Duration = Duration::from_secs(1);

    fn sleep(duration: Duration) -> Timer {
        Timer {
            expiration: Instant::now() + duration,
        }
    }

    let exec = Executor::new(4, Some(9090));

    for _ in 0.. {
        let task = async {
            sleep(ONE_SEC).await;
            println!("Done");
        };

        exec.spawn_with_name(task, "test");
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
}

#[test]
fn test_monitor() {
    let exec = Executor::new(4, Some(9090));

    for _ in 0.. {
        let task = async {
            println!("Done");
        };

        exec.spawn_with_name(task, "test");
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
}

#[test]
fn test_runners() {
    let (tx_data, rx_data) = async_fifo::fifo::LargeBlockSize::channel();

    let executor = Executor::new(8, None);
    let counter = Arc::new(AtomicUsize::new(0));

    for _ in 0..256 {
        let counter = counter.clone();
        let mut rx_data = rx_data.clone();

        let task = async move {
            rx_data.recv_array::<8192>().await.unwrap();
            counter.fetch_add(1, SeqCst);
        };

        executor.spawn(task);
    }

    let data = [(); 256 * 8192];
    tx_data.send_iter(data.iter().cloned());

    executor.join().unwrap();

    assert_eq!(counter.load(SeqCst), 256);
}
