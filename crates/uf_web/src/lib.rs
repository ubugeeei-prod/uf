use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub type WebPrimitiveList = SmallVec<[WebPrimitive; 16]>;
pub type GuardList = SmallVec<[NavigationGuard; 8]>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebStdlibContract {
    pub primitives: WebPrimitiveList,
    pub route_hooks: RouteHookContract,
    pub head: HeadContract,
    pub cookies: CookieContract,
    pub cache: CacheContract,
    pub pwa: PwaContract,
}

impl Default for WebStdlibContract {
    fn default() -> Self {
        Self {
            primitives: smallvec::smallvec![
                WebPrimitive::Font,
                WebPrimitive::Image,
                WebPrimitive::OgImage,
                WebPrimitive::Link,
                WebPrimitive::Page,
                WebPrimitive::Layout,
                WebPrimitive::Time,
                WebPrimitive::Announcer,
                WebPrimitive::Picture,
                WebPrimitive::Table,
                WebPrimitive::Charts,
            ],
            route_hooks: RouteHookContract::default(),
            head: HeadContract::default(),
            cookies: CookieContract::default(),
            cache: CacheContract::default(),
            pwa: PwaContract::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WebPrimitive {
    Font,
    Image,
    OgImage,
    Link,
    Page,
    Layout,
    Time,
    Announcer,
    Picture,
    Table,
    Charts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteHookContract {
    pub use_route: CompactString,
    pub use_router: CompactString,
    pub fully_type_safe: bool,
    pub guards: GuardList,
}

impl Default for RouteHookContract {
    fn default() -> Self {
        Self {
            use_route: CompactString::const_new("useRoute"),
            use_router: CompactString::const_new("useRouter"),
            fully_type_safe: true,
            guards: smallvec::smallvec![
                NavigationGuard::BeforeNavigate,
                NavigationGuard::CanActivate
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NavigationGuard {
    BeforeNavigate,
    CanActivate,
    CanLeave,
    Middleware,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadContract {
    pub hook: CompactString,
    pub server_component_safe: bool,
    pub dedupe: bool,
}

impl Default for HeadContract {
    fn default() -> Self {
        Self {
            hook: CompactString::const_new("useHead"),
            server_component_safe: true,
            dedupe: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieContract {
    pub hook: CompactString,
    pub http_only_default: bool,
    pub same_site_default: SameSite,
}

impl Default for CookieContract {
    fn default() -> Self {
        Self {
            hook: CompactString::const_new("useCookie"),
            http_only_default: true,
            same_site_default: SameSite::Lax,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SameSite {
    Lax,
    Strict,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheContract {
    pub route: CacheMode,
    pub fetch: CacheMode,
    pub image: CacheMode,
    pub font: CacheMode,
    pub pwa: CacheMode,
}

impl Default for CacheContract {
    fn default() -> Self {
        Self {
            route: CacheMode::OptIn,
            fetch: CacheMode::OptIn,
            image: CacheMode::OptIn,
            font: CacheMode::OptIn,
            pwa: CacheMode::OptIn,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CacheMode {
    OptIn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PwaContract {
    pub enabled_by_default: bool,
    pub cache: CacheMode,
}

impl Default for PwaContract {
    fn default() -> Self {
        Self {
            enabled_by_default: false,
            cache: CacheMode::OptIn,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkOptions {
    pub prefetch: LinkPrefetch,
}

impl Default for LinkOptions {
    fn default() -> Self {
        Self {
            prefetch: LinkPrefetch::Intent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkPrefetch {
    Off,
    Intent,
    Render,
}

pub fn stdlib_contract() -> WebStdlibContract {
    WebStdlibContract::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_nuxt_like_web_primitives() {
        let contract = stdlib_contract();

        assert!(contract.primitives.contains(&WebPrimitive::Font));
        assert!(contract.primitives.contains(&WebPrimitive::Image));
        assert!(contract.primitives.contains(&WebPrimitive::OgImage));
        assert!(contract.primitives.contains(&WebPrimitive::Link));
        assert!(contract.primitives.contains(&WebPrimitive::Page));
        assert!(contract.primitives.contains(&WebPrimitive::Layout));
        assert!(contract.primitives.contains(&WebPrimitive::Time));
        assert!(contract.primitives.contains(&WebPrimitive::Announcer));
        assert!(contract.primitives.contains(&WebPrimitive::Picture));
    }

    #[test]
    fn route_hooks_are_type_safe_with_guards() {
        let contract = stdlib_contract();

        assert!(contract.route_hooks.fully_type_safe);
        assert!(
            contract
                .route_hooks
                .guards
                .contains(&NavigationGuard::BeforeNavigate)
        );
        assert!(
            contract
                .route_hooks
                .guards
                .contains(&NavigationGuard::CanActivate)
        );
    }

    #[test]
    fn cache_and_pwa_are_opt_in() {
        let contract = stdlib_contract();

        assert_eq!(contract.cache.route, CacheMode::OptIn);
        assert_eq!(contract.cache.fetch, CacheMode::OptIn);
        assert_eq!(contract.cache.pwa, CacheMode::OptIn);
        assert!(!contract.pwa.enabled_by_default);
    }

    #[test]
    fn link_prefetch_defaults_to_intent_not_global_cache() {
        let options = LinkOptions::default();

        assert_eq!(options.prefetch, LinkPrefetch::Intent);
    }
}
