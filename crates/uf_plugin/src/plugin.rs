//! The trait a plugin implements, and the adapter that wires an existing crate
//! into the container without that crate having to know the trait exists.

use crate::descriptor::PluginDescriptor;
use crate::hook::PluginHook;
use crate::outcome::{
    HookFailure, HookOutcome, HookResult, LoadInput, ModuleCode, ResolveInput, ResolvedId,
    TransformInput,
};

/// One participant in the pipeline.
///
/// Every hook defaults to [`HookOutcome::Passthrough`], so a plugin implements
/// only what it cares about. The container never calls a hook the plugin's
/// descriptor did not declare, so the declared [`HookSet`](crate::HookSet) and
/// the implementation stay in step.
pub trait Plugin: Send + Sync {
    /// Name, order, apply condition, and declared hooks.
    fn descriptor(&self) -> &PluginDescriptor;

    /// Turn an import specifier into a module id. First-wins.
    fn resolve_id(&self, input: ResolveInput<'_>) -> HookResult<ResolvedId> {
        let _ = input;
        Ok(HookOutcome::Passthrough)
    }

    /// Produce the source text for a module id. First-wins.
    fn load(&self, input: LoadInput<'_>) -> HookResult<ModuleCode> {
        let _ = input;
        Ok(HookOutcome::Passthrough)
    }

    /// Rewrite a module's source text. Chained.
    fn transform(&self, input: TransformInput<'_>) -> HookResult<ModuleCode> {
        let _ = input;
        Ok(HookOutcome::Passthrough)
    }

    /// Observe a broadcast hook such as `BuildStart` or `WriteBundle`.
    fn notify(&self, hook: PluginHook) -> Result<(), HookFailure> {
        let _ = hook;
        Ok(())
    }
}

type ResolveIdFn = Box<dyn Fn(ResolveInput<'_>) -> HookResult<ResolvedId> + Send + Sync>;
type LoadFn = Box<dyn Fn(LoadInput<'_>) -> HookResult<ModuleCode> + Send + Sync>;
type TransformFn = Box<dyn Fn(TransformInput<'_>) -> HookResult<ModuleCode> + Send + Sync>;
type NotifyFn = Box<dyn Fn(PluginHook) -> Result<(), HookFailure> + Send + Sync>;

/// A plugin assembled from closures.
///
/// This is how uf's own stages become plugins without `uf_plugin` growing a
/// dependency on every crate that owns a transform: the crate that owns the
/// logic hands over a closure, and the container sees an ordinary [`Plugin`].
/// Registering a hook also records it in the descriptor, so a wired hook is
/// always a declared hook.
pub struct FnPlugin {
    descriptor: PluginDescriptor,
    resolve_id: Option<ResolveIdFn>,
    load: Option<LoadFn>,
    transform: Option<TransformFn>,
    notify: Option<NotifyFn>,
}

impl FnPlugin {
    /// An inert plugin: it occupies its place in the order and runs nothing.
    pub fn new(descriptor: PluginDescriptor) -> Self {
        Self {
            descriptor,
            resolve_id: None,
            load: None,
            transform: None,
            notify: None,
        }
    }

    /// Wire the `ResolveId` hook, declaring it on the descriptor.
    #[must_use]
    pub fn on_resolve_id(
        mut self,
        hook: impl Fn(ResolveInput<'_>) -> HookResult<ResolvedId> + Send + Sync + 'static,
    ) -> Self {
        self.descriptor.hooks = self.descriptor.hooks.with(PluginHook::ResolveId);
        self.resolve_id = Some(Box::new(hook));
        self
    }

    /// Wire the `Load` hook, declaring it on the descriptor.
    #[must_use]
    pub fn on_load(
        mut self,
        hook: impl Fn(LoadInput<'_>) -> HookResult<ModuleCode> + Send + Sync + 'static,
    ) -> Self {
        self.descriptor.hooks = self.descriptor.hooks.with(PluginHook::Load);
        self.load = Some(Box::new(hook));
        self
    }

    /// Wire the `Transform` hook, declaring it on the descriptor.
    #[must_use]
    pub fn on_transform(
        mut self,
        hook: impl Fn(TransformInput<'_>) -> HookResult<ModuleCode> + Send + Sync + 'static,
    ) -> Self {
        self.descriptor.hooks = self.descriptor.hooks.with(PluginHook::Transform);
        self.transform = Some(Box::new(hook));
        self
    }

    /// Wire the broadcast hooks. The hooks themselves stay as declared.
    #[must_use]
    pub fn on_notify(
        mut self,
        hook: impl Fn(PluginHook) -> Result<(), HookFailure> + Send + Sync + 'static,
    ) -> Self {
        self.notify = Some(Box::new(hook));
        self
    }
}

impl std::fmt::Debug for FnPlugin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FnPlugin")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

impl Plugin for FnPlugin {
    fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    fn resolve_id(&self, input: ResolveInput<'_>) -> HookResult<ResolvedId> {
        match &self.resolve_id {
            Some(hook) => hook(input),
            None => Ok(HookOutcome::Passthrough),
        }
    }

    fn load(&self, input: LoadInput<'_>) -> HookResult<ModuleCode> {
        match &self.load {
            Some(hook) => hook(input),
            None => Ok(HookOutcome::Passthrough),
        }
    }

    fn transform(&self, input: TransformInput<'_>) -> HookResult<ModuleCode> {
        match &self.transform {
            Some(hook) => hook(input),
            None => Ok(HookOutcome::Passthrough),
        }
    }

    fn notify(&self, hook: PluginHook) -> Result<(), HookFailure> {
        match &self.notify {
            Some(notify) => notify(hook),
            None => Ok(()),
        }
    }
}
