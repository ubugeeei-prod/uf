use std::marker::PhantomData;

use compact_str::{CompactString, ToCompactString};
use uniflowed_infra::InlineVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EffectId(u32);

impl EffectId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Effect<T> {
    id: EffectId,
    _result: PhantomData<fn() -> T>,
}

impl<T> Copy for Effect<T> {}

impl<T> Clone for Effect<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Effect<T> {
    pub fn id(self) -> EffectId {
        self.id
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Task<T> {
    id: EffectId,
    _result: PhantomData<fn() -> T>,
}

impl<T> Copy for Task<T> {}

impl<T> Clone for Task<T> {
    fn clone(&self) -> Self {
        *self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectNode {
    Pure {
        label: CompactString,
    },
    Call {
        function: CompactString,
        args: InlineVec<CompactString, 4>,
    },
    Fork {
        effect: EffectId,
    },
    All {
        effects: Vec<EffectId>,
    },
    Race {
        effects: Vec<(CompactString, EffectId)>,
    },
    Take {
        channel: CompactString,
    },
    Put {
        channel: CompactString,
        payload: CompactString,
    },
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EffectGraph {
    nodes: Vec<EffectNode>,
}

impl EffectGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pure<T>(&mut self, label: impl ToCompactString) -> Effect<T> {
        self.push(EffectNode::Pure {
            label: label.to_compact_string(),
        })
    }

    pub fn call<T>(
        &mut self,
        function: impl ToCompactString,
        args: impl IntoIterator<Item = impl ToCompactString>,
    ) -> Effect<T> {
        self.push(EffectNode::Call {
            function: function.to_compact_string(),
            args: args
                .into_iter()
                .map(|arg| arg.to_compact_string())
                .collect(),
        })
    }

    pub fn fork<T>(&mut self, effect: Effect<T>) -> Effect<Task<T>> {
        self.push(EffectNode::Fork {
            effect: effect.id(),
        })
    }

    pub fn all<T>(&mut self, effects: impl IntoIterator<Item = Effect<T>>) -> Effect<Vec<T>> {
        self.push(EffectNode::All {
            effects: effects.into_iter().map(Effect::id).collect(),
        })
    }

    pub fn race<T>(
        &mut self,
        effects: impl IntoIterator<Item = (impl ToCompactString, Effect<T>)>,
    ) -> Effect<T> {
        self.push(EffectNode::Race {
            effects: effects
                .into_iter()
                .map(|(name, effect)| (name.to_compact_string(), effect.id()))
                .collect(),
        })
    }

    pub fn take<T>(&mut self, channel: impl ToCompactString) -> Effect<T> {
        self.push(EffectNode::Take {
            channel: channel.to_compact_string(),
        })
    }

    pub fn put<T>(
        &mut self,
        channel: impl ToCompactString,
        payload: impl ToCompactString,
    ) -> Effect<T> {
        self.push(EffectNode::Put {
            channel: channel.to_compact_string(),
            payload: payload.to_compact_string(),
        })
    }

    pub fn node(&self, id: EffectId) -> Option<&EffectNode> {
        self.nodes.get(id.index())
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn push<T>(&mut self, node: EffectNode) -> Effect<T> {
        let id = EffectId(self.nodes.len() as u32);
        self.nodes.push(node);
        Effect {
            id,
            _result: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_typed_call_nodes() {
        let mut graph = EffectGraph::new();
        let user: Effect<String> = graph.call("fetchUser", ["42"]);

        assert_eq!(user.id().index(), 0);
        assert_eq!(
            graph.node(user.id()),
            Some(&EffectNode::Call {
                function: "fetchUser".into(),
                args: vec!["42".into()].into_iter().collect()
            })
        );
    }

    #[test]
    fn all_preserves_result_collection_type() {
        let mut graph = EffectGraph::new();
        let a: Effect<u32> = graph.pure("a");
        let b: Effect<u32> = graph.pure("b");
        let all: Effect<Vec<u32>> = graph.all([a, b]);

        assert_eq!(all.id().index(), 2);
        assert_eq!(
            graph.node(all.id()),
            Some(&EffectNode::All {
                effects: vec![a.id(), b.id()]
            })
        );
    }

    #[test]
    fn fork_returns_a_typed_task_effect() {
        let mut graph = EffectGraph::new();
        let action: Effect<bool> = graph.take("ready");
        let task: Effect<Task<bool>> = graph.fork(action);

        assert_eq!(
            graph.node(task.id()),
            Some(&EffectNode::Fork {
                effect: action.id()
            })
        );
    }

    #[test]
    fn race_keeps_named_branches() {
        let mut graph = EffectGraph::new();
        let fast: Effect<&'static str> = graph.call("fast", std::iter::empty::<&str>());
        let slow: Effect<&'static str> = graph.call("slow", std::iter::empty::<&str>());
        let winner: Effect<&'static str> = graph.race([("fast", fast), ("slow", slow)]);

        assert_eq!(
            graph.node(winner.id()),
            Some(&EffectNode::Race {
                effects: vec![("fast".into(), fast.id()), ("slow".into(), slow.id())]
            })
        );
    }
}
