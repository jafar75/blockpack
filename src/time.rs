use std::time::{Duration, Instant};

/// A clock abstraction that can be real or simulated
pub trait Clock: Send {
    /// Get current time as duration since clock start
    fn now(&self) -> Duration;
    
    /// Sleep for a duration (no-op in simulated mode)
    fn sleep(&self, duration: Duration);
    
    /// Advance time (only works in simulated mode)
    fn advance(&mut self, duration: Duration);
    
    /// Check if this is a simulated clock
    fn is_simulated(&self) -> bool;
}

/// Real wall-clock time
#[derive(Debug)]
pub struct RealClock {
    start: Instant,
}

impl RealClock {
    pub fn new() -> Self {
        Self { start: Instant::now() }
    }
}

impl Default for RealClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for RealClock {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }
    
    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
    
    fn advance(&mut self, _duration: Duration) {
        // No-op for real clock
    }
    
    fn is_simulated(&self) -> bool {
        false
    }
}

/// Simulated clock for fast testing
#[derive(Debug)]
pub struct SimulatedClock {
    current: Duration,
}

impl SimulatedClock {
    pub fn new() -> Self {
        Self { current: Duration::ZERO }
    }
    
    pub fn at(start: Duration) -> Self {
        Self { current: start }
    }
}

impl Default for SimulatedClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SimulatedClock {
    fn now(&self) -> Duration {
        self.current
    }
    
    fn sleep(&self, _duration: Duration) {
        // No-op - simulated clock doesn't actually sleep
    }
    
    fn advance(&mut self, duration: Duration) {
        self.current += duration;
    }
    
    fn is_simulated(&self) -> bool {
        true
    }
}

/// Simulation runner that processes events in simulated time
pub struct Simulation<C: Clock> {
    clock: C,
    speed: f64, // 1.0 = real time, 2.0 = 2x speed, 0.0 = instant
}

impl<C: Clock> Simulation<C> {
    pub fn new(clock: C) -> Self {
        Self { clock, speed: 1.0 }
    }
    
    pub fn with_speed(clock: C, speed: f64) -> Self {
        Self { clock, speed }
    }
    
    pub fn clock(&self) -> &C {
        &self.clock
    }
    
    pub fn clock_mut(&mut self) -> &mut C {
        &mut self.clock
    }
    
    pub fn now(&self) -> Duration {
        self.clock.now()
    }
    
    /// Sleep respecting simulation speed
    pub fn sleep(&self, duration: Duration) {
        if self.speed <= 0.0 {
            return;
        }
        let scaled = Duration::from_secs_f64(duration.as_secs_f64() / self.speed);
        self.clock.sleep(scaled);
    }
    
    pub fn speed(&self) -> f64 {
        self.speed
    }
    
    pub fn set_speed(&mut self, speed: f64) {
        self.speed = speed;
    }
}

/// Timed event for simulation
#[derive(Debug, Clone)]
pub struct TimedEvent<T> {
    pub time: Duration,
    pub event: T,
}

impl<T> TimedEvent<T> {
    pub fn new(time: Duration, event: T) -> Self {
        Self { time, event }
    }
    
    pub fn at_secs(secs: f64, event: T) -> Self {
        Self {
            time: Duration::from_secs_f64(secs),
            event,
        }
    }
}

impl<T> PartialEq for TimedEvent<T> {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time
    }
}

impl<T> Eq for TimedEvent<T> {}

impl<T> PartialOrd for TimedEvent<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for TimedEvent<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse order for min-heap behavior with BinaryHeap
        other.time.cmp(&self.time)
    }
}

/// Event queue for discrete event simulation
pub struct EventQueue<T> {
    events: std::collections::BinaryHeap<TimedEvent<T>>,
}

impl<T> EventQueue<T> {
    pub fn new() -> Self {
        Self {
            events: std::collections::BinaryHeap::new(),
        }
    }
    
    pub fn push(&mut self, event: TimedEvent<T>) {
        self.events.push(event);
    }
    
    pub fn schedule(&mut self, time: Duration, event: T) {
        self.push(TimedEvent::new(time, event));
    }
    
    pub fn pop(&mut self) -> Option<TimedEvent<T>> {
        self.events.pop()
    }
    
    pub fn peek(&self) -> Option<&TimedEvent<T>> {
        self.events.peek()
    }
    
    pub fn peek_time(&self) -> Option<Duration> {
        self.events.peek().map(|e| e.time)
    }
    
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
    
    pub fn len(&self) -> usize {
        self.events.len()
    }
}

impl<T> Default for EventQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Run a discrete event simulation
pub fn run_discrete_simulation<T, F>(
    mut events: EventQueue<T>,
    end_time: Duration,
    mut handler: F,
) where
    F: FnMut(Duration, T),
{
    while let Some(event) = events.pop() {
        if event.time > end_time {
            break;
        }
        handler(event.time, event.event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_real_clock() {
        let clock = RealClock::new();
        let t1 = clock.now();
        std::thread::sleep(Duration::from_millis(10));
        let t2 = clock.now();
        
        assert!(t2 > t1);
        assert!(!clock.is_simulated());
    }
    
    #[test]
    fn test_simulated_clock() {
        let mut clock = SimulatedClock::new();
        
        assert_eq!(clock.now(), Duration::ZERO);
        assert!(clock.is_simulated());
        
        clock.advance(Duration::from_secs(10));
        assert_eq!(clock.now(), Duration::from_secs(10));
        
        clock.advance(Duration::from_millis(500));
        assert_eq!(clock.now(), Duration::from_millis(10_500));
    }
    
    #[test]
    fn test_simulated_clock_no_sleep() {
        let clock = SimulatedClock::new();
        let start = std::time::Instant::now();
        
        // This should not actually sleep
        clock.sleep(Duration::from_secs(100));
        
        assert!(start.elapsed() < Duration::from_millis(100));
    }
    
    #[test]
    fn test_simulation_speed() {
        let clock = RealClock::new();
        let sim = Simulation::with_speed(clock, 2.0);
        
        assert_eq!(sim.speed(), 2.0);
    }
    
    #[test]
    fn test_event_queue_ordering() {
        let mut queue = EventQueue::new();
        
        queue.schedule(Duration::from_secs(3), "third");
        queue.schedule(Duration::from_secs(1), "first");
        queue.schedule(Duration::from_secs(2), "second");
        
        assert_eq!(queue.pop().unwrap().event, "first");
        assert_eq!(queue.pop().unwrap().event, "second");
        assert_eq!(queue.pop().unwrap().event, "third");
    }
    
    #[test]
    fn test_event_queue_peek() {
        let mut queue = EventQueue::new();
        
        queue.schedule(Duration::from_secs(5), "event");
        
        assert_eq!(queue.peek_time(), Some(Duration::from_secs(5)));
        assert_eq!(queue.len(), 1);
        
        queue.pop();
        assert!(queue.is_empty());
    }
    
    #[test]
    fn test_discrete_simulation() {
        let mut queue = EventQueue::new();
        
        queue.schedule(Duration::from_secs(1), 1);
        queue.schedule(Duration::from_secs(2), 2);
        queue.schedule(Duration::from_secs(3), 3);
        queue.schedule(Duration::from_secs(10), 10); // Beyond end time
        
        let mut results = Vec::new();
        
        run_discrete_simulation(
            queue,
            Duration::from_secs(5),
            |time, event| {
                results.push((time, event));
            },
        );
        
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], (Duration::from_secs(1), 1));
        assert_eq!(results[1], (Duration::from_secs(2), 2));
        assert_eq!(results[2], (Duration::from_secs(3), 3));
    }
    
    #[test]
    fn test_timed_event_at_secs() {
        let event = TimedEvent::at_secs(1.5, "test");
        assert_eq!(event.time, Duration::from_millis(1500));
        assert_eq!(event.event, "test");
    }
}