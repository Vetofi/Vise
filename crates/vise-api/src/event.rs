
use crate::identifier::Identifier;
use std::sync::{Arc, RwLock};

type Callback<C, R> = dyn Fn(&mut C) -> R + Send + Sync;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Phase(Identifier);

impl Phase {
    pub const EARLY_NAME: &'static str = "early";
    pub const DEFAULT_NAME: &'static str = "default";
    pub const LATE_NAME: &'static str = "late";

    pub fn early() -> Self {
        Self(Identifier::of("vise", Self::EARLY_NAME))
    }
    pub fn default_phase() -> Self {
        Self(Identifier::of("vise", Self::DEFAULT_NAME))
    }
    pub fn late() -> Self {
        Self(Identifier::of("vise", Self::LATE_NAME))
    }
    pub fn named(id: Identifier) -> Self {
        Self(id)
    }
    pub fn id(&self) -> &Identifier {
        &self.0
    }
}

impl From<Identifier> for Phase {
    fn from(id: Identifier) -> Self {
        Self(id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ListenerId(u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhaseCycle {
    pub first: Identifier,
    pub second: Identifier,
}

impl std::fmt::Display for PhaseCycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "phase {} cannot run before {}: that would make a cycle",
            self.first, self.second
        )
    }
}
impl std::error::Error for PhaseCycle {}

struct Listener<C, R> {
    id: ListenerId,
    phase: usize,
    f: Arc<Callback<C, R>>,
}

struct State<C, R> {
    phases: Vec<Phase>,
    edges: Vec<(usize, usize)>,
    listeners: Vec<Listener<C, R>>,
    next_id: u64,
    snapshot: Arc<Vec<Arc<Callback<C, R>>>>,
}

impl<C, R> State<C, R> {
    fn phase_index(&mut self, phase: &Phase) -> usize {
        if let Some(i) = self.phases.iter().position(|p| p == phase) {
            return i;
        }
        self.phases.push(phase.clone());
        self.phases.len() - 1
    }

    fn phase_order(&self) -> Option<Vec<usize>> {
        let n = self.phases.len();
        let mut indegree = vec![0usize; n];
        for &(_, b) in &self.edges {
            indegree[b] += 1;
        }
        let mut done = vec![false; n];
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            let next = (0..n).find(|&i| !done[i] && indegree[i] == 0)?;
            done[next] = true;
            out.push(next);
            for &(a, b) in &self.edges {
                if a == next {
                    indegree[b] -= 1;
                }
            }
        }
        Some(out)
    }

    fn rebuild(&mut self) {
        let order = self.phase_order().expect("edges are checked when added");
        let mut calls = Vec::with_capacity(self.listeners.len());
        for phase in order {
            calls.extend(
                self.listeners
                    .iter()
                    .filter(|l| l.phase == phase)
                    .map(|l| l.f.clone()),
            );
        }
        self.snapshot = Arc::new(calls);
    }
}

pub struct Event<C, R = ()> {
    state: RwLock<State<C, R>>,
}

impl<C, R> Default for Event<C, R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C, R> Event<C, R> {
    pub fn new() -> Self {
        let phases = vec![Phase::early(), Phase::default_phase(), Phase::late()];
        let edges = vec![(0, 1), (1, 2)];
        Self {
            state: RwLock::new(State {
                phases,
                edges,
                listeners: Vec::new(),
                next_id: 0,
                snapshot: Arc::new(Vec::new()),
            }),
        }
    }

    pub fn register(&self, f: impl Fn(&mut C) -> R + Send + Sync + 'static) -> ListenerId {
        self.register_in(Phase::default_phase(), f)
    }

    pub fn register_in(
        &self,
        phase: impl Into<Phase>,
        f: impl Fn(&mut C) -> R + Send + Sync + 'static,
    ) -> ListenerId {
        let phase = phase.into();
        let mut s = self.state.write().unwrap_or_else(|e| e.into_inner());
        let phase = s.phase_index(&phase);
        let id = ListenerId(s.next_id);
        s.next_id += 1;
        s.listeners.push(Listener {
            id,
            phase,
            f: Arc::new(f),
        });
        s.rebuild();
        id
    }

    pub fn unregister(&self, id: ListenerId) -> bool {
        let mut s = self.state.write().unwrap_or_else(|e| e.into_inner());
        let before = s.listeners.len();
        s.listeners.retain(|l| l.id != id);
        let removed = s.listeners.len() != before;
        if removed {
            s.rebuild();
        }
        removed
    }

    pub fn add_phase_ordering(
        &self,
        first: impl Into<Phase>,
        second: impl Into<Phase>,
    ) -> Result<(), PhaseCycle> {
        let (first, second) = (first.into(), second.into());
        let mut s = self.state.write().unwrap_or_else(|e| e.into_inner());
        let a = s.phase_index(&first);
        let b = s.phase_index(&second);
        s.edges.push((a, b));
        if s.phase_order().is_none() {
            s.edges.pop();
            return Err(PhaseCycle {
                first: first.0,
                second: second.0,
            });
        }
        s.rebuild();
        Ok(())
    }

    pub fn listener_count(&self) -> usize {
        self.state
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .listeners
            .len()
    }

    fn snapshot(&self) -> Arc<Vec<Arc<Callback<C, R>>>> {
        self.state
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot
            .clone()
    }

    pub fn invoke_until(&self, ctx: &mut C, is_final: impl Fn(&R) -> bool) -> Option<R> {
        for f in self.snapshot().iter() {
            let r = f(ctx);
            if is_final(&r) {
                return Some(r);
            }
        }
        None
    }
}

impl<C> Event<C, ()> {
    pub fn invoke(&self, ctx: &mut C) {
        for f in self.snapshot().iter() {
            f(ctx);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActionResult {
    #[default]
    Pass,
    Success,
    Consume,
    Fail,
}

impl ActionResult {
    pub fn is_final(self) -> bool {
        self != Self::Pass
    }
}

impl<C> Event<C, ActionResult> {
    pub fn invoke_action(&self, ctx: &mut C) -> ActionResult {
        self.invoke_until(ctx, |r| r.is_final())
            .unwrap_or(ActionResult::Pass)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn log() -> Arc<Mutex<Vec<&'static str>>> {
        Arc::default()
    }

    #[test]
    fn runs_in_registration_order() {
        let ev: Event<i32> = Event::new();
        ev.register(|n| *n += 1);
        ev.register(|n| *n *= 10);
        let mut n = 1;
        ev.invoke(&mut n);
        assert_eq!(n, 20);
    }

    #[test]
    fn phases_order_listeners() {
        let ev: Event<()> = Event::new();
        let out = log();
        let (a, b, c) = (out.clone(), out.clone(), out.clone());
        ev.register_in(Phase::late(), move |_| a.lock().unwrap().push("late"));
        ev.register(move |_| b.lock().unwrap().push("default"));
        ev.register_in(Phase::early(), move |_| c.lock().unwrap().push("early"));
        ev.invoke(&mut ());
        assert_eq!(*out.lock().unwrap(), ["early", "default", "late"]);
    }

    #[test]
    fn custom_phase_ordering() {
        let ev: Event<()> = Event::new();
        let mine = Phase::named(Identifier::of("mymod", "mine"));
        ev.add_phase_ordering(Phase::default_phase(), mine.clone())
            .unwrap();
        ev.add_phase_ordering(mine.clone(), Phase::late()).unwrap();
        let out = log();
        let (a, b, c) = (out.clone(), out.clone(), out.clone());
        ev.register_in(Phase::late(), move |_| a.lock().unwrap().push("late"));
        ev.register_in(mine, move |_| b.lock().unwrap().push("mine"));
        ev.register(move |_| c.lock().unwrap().push("default"));
        ev.invoke(&mut ());
        assert_eq!(*out.lock().unwrap(), ["default", "mine", "late"]);
    }

    #[test]
    fn cycles_are_refused_and_leave_the_order_alone() {
        let ev: Event<()> = Event::new();
        let err = ev
            .add_phase_ordering(Phase::late(), Phase::early())
            .unwrap_err();
        assert_eq!(err.first, *Phase::late().id());
        ev.invoke(&mut ());
    }

    #[test]
    fn first_final_result_wins() {
        let ev: Event<u8, ActionResult> = Event::new();
        let out = log();
        let (a, b, c) = (out.clone(), out.clone(), out.clone());
        ev.register(move |_| {
            a.lock().unwrap().push("pass");
            ActionResult::Pass
        });
        ev.register(move |_| {
            b.lock().unwrap().push("fail");
            ActionResult::Fail
        });
        ev.register(move |_| {
            c.lock().unwrap().push("never");
            ActionResult::Success
        });
        assert_eq!(ev.invoke_action(&mut 0), ActionResult::Fail);
        assert_eq!(*out.lock().unwrap(), ["pass", "fail"]);
        assert_eq!(
            Event::<u8, ActionResult>::new().invoke_action(&mut 0),
            ActionResult::Pass
        );
    }

    #[test]
    fn unregister_and_reentrancy() {
        let ev: Arc<Event<i32>> = Arc::new(Event::new());
        let id = ev.register(|n| *n += 1);
        let inner = ev.clone();
        ev.register(move |_| {
            inner.register(|n| *n += 100);
        });
        let mut n = 0;
        ev.invoke(&mut n);
        assert_eq!(n, 1);
        assert!(ev.unregister(id));
        assert!(!ev.unregister(id));
        assert_eq!(ev.listener_count(), 2);
    }
}
