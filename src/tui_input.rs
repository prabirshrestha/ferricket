use std::{
    collections::VecDeque,
    io,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use tokio::sync::Notify;

const NAVIGATION_INTERVAL: Duration = Duration::from_millis(16);
const INPUT_CAPACITY: usize = 64;
const MAX_ROUTE_STEPS: usize = 16;

struct QueuedEvent {
    result: io::Result<Event>,
    count: usize,
}

struct InputState {
    events: VecDeque<QueuedEvent>,
    closed: bool,
}

struct SharedInput {
    state: Mutex<InputState>,
    space: Condvar,
    ready: Notify,
}

impl SharedInput {
    fn new() -> Self {
        Self {
            state: Mutex::new(InputState {
                events: VecDeque::new(),
                closed: false,
            }),
            space: Condvar::new(),
            ready: Notify::new(),
        }
    }

    fn push(&self, result: io::Result<Event>, stop: &AtomicBool) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Ok(event) = &result
            && let Some(last) = state.events.back_mut()
        {
            if matches!(event, Event::Resize(_, _))
                && matches!(last.result, Ok(Event::Resize(_, _)))
            {
                last.result = result;
                last.count = 1;
                drop(state);
                self.ready.notify_one();
                return true;
            }
            if matches!(event, Event::Key(key) if key.kind == KeyEventKind::Repeat)
                && matches!(&last.result, Ok(previous) if previous == event)
            {
                last.count = last.count.saturating_add(1);
                drop(state);
                self.ready.notify_one();
                return true;
            }
        }
        while state.events.len() >= INPUT_CAPACITY && !stop.load(Ordering::Acquire) {
            state = self
                .space
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
        if stop.load(Ordering::Acquire) || state.closed {
            return false;
        }
        state.events.push_back(QueuedEvent { result, count: 1 });
        drop(state);
        self.ready.notify_one();
        true
    }

    fn try_pop(&self) -> Option<QueuedEvent> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let event = state.events.pop_front();
        if event.is_some() {
            self.space.notify_one();
        }
        event
    }

    fn is_closed(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .closed
    }

    fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.closed = true;
        drop(state);
        self.space.notify_all();
        self.ready.notify_waiters();
    }
}

struct InputReader {
    shared: Arc<SharedInput>,
    stop: Arc<AtomicBool>,
    reader: Option<thread::JoinHandle<()>>,
}

impl Drop for InputReader {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.shared.close();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

pub struct InputCoordinator {
    reader: InputReader,
    queued: VecDeque<QueuedEvent>,
    pending_navigation: Option<KeyEvent>,
    navigation_deadline: Option<tokio::time::Instant>,
}

impl InputCoordinator {
    pub fn spawn() -> Self {
        Self::new(spawn_input_reader())
    }

    fn new(reader: InputReader) -> Self {
        Self {
            reader,
            queued: VecDeque::new(),
            pending_navigation: None,
            navigation_deadline: None,
        }
    }

    pub async fn next(&mut self, text_input: bool) -> io::Result<Option<Event>> {
        let mut route_steps = 0;
        loop {
            if let Some(queued) = self
                .queued
                .pop_front()
                .or_else(|| self.reader.shared.try_pop())
            {
                let event = queued.result?;
                if queued.count > 1
                    && text_input
                    && matches!(event, Event::Key(key) if key.kind == KeyEventKind::Repeat)
                    && !matches!(event, Event::Key(key) if is_navigation(key, text_input))
                {
                    self.queued.push_front(QueuedEvent {
                        result: Ok(event.clone()),
                        count: queued.count - 1,
                    });
                }
                if let Some(event) = self.route(event, text_input) {
                    return Ok(Some(event));
                }
                route_steps += 1;
                if route_steps == MAX_ROUTE_STEPS {
                    route_steps = 0;
                    tokio::task::yield_now().await;
                }
                continue;
            }
            if self.reader.shared.is_closed() {
                return Ok(None);
            }
            let notified = self.reader.shared.ready.notified();
            tokio::select! {
                _ = notified => {}
                _ = async {
                    match self.navigation_deadline {
                        Some(deadline) if self.pending_navigation.is_some() => {
                            tokio::time::sleep_until(deadline).await
                        }
                        _ => std::future::pending().await,
                    }
                } => {
                    if let Some(key) = self.pending_navigation.take() {
                        self.navigation_deadline = Some(
                            tokio::time::Instant::now() + NAVIGATION_INTERVAL,
                        );
                        return Ok(Some(Event::Key(key)));
                    }
                }
            }
        }
    }

    fn route(&mut self, event: Event, text_input: bool) -> Option<Event> {
        let Event::Key(key) = event else {
            return Some(event);
        };
        if !is_navigation(key, text_input) {
            if key.kind == KeyEventKind::Repeat && !text_input {
                return None;
            }
            if let Some(pending) = self.pending_navigation.take()
                && pending.kind == KeyEventKind::Press
            {
                self.queued.push_back(QueuedEvent {
                    result: Ok(Event::Key(key)),
                    count: 1,
                });
                return Some(Event::Key(pending));
            }
            return Some(Event::Key(key));
        }
        if key.kind == KeyEventKind::Release {
            let pending = self.pending_navigation.take();
            return pending
                .filter(|pending| pending.kind == KeyEventKind::Press)
                .map(Event::Key);
        }
        let now = tokio::time::Instant::now();
        if self
            .navigation_deadline
            .is_none_or(|deadline| now >= deadline)
        {
            self.navigation_deadline = Some(now + NAVIGATION_INTERVAL);
            return Some(Event::Key(key));
        }
        self.pending_navigation = Some(key);
        None
    }
}

fn spawn_input_reader() -> InputReader {
    let shared = Arc::new(SharedInput::new());
    let stop = Arc::new(AtomicBool::new(false));
    let reader_stop = Arc::clone(&stop);
    let reader_shared = Arc::clone(&shared);
    let reader = thread::spawn(move || {
        while !reader_stop.load(Ordering::Acquire) {
            match event::poll(Duration::from_millis(25)) {
                Ok(true) => {
                    let result = event::read();
                    let successful = result.is_ok();
                    if !reader_shared.push(result, &reader_stop) || !successful {
                        break;
                    }
                }
                Ok(false) => {}
                Err(error) => {
                    let _ = reader_shared.push(Err(error), &reader_stop);
                    break;
                }
            }
        }
        reader_shared.close();
    });
    InputReader {
        shared,
        stop,
        reader: Some(reader),
    }
}

fn is_navigation(key: KeyEvent, text_input: bool) -> bool {
    matches!(
        key.code,
        KeyCode::Down | KeyCode::Up | KeyCode::Left | KeyCode::Right
    ) || (!text_input && matches!(key.code, KeyCode::Char('h' | 'j' | 'k' | 'l')))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    struct TestSender {
        shared: Arc<SharedInput>,
        stop: Arc<AtomicBool>,
    }

    impl TestSender {
        fn send(&self, result: io::Result<Event>) -> Result<(), ()> {
            self.shared.push(result, &self.stop).then_some(()).ok_or(())
        }
    }

    fn coordinator() -> (TestSender, InputCoordinator) {
        let shared = Arc::new(SharedInput::new());
        let stop = Arc::new(AtomicBool::new(false));
        let reader = InputReader {
            shared: Arc::clone(&shared),
            stop: Arc::clone(&stop),
            reader: None,
        };
        (TestSender { shared, stop }, InputCoordinator::new(reader))
    }

    fn key(code: KeyCode, kind: KeyEventKind) -> Event {
        Event::Key(KeyEvent::new_with_kind(code, KeyModifiers::NONE, kind))
    }

    #[tokio::test]
    async fn arrow_flood_cannot_block_a_following_command() {
        let (sender, mut input) = coordinator();
        sender
            .send(Ok(key(KeyCode::Down, KeyEventKind::Press)))
            .unwrap();
        for _ in 0..1_000 {
            sender
                .send(Ok(key(KeyCode::Down, KeyEventKind::Repeat)))
                .unwrap();
        }
        sender
            .send(Ok(key(KeyCode::Char('q'), KeyEventKind::Press)))
            .unwrap();
        assert_eq!(
            input.next(false).await.unwrap().unwrap(),
            key(KeyCode::Down, KeyEventKind::Press)
        );
        assert_eq!(
            input.next(false).await.unwrap().unwrap(),
            key(KeyCode::Char('q'), KeyEventKind::Press)
        );
    }

    #[tokio::test]
    async fn vim_flood_is_coalesced_outside_text_input() {
        let (sender, mut input) = coordinator();
        sender
            .send(Ok(key(KeyCode::Char('j'), KeyEventKind::Press)))
            .unwrap();
        for _ in 0..1_000 {
            sender
                .send(Ok(key(KeyCode::Char('j'), KeyEventKind::Repeat)))
                .unwrap();
        }
        sender
            .send(Ok(key(KeyCode::Char('q'), KeyEventKind::Press)))
            .unwrap();
        assert_eq!(
            input.next(false).await.unwrap().unwrap(),
            key(KeyCode::Char('j'), KeyEventKind::Press)
        );
        assert_eq!(
            input.next(false).await.unwrap().unwrap(),
            key(KeyCode::Char('q'), KeyEventKind::Press)
        );
    }

    #[tokio::test]
    async fn quick_press_release_moves_once() {
        let (sender, mut input) = coordinator();
        sender
            .send(Ok(key(KeyCode::Down, KeyEventKind::Press)))
            .unwrap();
        sender
            .send(Ok(key(KeyCode::Down, KeyEventKind::Release)))
            .unwrap();
        assert_eq!(
            input.next(false).await.unwrap().unwrap(),
            key(KeyCode::Down, KeyEventKind::Press)
        );
    }

    #[tokio::test]
    async fn navigation_is_delivered_before_a_modal_command() {
        let (sender, mut input) = coordinator();
        sender
            .send(Ok(key(KeyCode::Down, KeyEventKind::Press)))
            .unwrap();
        sender
            .send(Ok(key(KeyCode::Char('c'), KeyEventKind::Press)))
            .unwrap();
        assert_eq!(
            input.next(false).await.unwrap().unwrap(),
            key(KeyCode::Down, KeyEventKind::Press)
        );
        assert_eq!(
            input.next(false).await.unwrap().unwrap(),
            key(KeyCode::Char('c'), KeyEventKind::Press)
        );
    }

    #[tokio::test]
    async fn vim_characters_are_not_coalesced_in_text_input() {
        let (sender, mut input) = coordinator();
        sender
            .send(Ok(key(KeyCode::Char('j'), KeyEventKind::Press)))
            .unwrap();
        sender
            .send(Ok(key(KeyCode::Char('j'), KeyEventKind::Repeat)))
            .unwrap();
        assert_eq!(
            input.next(true).await.unwrap().unwrap(),
            key(KeyCode::Char('j'), KeyEventKind::Press)
        );
        assert_eq!(
            input.next(true).await.unwrap().unwrap(),
            key(KeyCode::Char('j'), KeyEventKind::Repeat)
        );
    }

    #[tokio::test]
    async fn resize_burst_keeps_only_the_latest_size() {
        let (sender, mut input) = coordinator();
        for width in 80..180 {
            sender.send(Ok(Event::Resize(width, 24))).unwrap();
        }
        sender
            .send(Ok(key(KeyCode::Char('q'), KeyEventKind::Press)))
            .unwrap();
        assert_eq!(
            input.next(false).await.unwrap(),
            Some(Event::Resize(179, 24))
        );
        assert_eq!(
            input.next(false).await.unwrap().unwrap(),
            key(KeyCode::Char('q'), KeyEventKind::Press)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn continuing_repeat_producer_cannot_starve_the_scheduler() {
        let (sender, mut input) = coordinator();
        sender
            .send(Ok(key(KeyCode::Down, KeyEventKind::Press)))
            .unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let producer_running = Arc::clone(&running);
        let producer = thread::spawn(move || {
            while producer_running.load(Ordering::Acquire) {
                let _ = sender.send(Ok(key(KeyCode::Down, KeyEventKind::Repeat)));
            }
        });
        let consumer_running = Arc::clone(&running);
        let consumer = tokio::spawn(async move {
            while consumer_running.load(Ordering::Acquire) {
                let _ = input.next(false).await;
            }
        });
        tokio::time::timeout(Duration::from_millis(250), async {
            for _ in 0..5 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the runtime scheduler should remain runnable during repeat input");
        running.store(false, Ordering::Release);
        consumer.abort();
        producer.join().unwrap();
    }
}
