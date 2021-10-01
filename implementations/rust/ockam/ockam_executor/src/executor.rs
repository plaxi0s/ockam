use core::cell::UnsafeCell;
use core::future::Future;
use core::mem::MaybeUninit;
use core::pin::Pin;
use core::sync::atomic::{self, AtomicBool, AtomicUsize, Ordering};
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use crossbeam_queue::SegQueue;
use ockam_core::compat::boxed::Box;
use ockam_core::compat::collections::BTreeMap;
use ockam_core::compat::sync::{Arc, Mutex, RwLock};
use ockam_core::compat::task::Wake;
use ockam_core::compat::vec::Vec;

use pin_utils::pin_mut;

/// Returns current executor.
/// WARNING: this is not thread-safe
pub fn current() -> &'static Executor<'static> {
    static INIT: AtomicBool = AtomicBool::new(false);
    static mut EXECUTOR: UnsafeCell<MaybeUninit<Executor>> = UnsafeCell::new(MaybeUninit::uninit());

    if INIT.load(Ordering::Relaxed) {
        unsafe { &*(EXECUTOR.get() as *const Executor) }
    } else {
        unsafe {
            let executorp = EXECUTOR.get() as *mut Executor;
            executorp.write(Executor::new());
            atomic::compiler_fence(Ordering::Release);
            INIT.store(true, Ordering::Relaxed);
            &*executorp
        }
    }
}

/// Executor
pub struct Executor<'a> {
    tasks: UnsafeCell<Vec<Box<Task>>>,
    task_queue: Arc<SegQueue<TaskId>>,
    marker: core::marker::PhantomData<&'a ()>,
}

impl<'a> Executor<'a> {
    pub fn new() -> Self {
        Self {
            tasks: UnsafeCell::new(Vec::new()),
            task_queue: Arc::new(SegQueue::new()),
            marker: core::marker::PhantomData,
        }
    }

    pub fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
        pin_mut!(future);
        let ready = AtomicBool::new(true);
        let waker =
            unsafe { Waker::from_raw(RawWaker::new(&ready as *const _ as *const _, &VTABLE)) };

        let result = loop {
            if ready.load(Ordering::Acquire) {
                ready.store(false, Ordering::Release);
                let mut context = Context::from_waker(&waker);
                if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
                    // exit main task
                    break result;
                }
            }

            let tasks = unsafe {
                let tasksp = self.tasks.get() as *mut Vec<Box<Task>>;
                &mut (*tasksp)
            };
            let len = tasks.len();
            for i in 0..len {
                let task = unsafe { tasks.get_unchecked(i) };
                if task.ready.load(Ordering::Acquire) {
                    task.ready.store(false, Ordering::Release);
                    let waker = unsafe {
                        Waker::from_raw(RawWaker::new(&task.ready as *const _ as *const _, &VTABLE))
                    };
                    let mut context = Context::from_waker(&waker);
                    unsafe {
                        let _ready = Pin::new_unchecked(&mut *task.future.get())
                            .poll(&mut context)
                            .is_ready();
                    }
                }
            }
            self.sleep_if_idle();
        };
        result
    }

    /// spawn
    pub fn spawn(&self, future: impl Future + 'static) {
        let task_id = Task::allocate(self, future);
        self.task_queue.push(task_id);
    }

    fn sleep_if_idle(&self) {
        // TODO disable interrupts
        if self.task_queue.is_empty() {
            // TODO sleep
        }
    }
}

// - Task ---------------------------------------------------------------------

type Task = Node<dyn Future<Output = ()> + 'static>;

/// Node
pub struct Node<F>
where
    F: ?Sized,
{
    id: TaskId,
    ready: AtomicBool,
    future: UnsafeCell<F>,
}

impl Task {
    fn allocate(executor: &Executor, future: impl Future + 'static) -> TaskId {
        let task_id = TaskId::new();
        let task = Node {
            id: task_id.clone(),
            ready: AtomicBool::new(true),
            future: UnsafeCell::new(async {
                // task terminating
                future.await;
            }),
        };

        unsafe {
            (*executor.tasks.get()).push(Box::new(task));
        }

        task_id
    }
}

// - TaskId ---------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TaskId(usize);

impl TaskId {
    fn new() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        TaskId(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

// - VTABLE -------------------------------------------------------------------

// NOTE `*const ()` is &AtomicBool
static VTABLE: RawWakerVTable = {
    unsafe fn clone(p: *const ()) -> RawWaker {
        RawWaker::new(p, &VTABLE)
    }
    unsafe fn wake(p: *const ()) {
        wake_by_ref(p)
    }
    unsafe fn wake_by_ref(p: *const ()) {
        (*(p as *const AtomicBool)).store(true, Ordering::Release)
    }
    unsafe fn drop(_: *const ()) {
        // no-op
    }

    RawWakerVTable::new(clone, wake, wake_by_ref, drop)
};

struct TaskWaker<'a> {
    task_id: TaskId,
    task_queue: Arc<SegQueue<TaskId>>,
    marker: core::marker::PhantomData<&'a ()>,
}

impl<'a> TaskWaker<'a> {
    fn reschedule_task(&self) {
        self.task_queue.push(self.task_id);
    }
}

impl<'a> Wake for TaskWaker<'a> {
    fn wake(self: Arc<Self>) {
        self.reschedule_task();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.reschedule_task();
    }
}
