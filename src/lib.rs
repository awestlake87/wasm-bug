#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        mem,
        pin::Pin,
        task::{Context, Poll},
    };

    use futures::{channel::mpsc, sink::SinkExt, stream::StreamExt};

    use wasm_bindgen_test::wasm_bindgen_test;

    // async-std does not compile for WASM yet.
    #[cfg(not(target_arch = "wasm32"))]
    use async_std::task;

    // A simple task distilled down to something that sends and receives an item on
    // an mpsc channel. Pretty dumb, I know, but it fails for some reason under
    // certain circumstances.
    async fn test_task(num: i32) -> i32 {
        let (mut tx, mut rx) = mpsc::channel(1);

        tx.send(num).await.unwrap();

        rx.next().await.unwrap()
    }

    // Wraps a list of tasks and polls to collect their results. This is very
    // similar to the 'join' combinator, but in my own project, I'm trying to
    // do something like a dynamic join where I add tasks to the list between
    // polls. I just isolated the bits that cause the problem here, though.
    struct TaskWrapper {
        tasks: Vec<Pin<Box<dyn Future<Output = i32>>>>,
        results: Vec<Option<i32>>,
    }

    impl TaskWrapper {
        fn new(tasks: Vec<Pin<Box<dyn Future<Output = i32>>>>) -> Self {
            let mut results = Vec::with_capacity(tasks.len());
            results.resize(tasks.len(), None);

            Self { tasks, results }
        }
    }

    impl Future for TaskWrapper {
        type Output = Vec<i32>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
            let mut still_polling = false;

            let mut results = mem::replace(&mut self.results, vec![]);

            // poll each task and store the result if it's ready
            for (i, task) in self.tasks.iter_mut().enumerate() {
                // check if we should poll this task
                if results[i].is_none() {
                    still_polling = true;

                    match task.as_mut().poll(cx) {
                        Poll::Pending => continue,
                        Poll::Ready(result) => results[i] = Some(result),
                    }
                }
            }

            mem::replace(&mut self.results, results);

            if still_polling {
                Poll::Pending
            } else {
                // unwrap each result and collect into a vec
                Poll::Ready(self.results.drain(..).map(|res| res.unwrap()).collect())
            }
        }
    }

    // This is the problematic task in WASM
    async fn unsound_in_wasm_task() {
        let wrapper = TaskWrapper::new(vec![
            Pin::from(Box::new(test_task(12))),
            Pin::from(Box::new(test_task(43))),
            Pin::from(Box::new(test_task(-15))),
        ]);
        assert_eq!(wrapper.await, vec![12, 43, -15]);
    }

    // Using the executor in async-std, the task runs just fine.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn sound_in_async_std() {
        task::block_on(unsound_in_wasm_task());
    }

    // The same task on the wasm-bindgen-futures executor crashes
    #[wasm_bindgen_test(async)]
    async fn unsound_in_wasm() {
        unsound_in_wasm_task().await;
    }

    // Just a small proof that the test_task works on its own
    #[wasm_bindgen_test(async)]
    async fn this_works_in_wasm() {
        assert_eq!(test_task(12).await, 12);
    }
}
